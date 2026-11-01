//! Piezas comunes del bundle OKF: incidencias, YAML, escritura del ZIP y validación.
use crate::format::Format;
use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::Path as FsPath,
};
use yaml_serde::Value as YamlValue;
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

/// `Loss` señala pérdida o incertidumbre sustantiva y vuelve `partial` el bundle;
/// `Note` se informa sin afectar el estado.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Severity {
    Loss,
    Note,
}

#[derive(Clone, Debug)]
pub(crate) struct Issue {
    pub(crate) severity: Severity,
    pub(crate) text: String,
}

impl Issue {
    pub(crate) fn loss(text: String) -> Self {
        Issue {
            severity: Severity::Loss,
            text,
        }
    }
    pub(crate) fn note(text: String) -> Self {
        Issue {
            severity: Severity::Note,
            text,
        }
    }
}

/// Comentario de revisión (formato clásico o moderno) con sus respuestas.
#[derive(Clone, Default, Debug)]
pub(crate) struct Comment {
    pub(crate) author: Option<String>,
    pub(crate) date: Option<String>,
    pub(crate) text: String,
    pub(crate) replies: Vec<Comment>,
}

/// Sección «Comentarios de revisión»: autor, fecha y texto, con las respuestas anidadas.
pub(crate) fn comments_markdown(comments: &[Comment]) -> String {
    fn line(c: &Comment, indent: &str) -> String {
        let mut head = format!(
            "{indent}- **{}**",
            escape_inline(c.author.as_deref().unwrap_or("Autor desconocido"))
        );
        if let Some(d) = &c.date {
            head.push_str(&format!(" · {d}"));
        }
        let text = c.text.split_whitespace().collect::<Vec<_>>().join(" ");
        let mut out = format!("{head}: {}\n", escape_inline(&text));
        for r in &c.replies {
            out.push_str(&line(r, &format!("{indent}  ")));
        }
        out
    }
    let mut out = "## Comentarios de revisión\n\n> Comentarios del archivo original; no forman parte del contenido presentado.\n\n".to_string();
    for c in comments {
        out.push_str(&line(c, ""));
    }
    out
}

/// «Bloque 4» o «Bloques 4–6, 11 y 19»: los tramos de tres o más unidades consecutivas
/// se abrevian.
pub(crate) fn units_phrase(singular: &str, plural: &str, ns: &[usize]) -> String {
    if let [n] = ns {
        return format!("{singular} {n}");
    }
    let mut parts = Vec::new();
    let mut start = 0;
    while start < ns.len() {
        let mut end = start;
        while end + 1 < ns.len() && ns[end + 1] == ns[end] + 1 {
            end += 1;
        }
        if end - start >= 2 {
            parts.push(format!("{}–{}", ns[start], ns[end]));
        } else {
            parts.extend(ns[start..=end].iter().map(usize::to_string));
        }
        start = end + 1;
    }
    let last = parts.pop().unwrap_or_default();
    if parts.is_empty() {
        format!("{plural} {last}")
    } else {
        format!("{plural} {} y {last}", parts.join(", "))
    }
}

pub(crate) fn table_markdown(rows: &[Vec<String>]) -> String {
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut md = String::new();
    for (row_index, row) in rows.iter().enumerate() {
        md.push('|');
        for col in 0..cols {
            md.push(' ');
            md.push_str(row.get(col).map(String::as_str).unwrap_or(""));
            md.push_str(" |")
        }
        md.push('\n');
        if row_index == 0 {
            md.push('|');
            for _ in 0..cols {
                md.push_str(" --- |")
            }
            md.push('\n');
        }
    }
    md
}

/// Recorta en un límite de palabra y marca el corte con «…».
pub(crate) fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    format!("{}…", cut.trim_end())
}

