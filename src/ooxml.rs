//! Lectura segura de paquetes OOXML (PPTX, DOCX, XLSX): límites del ZIP, partes,
//! relaciones y atributos.
use anyhow::{Context, Result, bail};
use std::{
    collections::HashMap,
    io::{Cursor, Read},
};
use zip::ZipArchive;

pub(crate) const MAX_ZIP_ENTRY: u64 = 128 * 1024 * 1024;

pub(crate) const MAX_ZIP_TOTAL: u64 = 512 * 1024 * 1024;

pub(crate) const MAX_ZIP_ENTRIES: usize = 4096;

/// Texto de un gráfico a partir de su caché: el título y, por serie, su nombre y los
/// pares «categoría = valor». Las fórmulas (`c:f`) y los códigos de formato no son
/// contenido. Vacío si la caché no tiene valores.
pub(crate) fn chart_text(xml: &str) -> String {
    #[derive(Default)]
    struct Series {
        name: String,
        cats: Vec<String>,
        vals: Vec<String>,
    }
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut stack: Vec<Vec<u8>> = Vec::new();
    let mut title = String::new();
    let mut series: Vec<Series> = Vec::new();
    let mut loose = Vec::new();
    let mut text = String::new();
    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Start(e)) => {
                let name = e.local_name().as_ref().to_vec();
                if name == b"ser" {
                    series.push(Series::default());
                }
                text.clear();
                stack.push(name);
            }
            Ok(quick_xml::events::Event::Text(t)) => {
                if let Ok(s) = t.decode() {
                    text.push_str(&s);
                }
            }
            Ok(quick_xml::events::Event::GeneralRef(r)) => {
                if let Ok(s) = resolve_ref(&r) {
                    text.push_str(&s);
                }
            }
            Ok(quick_xml::events::Event::End(_)) => {
                let name = stack.pop().unwrap_or_default();
                let value = text.trim().to_string();
                text.clear();
                let inside = |n: &[u8]| stack.iter().any(|s| s == n);
                if name == b"t" && inside(b"title") && !inside(b"ser") {
                    if !title.is_empty() && !value.is_empty() {
                        title.push(' ');
                    }
                    title.push_str(&value);
                } else if name == b"v" && !value.is_empty() {
                    match series.last_mut().filter(|_| inside(b"ser")) {
                        Some(s) if inside(b"tx") => s.name = value,
                        Some(s) if inside(b"cat") || inside(b"xVal") => s.cats.push(value),
                        Some(s) if inside(b"val") || inside(b"yVal") => s.vals.push(value),
                        _ => loose.push(value),
                    }
                }
            }
            Ok(quick_xml::events::Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    let mut parts = Vec::new();
    for (i, s) in series
        .iter()
        .enumerate()
        .filter(|(_, s)| !s.vals.is_empty())
    {
        let label = if s.name.is_empty() {
            format!("Serie {}", i + 1)
        } else {
            s.name.clone()
        };
        let pairs: Vec<String> = if s.cats.len() == s.vals.len() {
            s.cats
                .iter()
                .zip(&s.vals)
                .map(|(c, v)| format!("{c} = {v}"))
                .collect()
        } else {
            s.vals.clone()
        };
        parts.push(format!("{label}: {}", pairs.join("; ")));
    }
    if parts.is_empty() && series.iter().all(|s| s.vals.is_empty()) {
        parts.extend(loose);
        if parts.is_empty() {
            return String::new();
        }
    }
    if !title.is_empty() {
        parts.insert(0, title);
    }
    parts.join(". ")
}

pub(crate) fn attr<'a>(e: &'a quick_xml::events::BytesStart<'a>, name: &[u8]) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.local_name().as_ref() == name)
        .and_then(|a| String::from_utf8(a.value.into_owned()).ok())
}

pub(crate) fn read_entry(z: &mut ZipArchive<Cursor<&[u8]>>, path: &str) -> Result<Vec<u8>> {
    let mut f = z
        .by_name(path)
        .with_context(|| format!("falta la parte {path} del paquete"))?;
    if f.size() > MAX_ZIP_ENTRY {
        bail!("una parte del paquete excede el límite de expansión");
    }
    let mut b = Vec::new();
    f.read_to_end(&mut b)?;
    Ok(b)
}

pub(crate) fn enforce_package_limits(z: &mut ZipArchive<Cursor<&[u8]>>) -> Result<()> {
    if z.len() > MAX_ZIP_ENTRIES {
        bail!("el paquete contiene demasiadas partes OOXML")
    }
    let mut expanded = 0u64;
    for i in 0..z.len() {
        let f = z.by_index(i)?;
        if f.size() > MAX_ZIP_ENTRY {
            bail!("una parte del paquete excede el límite de expansión")
        }
        expanded = expanded.saturating_add(f.size());
        if expanded > MAX_ZIP_TOTAL {
            bail!("la expansión total del paquete excede 512 MiB")
        }
        if f.size() > 1024 * 1024 && f.compressed_size() > 0 && f.size() / f.compressed_size() > 250
        {
            bail!("la relación de compresión del paquete excede el límite permitido")
        }
    }
    Ok(())
}

