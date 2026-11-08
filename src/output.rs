//! Modelo de salida común y ensamblador único (plan 1, R1). Cada adaptador conserva su
//! modelo de lectura y entrega conceptos, recursos e incidencias con estos tipos; aquí se
//! escriben en un solo sitio el frontmatter, `documento.md`, los índices, el informe y el
//! original, y se valida el bundle antes de devolverlo.
//!
//! El frontmatter se serializa desde un árbol tipado. Todas las cadenas van entre comillas
//! dobles con sus caracteres no imprimibles escapados: un analizador YAML 1.1 leería como
//! fecha, booleano o número una cadena sin comillas como `2026-11-01T10:00:00Z`, `yes` o
//! `12e3`, que la serialización por omisión de `yaml_serde` deja sin comillas.
use crate::bundle::{Issue, Severity, escape_inline, validate_bundle, write_zip_file};
use crate::format::Format;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use zip::ZipWriter;

/// Valor del frontmatter.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Yaml {
    Str(String),
    Int(i64),
    Bool(bool),
    List(Vec<Yaml>),
    Map(Fields),
}

impl From<&str> for Yaml {
    fn from(s: &str) -> Self {
        Yaml::Str(s.to_string())
    }
}
impl From<String> for Yaml {
    fn from(s: String) -> Self {
        Yaml::Str(s)
    }
}
impl From<&String> for Yaml {
    fn from(s: &String) -> Self {
        Yaml::Str(s.clone())
    }
}
impl From<bool> for Yaml {
    fn from(b: bool) -> Self {
        Yaml::Bool(b)
    }
}
macro_rules! from_int {
    ($($t:ty),*) => {$(
        impl From<$t> for Yaml {
            fn from(n: $t) -> Self {
                Yaml::Int(i64::try_from(n).unwrap_or(i64::MAX))
            }
        }
    )*};
}
from_int!(i64, u32, u64, usize);
impl From<Fields> for Yaml {
    fn from(f: Fields) -> Self {
        Yaml::Map(f)
    }
}
impl<T: Into<Yaml>> From<Vec<T>> for Yaml {
    fn from(v: Vec<T>) -> Self {
        Yaml::List(v.into_iter().map(Into::into).collect())
    }
}

/// Campos de un mapa YAML en el orden en que se escriben.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Fields(Vec<(String, Yaml)>);

impl Fields {
    pub(crate) fn new() -> Self {
        Fields::default()
    }
    /// Añade un campo.
    pub(crate) fn with(mut self, key: &str, value: impl Into<Yaml>) -> Self {
        self.0.push((key.to_string(), value.into()));
        self
    }
    /// Añade un campo si hay valor.
    pub(crate) fn with_opt(self, key: &str, value: Option<impl Into<Yaml>>) -> Self {
        match value {
            Some(v) => self.with(key, v),
            None => self,
        }
    }
    /// Añade `key: true` si se cumple la condición.
    pub(crate) fn flag(self, key: &str, on: bool) -> Self {
        if on { self.with(key, true) } else { self }
    }
    /// Inserta un campo detrás de `after`, o al principio si no está.
    pub(crate) fn insert_after(mut self, after: &str, key: &str, value: impl Into<Yaml>) -> Self {
        let at = self
            .0
            .iter()
            .position(|(k, _)| k == after)
            .map_or(0, |i| i + 1);
        self.0.insert(at, (key.to_string(), value.into()));
        self
    }
    pub(crate) fn push(&mut self, key: &str, value: impl Into<Yaml>) {
        self.0.push((key.to_string(), value.into()));
    }
    /// El frontmatter completo, con sus delimitadores y una línea en blanco detrás.
    pub(crate) fn frontmatter(&self) -> String {
        let mut out = String::from("---\n");
        emit_map(&self.0, 0, &mut out);
        out.push_str("---\n\n");
        out
    }
}