/// Escapa los caracteres con significado en línea para Markdown/CommonMark.
pub(crate) fn escape_inline(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\\' | '`' | '*' | '_' | '[' | ']' | '<') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Evita que una línea literal se interprete como encabezado, cita o lista.
pub(crate) fn escape_line_start(line: &str) -> String {
    let trimmed = line.trim_start();
    let indent = &line[..line.len() - trimmed.len()];
    if trimmed.starts_with(['#', '>', '-', '+', '=']) {
        return format!("{indent}\\{trimmed}");
    }
    let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 && matches!(trimmed[digits..].chars().next(), Some('.') | Some(')')) {
        return format!("{indent}{}\\{}", &trimmed[..digits], &trimmed[digits..]);
    }
    line.to_string()
}

pub(crate) fn hex(b: &[u8]) -> String {
    b.iter().map(|v| format!("{v:02x}")).collect()
}

pub(crate) fn write_zip_file(zip: &mut ZipWriter<fs::File>, name: &str, data: &[u8]) -> Result<()> {
    zip.start_file(name, SimpleFileOptions::default())?;
    zip.write_all(data)?;
    Ok(())
}

pub(crate) fn frontmatter(md: &str) -> Result<YamlValue> {
    if !md.starts_with("---\n") {
        bail!("concepto sin frontmatter")
    };
    let rest = &md[4..];
    let end = rest.find("\n---\n").context("frontmatter sin cierre")?;
    Ok(yaml_serde::from_str(&rest[..end])?)
}

/// Ruta normalizada dentro del bundle de un destino de enlace local; `None` para
/// enlaces externos o anclas del mismo documento.
pub(crate) fn local_target(entry: &str, target: &str) -> Option<String> {
    if target.starts_with("http://")
        || target.starts_with("https://")
        || target.starts_with("mailto:")
        || target.is_empty()
    {
        return None;
    }
    let t = target.split('#').next().unwrap_or("");
    if t.is_empty() {
        return None;
    }
    let raw = if t.starts_with('/') {
        t.trim_start_matches('/').to_string()
    } else {
        format!(
            "{}/{}",
            entry.rsplit_once('/').map(|x| x.0).unwrap_or(""),
            t
        )
    };
    let mut p = Vec::new();
    for c in raw.split('/') {
        match c {
            "" | "." => {}
            ".." => {
                p.pop();
            }
            x => p.push(x),
        }
    }
    Some(p.join("/"))
}

/// Marcador de pasaje del perfil: `<!-- okf-source-span: ID -->`.
const SPAN_MARKER: &str = "<!-- okf-source-span: ";

pub(crate) fn validate_bundle(path: &FsPath) -> Result<()> {
    let mut z = ZipArchive::new(fs::File::open(path)?)?;
    let names: std::collections::HashSet<String> = (0..z.len())
        .map(|i| z.by_index(i).map(|e| e.name().to_string()))
        .collect::<std::result::Result<_, _>>()?;
    // El original conservado determina el formato y sus comprobaciones propias.
    let originals: Vec<&String> = names
        .iter()
        .filter(|n| n.starts_with("references/original."))
        .collect();
    let format = match originals.as_slice() {
        [o] => o
            .rsplit_once('.')
            .and_then(|(_, ext)| Format::from_extension(ext))
            .with_context(|| format!("formato de original no admitido: {o}"))?,
        _ => bail!("el bundle debe contener exactamente un references/original.<extensión>"),
    };
    for req in [
        "index.md",
        "contenido/index.md",
        "documento.md",
        "conversion-report.md",
    ] {
        if !names.contains(req) {
            bail!("bundle no contiene {req}")
        }
    }
    let mut root = String::new();
    z.by_name("index.md")?.read_to_string(&mut root)?;
    let rootfm = frontmatter(&root)?;
    if rootfm.get("okf_version").and_then(YamlValue::as_str) != Some("0.2") {
        bail!("index.md no declara okf_version 0.2")
    }
    let mut linked = std::collections::HashSet::new();
    for name in names.iter().filter(|n| n.ends_with(".md")) {
        let mut md = String::new();
        z.by_name(name)?.read_to_string(&mut md)?;
        let mut span_ids: Vec<String> = Vec::new();
        let reserved = name == "index.md" || name == "contenido/index.md" || name == "log.md";
        if !reserved {
            let fm = frontmatter(&md)?;
            if fm
                .get("type")
                .and_then(YamlValue::as_str)
                .filter(|x| !x.trim().is_empty())
                .is_none()
            {
                bail!("concepto {name} carece de type")
            }
            if name != "conversion-report.md"
                && (fm.get("title").is_none()
                    || fm.get("description").is_none()
                    || fm.get("sources").is_none()
                    || fm.get("generated").is_none())
            {
                bail!("concepto {name} carece de metadatos requeridos")
            }
            let sources = fm
                .get("sources")
                .and_then(YamlValue::as_sequence)
                .map(Vec::as_slice)
                .unwrap_or_default();
            for source in sources {
                if let Some(resource) = source.get("resource").and_then(YamlValue::as_str) {
                    let resource = resource.trim_start_matches('/');
                    if !names.contains(resource) {
                        bail!("recurso de origen sin resolver en {name}: {resource}")
                    }
                }
            }
            // Cada localizador apunta a una fuente declarada en el mismo concepto.
            let source_ids: Vec<&str> = sources
                .iter()
                .filter_map(|s| s.get("id").and_then(YamlValue::as_str))
                .collect();
            span_ids = fm
                .get("source_spans")
                .and_then(YamlValue::as_sequence)
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .filter_map(|s| s.get("id").and_then(YamlValue::as_str).map(str::to_string))
                .collect();
            for span in fm
                .get("source_spans")
                .and_then(YamlValue::as_sequence)
                .map(Vec::as_slice)
                .unwrap_or_default()
            {
                let source = span.get("source_id").and_then(YamlValue::as_str);
                if !source.is_some_and(|s| source_ids.contains(&s)) {
                    bail!("source_spans de {name} cita una fuente no declarada: {source:?}")
                }
            }
        } else if name != "index.md" && md.starts_with("---\n") {
            bail!("{name} es un índice reservado y no admite frontmatter; solo el índice raíz")
        }
        let mut pos = 0;
        while let Some(start) = md[pos..].find("](") {
            let s = pos + start + 2;
            // `\](` es texto literal escapado, no un enlace.
            if md[..pos + start].ends_with('\\') {
                pos = s;
                continue;
            }
            // Un destino entre `<` y `>` puede contener paréntesis.
            let close = if md[s..].starts_with('<') {
                md[s..].find('>').map(|i| i + 1)
            } else {
                md[s..].find(')')
            };
            if let Some(end) = close {
                let target = md[s..s + end]
                    .trim()
                    .trim_start_matches('<')
                    .trim_end_matches('>');
                if let Some(t) = local_target(name, target) {
                    if !names.contains(&t) {
                        bail!("enlace interno sin resolver en {name}: {target}")
                    }
                    linked.insert(t);
                }
                pos = s + end + 1
            } else {
                break;
            }
        }
        // Cada marcador de pasaje resuelve a un localizador del mismo concepto.
        for (i, _) in md.match_indices(SPAN_MARKER) {
            let rest = &md[i + SPAN_MARKER.len()..];
            let id = rest.split(" -->").next().unwrap_or_default().trim();
            if !span_ids.iter().any(|s| s == id) {
                bail!("marcador de pasaje sin localizador en {name}: {id}")
            }
            // El marcador corresponde al bloque siguiente, que debe existir.
            let next = rest
                .split_once(" -->\n")
                .map(|(_, after)| after.lines().next().unwrap_or_default())
                .unwrap_or_default();
            if next.trim().is_empty() || next.starts_with(SPAN_MARKER) {
                bail!("marcador de pasaje sin bloque a continuación en {name}: {id}")
            }
        }
    }
    // Toda imagen o archivo de datos adjunto se enlaza desde algún concepto.
    if let Some(orphan) = names
        .iter()
        .find(|n| n.starts_with("assets/") && !n.ends_with('/') && !linked.contains(*n))
    {
        bail!("el recurso {orphan} no está enlazado desde ningún concepto")
    }
    validate_against_original(&mut z, &names, format)
}

/// Integridad frente al original: la huella SHA-256 del informe y de `documento.md`,
/// y después las comprobaciones propias del formato.
pub(crate) fn validate_against_original(
    z: &mut ZipArchive<fs::File>,
    names: &std::collections::HashSet<String>,
    format: Format,
) -> Result<()> {
    let entry = format.original_entry();
    let mut original = Vec::new();
    z.by_name(&entry)?.read_to_end(&mut original)?;
    let hash = hex(&Sha256::digest(&original));
    let report = read_text(z, "conversion-report.md")?;
    if frontmatter(&report)?
        .get("sha256")
        .and_then(YamlValue::as_str)
        != Some(hash.as_str())
    {
        bail!("el SHA-256 del informe no coincide con {entry}")
    }
    let doc_hash = frontmatter(&read_text(z, "documento.md")?)?
        .get("source_file")
        .and_then(|f| f.get("sha256"))
        .and_then(YamlValue::as_str)
        .map(str::to_string);
    if doc_hash.as_deref() != Some(hash.as_str()) {
        bail!("el SHA-256 de documento.md no coincide con {entry}")
    }
    format.validate(z, names, &original)
}
/// Contenido de texto de una entrada del bundle.
pub(crate) fn read_text(z: &mut ZipArchive<fs::File>, name: &str) -> Result<String> {
    let mut s = String::new();
    z.by_name(name)?.read_to_string(&mut s)?;
    Ok(s)
}
pub(crate) fn chrono_now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}
