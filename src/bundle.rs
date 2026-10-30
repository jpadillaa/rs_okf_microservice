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

pub(crate) fn esc(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', " ")
}

pub(crate) fn yaml(s: &str) -> String {
    format!("\"{}\"", esc(s))
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

pub(crate) fn local_link_exists(
    entry: &str,
    target: &str,
    entries: &std::collections::HashSet<String>,
) -> bool {
    if target.starts_with("http://")
        || target.starts_with("https://")
        || target.starts_with("mailto:")
        || target.is_empty()
    {
        return true;
    }
    let t = target.split('#').next().unwrap_or("");
    if t.is_empty() {
        return true;
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
    entries.contains(&p.join("/"))
}

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
    for name in names.iter().filter(|n| n.ends_with(".md")) {
        let mut md = String::new();
        z.by_name(name)?.read_to_string(&mut md)?;
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
                if !local_link_exists(name, target, &names) {
                    bail!("enlace interno sin resolver en {name}: {target}")
                }
                pos = s + end + 1
            } else {
                break;
            }
        }
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