/// Cadena YAML entre comillas dobles. Se escapan las comillas, la barra inversa y todo
/// carácter que YAML no admite literalmente (controles, `DEL`, C1, separadores de línea
/// Unicode y no caracteres).
fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20
                || ('\u{7f}'..='\u{9f}').contains(&c)
                || matches!(
                    c,
                    '\u{2028}' | '\u{2029}' | '\u{feff}' | '\u{fffe}' | '\u{ffff}'
                ) =>
            {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Clave sin comillas si es un identificador; entre comillas en otro caso.
fn key(k: &str) -> String {
    let plain = k
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if plain { k.to_string() } else { quote(k) }
}

fn scalar(v: &Yaml) -> Option<String> {
    match v {
        Yaml::Str(s) => Some(quote(s)),
        Yaml::Int(n) => Some(n.to_string()),
        Yaml::Bool(b) => Some(b.to_string()),
        Yaml::List(l) if l.is_empty() => Some("[]".into()),
        Yaml::Map(m) if m.0.is_empty() => Some("{}".into()),
        _ => None,
    }
}

fn emit_map(entries: &[(String, Yaml)], indent: usize, out: &mut String) {
    let pad = " ".repeat(indent);
    for (k, v) in entries {
        match (scalar(v), v) {
            (Some(s), _) => out.push_str(&format!("{pad}{}: {s}\n", key(k))),
            (None, Yaml::Map(m)) => {
                out.push_str(&format!("{pad}{}:\n", key(k)));
                emit_map(&m.0, indent + 2, out);
            }
            (None, Yaml::List(items)) => {
                out.push_str(&format!("{pad}{}:\n", key(k)));
                for item in items {
                    emit_item(item, indent + 2, out);
                }
            }
            (None, _) => unreachable!("los escalares tienen representación"),
        }
    }
}

fn emit_item(item: &Yaml, indent: usize, out: &mut String) {
    let pad = " ".repeat(indent);
    match (scalar(item), item) {
        (Some(s), _) => out.push_str(&format!("{pad}- {s}\n")),
        (None, Yaml::Map(m)) => {
            // El primer campo va en la línea del guion; los demás, alineados con él.
            let mut nested = String::new();
            emit_map(&m.0, indent + 2, &mut nested);
            out.push_str(&pad);
            out.push_str("- ");
            out.push_str(&nested[indent + 2..]);
        }
        (None, Yaml::List(items)) => {
            out.push_str(&format!("{pad}-\n"));
            for i in items {
                emit_item(i, indent + 2, out);
            }
        }
        (None, _) => unreachable!("los escalares tienen representación"),
    }
}

/// Campos comunes de un concepto y de `documento.md`: tipo, título, descripción, fuente
/// y generación.
pub(crate) fn concept_fields(
    format: Format,
    kind: &str,
    title: &str,
    description: &str,
    generated: &str,
) -> Fields {
    Fields::new()
        .with("type", kind)
        .with("title", title)
        .with("description", description)
        .with(
            "sources",
            vec![
                Fields::new()
                    .with("id", "original")
                    .with("resource", format!("/{}", format.original_entry())),
            ],
        )
        .with("generated", generated_fields(generated))
}

fn generated_fields(generated: &str) -> Fields {
    Fields::new()
        .with("by", "process:document-conversion")
        .with("at", generated)
}

/// Tramo de procedencia (`source_spans`) con su identificador y tipo de localizador.
pub(crate) fn span(id: &str, kind: &str) -> Fields {
    Fields::new()
        .with("id", id)
        .with("source_id", "original")
        .with("kind", kind)
}

/// Campos de `documento.md`: los comunes, la configuración y la huella del original.
pub(crate) fn document_fields(
    format: Format,
    title: &str,
    description: &str,
    generated: &str,
    source: &crate::identity::Source,
    settings: &crate::settings::Settings,
) -> Fields {
    crate::identity::annotate(
        crate::version::Versions::annotate(concept_fields(
            format,
            "Source Document",
            title,
            description,
            generated,
        )),
        source,
        settings,
    )
    .with(
        "source_file",
        Fields::new()
            .with("path", format.original_entry())
            .with("sha256", &source.sha256),
    )
}

/// Estado del bundle: `partial` si hay alguna pérdida o incertidumbre sustantiva.
pub(crate) fn state(issues: &[Issue]) -> &'static str {
    if issues.iter().any(|i| i.severity == Severity::Loss) {
        "partial"
    } else {
        "complete"
    }
}

/// Informe de conversión: frontmatter, resumen del adaptador (líneas `- …` sin el
/// estado), secciones propias del formato e incidencias separadas en pérdidas y
/// observaciones.
pub(crate) fn report(
    format: Format,
    generated: &str,
    source: &crate::identity::Source,
    issues: &[Issue],
    summary: &[String],
    sections: &str,
    settings: &crate::settings::Settings,
) -> String {
    let state = state(issues);
    let id = crate::identity::conversion_id(&source.sha256, settings);
    let fields = crate::identity::annotate(
        crate::version::Versions::annotate(Fields::new()),
        source,
        settings,
    )
    .with("type", "Conversion Report")
    .with(
        "unit_id",
        crate::identity::unit_id(&id, "conversion-report.md"),
    )
    .with("title", "Informe de conversión")
    .with("generated", generated_fields(generated))
    .with("status", state)
    .with("format", format.extension())
    .with("sha256", &source.sha256);
    let list = |severity: Severity, empty: &str| -> String {
        let items: String = issues
            .iter()
            .filter(|i| i.severity == severity)
            .map(|i| format!("- {}\n", escape_inline(&i.text)))
            .collect();
        if items.is_empty() {
            format!("{empty}\n")
        } else {
            items
        }
    };
    let mut out = fields.frontmatter();
    out.push_str(&format!(
        "# Informe de conversión {}\n\n- Estado: `{state}`\n",
        format.extension().to_uppercase()
    ));
    for line in summary {
        out.push_str(&format!("- {line}\n"));
    }
    out.push('\n');
    out.push_str(sections);
    out.push_str("## Incidencias\n\nPérdidas o incertidumbres sustantivas; determinan el estado `partial`.\n\n");
    out.push_str(&list(
        Severity::Loss,
        "Sin pérdidas ni incertidumbres sustantivas.",
    ));
    out.push_str(
        "\n## Observaciones\n\nInformación sobre la conversión que no afecta el estado.\n\n",
    );
    out.push_str(&list(Severity::Note, "Sin observaciones."));
    out
}