pub(crate) fn relationship_map(xml: &str) -> HashMap<String, String> {
    let mut r = quick_xml::Reader::from_str(xml);
    let mut map = HashMap::new();
    loop {
        match r.read_event() {
            Ok(quick_xml::events::Event::Start(e)) | Ok(quick_xml::events::Event::Empty(e))
                if e.local_name().as_ref() == b"Relationship" =>
            {
                if let (Some(id), Some(target)) = (attr(&e, b"Id"), attr(&e, b"Target")) {
                    map.insert(id, target);
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    map
}

pub(crate) fn resolve(base: &str, target: &str) -> String {
    let raw = if target.starts_with('/') {
        target.trim_start_matches('/').to_string()
    } else {
        format!(
            "{}/{}",
            base.rsplit_once('/').map(|x| x.0).unwrap_or(""),
            target
        )
    };
    let mut parts = Vec::new();
    for p in raw.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            x => parts.push(x),
        }
    }
    parts.join("/")
}

pub(crate) fn meaningful_alt(descr: Option<&str>) -> Option<String> {
    let d = descr?.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = d.to_ascii_lowercase();
    let filename = [
        ".png", ".jpg", ".jpeg", ".gif", ".svg", ".emf", ".wmf", ".bmp", ".tif", ".tiff", ".webp",
    ]
    .iter()
    .any(|ext| lower.ends_with(ext));
    (!d.is_empty() && !filename).then_some(d)
}

/// Atributo del elemento raíz de una parte XML (`show`, `showMasterSp`).
pub(crate) fn root_attr(xml: &str, name: &[u8]) -> Option<String> {
    let mut reader = quick_xml::Reader::from_str(xml);
    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Start(e)) | Ok(quick_xml::events::Event::Empty(e)) => {
                return attr(&e, name);
            }
            Ok(quick_xml::events::Event::Eof) | Err(_) => return None,
            _ => {}
        }
    }
}

/// Texto de una referencia de carácter o entidad XML predefinida.
pub(crate) fn resolve_ref(r: &quick_xml::events::BytesRef<'_>) -> anyhow::Result<String> {
    Ok(match r.resolve_char_ref()? {
        Some(c) => c.to_string(),
        None => {
            let name = r.decode()?;
            quick_xml::escape::resolve_predefined_entity(&name)
                .map(str::to_string)
                .unwrap_or_else(|| format!("&{name};"))
        }
    })
}

/// Relación de una parte del paquete.
pub(crate) struct Rel {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) target: String,
    pub(crate) external: bool,
}

pub(crate) fn rels_path(part: &str) -> String {
    match part.rsplit_once('/') {
        Some((dir, file)) => format!("{dir}/_rels/{file}.rels"),
        None => format!("_rels/{part}.rels"),
    }
}

pub(crate) fn parse_entry(
    z: &mut ZipArchive<Cursor<&[u8]>>,
    path: &str,
) -> Option<crate::xml::Node> {
    let bytes = read_entry(z, path).ok()?;
    crate::xml::parse(&String::from_utf8_lossy(&bytes)).ok()
}

pub(crate) fn relationships(z: &mut ZipArchive<Cursor<&[u8]>>, part: &str) -> Vec<Rel> {
    let Some(root) = parse_entry(z, &rels_path(part)) else {
        return Vec::new();
    };
    root.nodes()
        .filter(|r| r.is(b"Relationship"))
        .filter_map(|r| {
            Some(Rel {
                id: r.attr(b"Id")?.to_string(),
                kind: r.attr(b"Type")?.to_string(),
                target: r.attr(b"Target")?.to_string(),
                external: r.attr(b"TargetMode") == Some("External"),
            })
        })
        .collect()
}

pub(crate) fn rel_map(rels: &[Rel]) -> HashMap<String, String> {
    rels.iter()
        .map(|r| (r.id.clone(), r.target.clone()))
        .collect()
}

/// Partes relacionadas con `part` cuyo tipo termina en `suffix` (p. ej. `/styles`).
pub(crate) fn related(rels: &[Rel], part: &str, suffix: &str) -> Vec<String> {
    rels.iter()
        .filter(|r| !r.external && r.kind.ends_with(suffix))
        .map(|r| resolve(part, &r.target))
        .collect()
}

/// Parte principal del paquete (documento, libro o presentación).
pub(crate) fn main_part(z: &mut ZipArchive<Cursor<&[u8]>>) -> Option<String> {
    relationships(z, "")
        .iter()
        .find(|r| r.kind.ends_with("/officeDocument"))
        .map(|r| resolve("", &r.target))
}
