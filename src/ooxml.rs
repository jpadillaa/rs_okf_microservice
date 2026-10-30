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

pub(crate) fn xml_text(xml: &str) -> String {
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut out = String::new();
    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Text(t)) => {
                if let Ok(s) = t.decode()
                    && !s.trim().is_empty()
                {
                    if !out.is_empty() {
                        out.push(' ')
                    }
                    out.push_str(&s);
                }
            }
            Ok(quick_xml::events::Event::GeneralRef(r)) => {
                let resolved = r
                    .resolve_char_ref()
                    .ok()
                    .flatten()
                    .map(String::from)
                    .or_else(|| {
                        r.decode().ok().and_then(|name| {
                            quick_xml::escape::resolve_predefined_entity(&name).map(str::to_string)
                        })
                    });
                if let Some(s) = resolved {
                    out.push_str(&s);
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    out
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