/// Duración de las etapas de una conversión, en milisegundos. En XLSX y PDF las hojas y
/// las páginas se leen mientras se ensamblan, de modo que su lectura cuenta como
/// ensamblado y `read_ms` solo cubre la apertura del archivo.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Stages {
    pub read_ms: u64,
    pub assemble_ms: u64,
    pub validate_ms: u64,
}

/// Bundle validado de un intento: ruta, estado y duración de sus etapas.
#[derive(Debug)]
pub(crate) struct Built {
    pub(crate) path: PathBuf,
    pub(crate) status: String,
    pub(crate) stages: Stages,
    /// Identificador de la conversión (plan 3, I1).
    pub(crate) conversion_id: String,
}

fn ms(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

/// Escritor del bundle de un intento. Los adaptadores añaden recursos y conceptos a
/// medida que los producen, sin acumularlos; `finish` escribe el resto y valida.
pub(crate) struct BundleWriter {
    format: Format,
    path: PathBuf,
    zip: ZipWriter<fs::File>,
    /// Comienzo del ensamblado: lo anterior a crear el escritor es lectura.
    created: Instant,
    /// Prefijo del `unit_id` de cada concepto.
    conversion_id: String,
}

/// Navegación del bundle: el rótulo del índice de contenido, el título de la lista y sus
/// entradas desde la raíz y desde `contenido/`.
pub(crate) struct Navigation {
    pub(crate) contents_label: &'static str,
    pub(crate) heading: &'static str,
    pub(crate) root_entries: String,
    pub(crate) contents_entries: String,
}

impl BundleWriter {
    /// Escritor del bundle del original `source`, convertido con `settings`.
    pub(crate) fn create(
        format: Format,
        attempt: &Path,
        source: &crate::identity::Source,
        settings: &crate::settings::Settings,
    ) -> Result<Self> {
        let path = attempt.join("bundle.zip");
        let zip = ZipWriter::new(fs::File::create(&path)?);
        Ok(BundleWriter {
            format,
            path,
            zip,
            created: Instant::now(),
            conversion_id: crate::identity::conversion_id(&source.sha256, settings),
        })
    }
    /// Escritor del ZIP, para los recursos que el adaptador escribe en streaming.
    pub(crate) fn zip(&mut self) -> &mut ZipWriter<fs::File> {
        &mut self.zip
    }
    pub(crate) fn file(&mut self, name: &str, data: &[u8]) -> Result<()> {
        write_zip_file(&mut self.zip, name, data)
    }
    /// Concepto: frontmatter, con su `unit_id` detrás de `type`, y cuerpo Markdown.
    pub(crate) fn concept(&mut self, path: &str, fields: &Fields, body: &str) -> Result<()> {
        let fields = fields.clone().insert_after(
            "type",
            "unit_id",
            crate::identity::unit_id(&self.conversion_id, path),
        );
        let mut md = fields.frontmatter();
        md.push_str(body);
        self.file(path, md.as_bytes())
    }
    /// Escribe el original, `documento.md`, los índices y el informe, cierra el ZIP y lo
    /// valida.
    pub(crate) fn finish(
        mut self,
        original: &[u8],
        document: (&Fields, &str),
        nav: &Navigation,
        report: &str,
        issues: &[Issue],
    ) -> Result<Built> {
        let original_entry = self.format.original_entry();
        self.file(&original_entry, original)?;
        self.concept("documento.md", document.0, document.1)?;
        let index = format!(
            "{}# Bundle OKF\n\n- [Documento fuente](documento.md)\n- [{}](contenido/index.md)\n- [Informe de conversión](conversion-report.md)\n\n## {}\n\n{}",
            Fields::new().with("okf_version", "0.2").frontmatter(),
            nav.contents_label,
            nav.heading,
            nav.root_entries
        );
        self.file("index.md", index.as_bytes())?;
        let contents = format!(
            "# Contenido\n\n## {}\n\n{}",
            nav.heading, nav.contents_entries
        );
        self.file("contenido/index.md", contents.as_bytes())?;
        self.file("conversion-report.md", report.as_bytes())?;
        self.zip.finish()?;
        let assembled = Instant::now();
        validate_bundle(&self.path)?;
        Ok(Built {
            path: self.path,
            status: state(issues).into(),
            conversion_id: self.conversion_id,
            stages: Stages {
                read_ms: 0,
                assemble_ms: ms(assembled - self.created),
                validate_ms: ms(assembled.elapsed()),
            },
        })
    }
}

/// Empaqueta un directorio para el validador independiente sin seguir enlaces.
/// Toda escritura de bundles, también esta copia temporal, pasa por el ensamblador.
pub(crate) fn pack_directory(
    root: &std::path::Path,
    target: &std::path::Path,
) -> anyhow::Result<()> {
    use anyhow::{Context, bail};
    use std::{fs, io::Read, path::Path};
    use zip::{ZipWriter, write::SimpleFileOptions};
    fn walk(
        root: &Path,
        dir: &Path,
        zip: &mut ZipWriter<fs::File>,
        count: &mut usize,
        total: &mut u64,
    ) -> anyhow::Result<()> {
        let limits = crate::ooxml::limits();
        let mut entries = fs::read_dir(dir)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let ty = entry.file_type()?;
            if ty.is_dir() {
                walk(root, &entry.path(), zip, count, total)?;
            } else if ty.is_file() {
                *count += 1;
                if *count > limits.entries {
                    bail!("demasiados archivos en el bundle")
                }
                let size = entry.metadata()?.len();
                *total = total
                    .checked_add(size)
                    .context("tamaño del bundle desbordado")?;
                if size > limits.entry_bytes()
                    || *total > limits.total_mb.saturating_mul(1024 * 1024)
                {
                    bail!("bundle demasiado grande")
                }
                let path = entry.path();
                let name = path.strip_prefix(root)?.to_str().context("ruta no UTF-8")?;
                if !crate::validate::safe_name(name) {
                    bail!("ruta insegura: {name}")
                }
                zip.start_file(
                    name,
                    SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Deflated),
                )?;
                // NOFOLLOW evita seguir un enlace si el archivo cambia tras read_dir.
                use std::os::unix::fs::OpenOptionsExt;
                let file = fs::OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_NOFOLLOW)
                    .open(&path)?;
                let read = std::io::copy(&mut file.take(limits.entry_bytes() + 1), zip)?;
                if read != size {
                    bail!("el archivo cambió mientras se validaba: {name}")
                }
            } else {
                bail!("el bundle solo admite archivos regulares y directorios")
            }
        }
        Ok(())
    }
    let mut zip = ZipWriter::new(fs::File::create(target)?);
    walk(root, root, &mut zip, &mut 0, &mut 0)?;
    zip.finish()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use yaml_serde::Value;

    #[test]
    fn frontmatter_round_trips_through_a_yaml_parser() {
        let tricky = "Título: «x» #1 \"citado\" \\ barra\nsegunda\u{b}\u{85}\u{2028} yes";
        let fields = concept_fields(
            Format::Pdf,
            "Source Section",
            tricky,
            "12e3",
            "2026-11-01T10:00:00Z",
        )
        .with(
            "source_spans",
            vec![
                span("seccion", "pdf_pages")
                    .with("from", 1u32)
                    .with("to", 3u32),
            ],
        )
        .with("page_labels", Fields::new().with("1", "i").with("2", "ii"))
        .with("empty", Vec::<String>::new())
        .flag("title_inferred", true)
        .flag("visual_only", false);
        let md = fields.frontmatter();
        assert!(md.starts_with("---\ntype: \"Source Section\"\n"));
        assert!(md.contains(
            "sources:\n  - id: \"original\"\n    resource: \"/references/original.pdf\"\n"
        ));
        assert!(!md.contains("visual_only"));
        let value = crate::bundle::frontmatter(&md).unwrap();
        let get = |k: &str| value.get(k).cloned().unwrap();
        assert_eq!(get("title"), Value::String(tricky.into()));
        assert_eq!(get("description"), Value::String("12e3".into()));
        assert_eq!(
            value["generated"]["at"],
            Value::String("2026-11-01T10:00:00Z".into())
        );
        assert_eq!(value["source_spans"][0]["to"], Value::Number(3.into()));
        assert_eq!(value["page_labels"]["1"], Value::String("i".into()));
        assert_eq!(get("title_inferred"), Value::Bool(true));
        assert_eq!(get("empty"), Value::Sequence(vec![]));
    }
}
