//! Lectura de PDF con pdf_oxide: caracteres con posición y fuente, imágenes con su
//! rectángulo, esquema, metadatos, etiquetas de página internas y vistas renderizadas.
//! Las coordenadas pasan a puntos con origen en la esquina superior izquierda de la
//! página.
use crate::worker::Rejected;
use anyhow::{Context, Result};
use pdf_oxide::{
    Destination, OutlineItem, PdfDocument,
    content::{Operator, TextElement, parse_content_stream, parse_content_stream_images_only},
    extractors::images::{ColorSpace, PdfFilter},
    layout::TextChar,
    object::Object,
    rendering::{RenderOptions, render_page},
};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

/// Resuelve una URI sin protocolo solo si el propio PDF contiene un destino
/// HTTP(S) inequívoco para el mismo dominio y ruta. No consulta la red.
pub(crate) fn uri_aliases(doc: &PdfDocument, pages: usize) -> HashMap<String, String> {
    aliases_for_uris(
        (0..pages)
            .flat_map(|p| doc.get_annotations(p).unwrap_or_default())
            .filter_map(|a| match a.action {
                Some(pdf_oxide::LinkAction::Uri(uri)) => Some(uri),
                _ => None,
            }),
    )
}

fn aliases_for_uris(uris: impl IntoIterator<Item = String>) -> HashMap<String, String> {
    let mut explicit: HashMap<String, HashSet<String>> = HashMap::new();
    let mut bare = HashSet::new();
    for uri in uris {
        let Some(k) = uri_key(&uri) else {
            continue;
        };
        if crate::bundle::has_scheme(&uri) {
            let target = if k.ends_with('/') && !k[..k.len() - 1].contains('/') {
                uri.trim_end_matches('/').to_string()
            } else {
                uri.clone()
            };
            explicit.entry(k).or_default().insert(target);
        } else {
            bare.insert(uri);
        }
    }
    bare.into_iter()
        .filter_map(|uri| {
            let targets = explicit.get(&uri_key(&uri)?)?;
            (targets.len() == 1).then(|| (uri, targets.iter().next().unwrap().clone()))
        })
        .collect()
}

fn uri_key(uri: &str) -> Option<String> {
    let uri = uri.trim();
    let rest = if let Some((scheme, rest)) = uri.split_once("://") {
        if !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https") {
            return None;
        }
        rest
    } else {
        uri
    };
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = host.to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    if !host.contains('.')
        || !host
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'-'))
    {
        return None;
    }
    Some(format!("{host}/{}", path))
}

/// Rectángulo en puntos, con origen arriba a la izquierda.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Rect {
    pub(crate) x0: f32,
    pub(crate) top: f32,
    pub(crate) x1: f32,
    pub(crate) bottom: f32,
}

impl Rect {
    pub(crate) fn width(&self) -> f32 {
        self.x1 - self.x0
    }
    pub(crate) fn height(&self) -> f32 {
        self.bottom - self.top
    }
    pub(crate) fn area(&self) -> f32 {
        self.width().max(0.0) * self.height().max(0.0)
    }
    pub(crate) fn intersect(&self, o: &Rect) -> Rect {
        Rect {
            x0: self.x0.max(o.x0),
            top: self.top.max(o.top),
            x1: self.x1.min(o.x1),
            bottom: self.bottom.min(o.bottom),
        }
    }
    pub(crate) fn overlaps(&self, o: &Rect) -> bool {
        let i = self.intersect(o);
        i.width() > 0.0 && i.height() > 0.0
    }
}

/// Carácter dibujado en la página.
#[derive(Clone, Debug)]
pub(crate) struct Glyph {
    pub(crate) c: char,
    pub(crate) x0: f32,
    pub(crate) x1: f32,
    /// Línea base, desde el borde superior.
    pub(crate) base: f32,
    pub(crate) size: f32,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    pub(crate) mono: bool,
    /// Girado respecto de la página (texto vertical o inclinado).
    pub(crate) rotated: bool,
    /// Fragmento de texto de pdf_oxide al que pertenece; un fragmento nunca cruza
    /// líneas.
    pub(crate) span: usize,
    /// Destino de la anotación de enlace que lo cubre (plan 3, I5): una URL, una página
    /// del documento (`PAGE_LINK` y su número) o `UNRESOLVED_DEST`.
    pub(crate) link: Option<String>,
}

/// Prefijo del destino de un enlace a una página del propio documento.
pub(crate) const PAGE_LINK: &str = "\u{5}";

fn uri_target(uri: &str) -> Option<String> {
    let uri = uri.trim();
    (!uri.is_empty()).then(|| {
        if crate::bundle::has_scheme(uri) {
            uri.to_string()
        } else {
            // El destino pertenece al PDF original, no a contenido/ en el ZIP.
            crate::bundle::UNRESOLVED_DEST.to_string()
        }
    })
}

fn resolved(doc: &PdfDocument, object: &Object) -> Option<Object> {
    let mut object = object.clone();
    let mut seen = std::collections::HashSet::new();
    for _ in 0..64 {
        match object {
            Object::Reference(r) if seen.insert(r) => object = doc.load_object(r).ok()?,
            Object::Reference(_) => return None,
            _ => return Some(object),
        }
    }
    None
}

/// Un destino local referencia un objeto Page, no su ordinal. Recorre sus padres
/// sumando las páginas de los hermanos anteriores y comprueba la identidad final.
fn referenced_page(
    doc: &PdfDocument,
    reference: pdf_oxide::object::ObjectRef,
    pages: usize,
) -> Option<usize> {
    let target = doc.load_object(reference).ok()?;
    let mut child = reference;
    let mut object = target.clone();
    let mut offset = 0usize;
    let mut seen = std::collections::HashSet::new();
    for _ in 0..64 {
        if !seen.insert(child) {
            return None;
        }
        let dict = object.as_dict()?;
        let Some(parent) = dict.get("Parent") else {
            return (offset < pages && doc.get_page(offset).ok()? == target).then_some(offset);
        };
        let parent_ref = parent.as_reference()?;
        let parent = doc.load_object(parent_ref).ok()?;
        let kids = resolved(doc, parent.as_dict()?.get("Kids")?)?;
        let mut found = false;
        for sibling in kids.as_array()? {
            if sibling.as_reference() == Some(child) {
                found = true;
                break;
            }
            let sibling = resolved(doc, sibling)?;
            let d = sibling.as_dict()?;
            let count = if d.get("Type").and_then(Object::as_name) == Some("Pages") {
                usize::try_from(d.get("Count")?.as_integer()?).ok()?
            } else {
                1
            };
            offset = offset.checked_add(count)?;
            if offset >= pages {
                return None;
            }
        }
        if !found {
            return None;
        }
        child = parent_ref;
        object = parent;
    }
    None
}

fn raw_destination(doc: &PdfDocument, a: &pdf_oxide::Annotation) -> Option<Object> {
    let dict = a.raw_dict.as_ref()?;
    if let Some(dest) = dict.get("Dest") {
        return resolved(doc, dest);
    }
    let action = resolved(doc, dict.get("A")?)?;
    let action = action.as_dict()?;
    (action.get("S").and_then(Object::as_name) == Some("GoTo"))
        .then(|| resolved(doc, action.get("D")?))
        .flatten()
}

/// Destino de una anotación de enlace: URL, página (desde 1) o referencia sin destino.
/// Los demás (otro archivo, acciones con nombre) no se convierten en enlace.
fn link_target(doc: &PdfDocument, a: &pdf_oxide::Annotation, pages: usize) -> Option<String> {
    use pdf_oxide::{LinkAction, LinkDestination};
    if let Some(Object::Array(dest)) = raw_destination(doc, a) {
        let page = match dest.first() {
            Some(Object::Reference(r)) => referenced_page(doc, *r, pages),
            Some(Object::Integer(n)) => usize::try_from(*n).ok().filter(|&n| n < pages),
            _ => None,
        };
        return Some(page.map_or_else(
            || crate::bundle::UNRESOLVED_DEST.to_string(),
            |p| format!("{PAGE_LINK}{}", p + 1),
        ));
    }
    let page = |d: &LinkDestination| -> String {
        let p = match d {
            LinkDestination::Explicit { page, .. } => Some(*page as usize),
            LinkDestination::Named(n) => doc.resolve_named_destination(n).ok().flatten(),
        };
        match p.filter(|&p| p < pages) {
            Some(p) => format!("{PAGE_LINK}{}", p + 1),
            None => crate::bundle::UNRESOLVED_DEST.to_string(),
        }
    };
    match (&a.action, &a.destination) {
        (Some(LinkAction::Uri(u)), _) => uri_target(u),
        (Some(LinkAction::GoTo(d)), _) | (None, Some(d)) => Some(page(d)),
        _ => None,
    }
}

/// Marca los caracteres que cubre cada anotación de enlace de la página.
fn apply_links(
    doc: &PdfDocument,
    index: usize,
    (mx0, my1): (f32, f32),
    glyphs: &mut [Glyph],
    aliases: &HashMap<String, String>,
) {
    let Ok(annotations) = doc.get_annotations(index) else {
        return;
    };
    let pages = doc.page_count().unwrap_or(0);
    for a in annotations {
        if a.subtype.as_deref() != Some("Link") {
            continue;
        }
        let target = match &a.action {
            Some(pdf_oxide::LinkAction::Uri(uri)) if aliases.contains_key(uri) => {
                aliases.get(uri).cloned()
            }
            _ => link_target(doc, &a, pages),
        };
        let (Some([ax0, ay0, ax1, ay1]), Some(target)) = (a.rect, target) else {
            continue;
        };
        let (left, right) = (ax0.min(ax1) as f32 - mx0, ax0.max(ax1) as f32 - mx0);
        let (top, bottom) = (my1 - ay0.max(ay1) as f32, my1 - ay0.min(ay1) as f32);
        // Centro del carácter, algo por encima de la línea base, con un punto de margen.
        let inside = |g: &Glyph| {
            let (cx, cy) = ((g.x0 + g.x1) / 2.0, g.base - 0.3 * g.size);
            (left - 1.0..=right + 1.0).contains(&cx) && (top - 1.0..=bottom + 1.0).contains(&cy)
        };
        let covered: Vec<bool> = glyphs.iter().map(inside).collect();
        let mut linked = covered.clone();
        // Algunos productores alargan el rectángulo sobre el comienzo de la palabra que
        // sigue al enlace: si solo cubre el principio de una palabra y también la palabra
        // anterior, ese principio no forma parte del enlace.
        let ranges = words(glyphs);
        for (w, word) in ranges.iter().enumerate() {
            let inside_part = word.clone().take_while(|&k| covered[k]).count();
            let partial = inside_part > 0 && inside_part < word.len();
            let prefix_only = word.clone().skip(inside_part).all(|k| !covered[k]);
            let previous_covered = w > 0 && covered[ranges[w - 1].end - 1];
            if partial && prefix_only && previous_covered {
                for k in word.clone() {
                    linked[k] = false;
                }
            }
        }
        for (g, on) in glyphs.iter_mut().zip(linked) {
            if on {
                g.link = Some(target.clone());
            }
        }
    }
}

/// Palabras de una página: tramos de caracteres seguidos de la misma línea, sin espacios
/// ni huecos mayores que un tercio del tamaño de letra.
fn words(glyphs: &[Glyph]) -> Vec<std::ops::Range<usize>> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (k, g) in glyphs.iter().enumerate() {
        let joined = k > 0 && {
            let p = &glyphs[k - 1];
            !p.c.is_whitespace()
                && (p.base - g.base).abs() < 0.5
                && g.x0 - p.x1 < 0.33 * p.size.min(g.size)
        };
        if g.c.is_whitespace() {
            if let Some(s) = start.take() {
                out.push(s..k);
            }
            continue;
        }
        match start {
            Some(s) if !joined => {
                out.push(s..k);
                start = Some(k);
            }
            None => start = Some(k),
            _ => {}
        }
    }
    if let Some(s) = start {
        out.push(s..glyphs.len());
    }
    out
}

/// Imagen dibujada en la página. Las que no son decorativas llevan la huella de sus
/// datos, que identifica las que se repiten; sus bytes se codifican después, solo si
/// se adjuntan.
#[derive(Clone, Debug)]
pub(crate) struct Picture {
    /// Posición en la lista de imágenes de la página que entrega pdf_oxide.
    pub(crate) index: usize,
    pub(crate) rect: Rect,
    /// Fracción del área de la página que cubre la parte visible de la imagen.
    pub(crate) coverage: f32,
    pub(crate) hash: Option<String>,
}

pub(crate) struct Encoded {
    pub(crate) bytes: Vec<u8>,
    pub(crate) ext: &'static str,
}

#[derive(Clone, Debug)]
pub(crate) struct LateralLink {
    pub(crate) rect: Rect,
    pub(crate) target: String,
}

fn annotation_rect(rect: [f64; 4], origin: (f32, f32)) -> Rect {
    let [x0, y0, x1, y1] = rect;
    Rect {
        x0: x0.min(x1) as f32 - origin.0,
        x1: x0.max(x1) as f32 - origin.0,
        top: origin.1 - y0.max(y1) as f32,
        bottom: origin.1 - y0.min(y1) as f32,
    }
}

pub(crate) fn lateral_links(doc: &PdfDocument, index: usize) -> Vec<LateralLink> {
    let (x0, y0, x1, y1) = media_box(doc, index);
    let (w, h) = (x1 - x0, y1 - y0);
    doc.get_annotations(index)
        .unwrap_or_default()
        .iter()
        .filter_map(|a| {
            if a.subtype.as_deref() != Some("Link") {
                return None;
            }
            let rect = annotation_rect(a.rect?, (x0, y1));
            ((rect.x0 >= 0.88 * w || rect.x1 <= 0.12 * w)
                && rect.width() <= 0.15 * w
                && rect.height() >= 0.04 * h)
                .then(|| {
                    Some(LateralLink {
                        rect,
                        target: link_target(doc, a, doc.page_count().ok()?)?,
                    })
                })
                .flatten()
        })
        .collect()
}

/// Recupera las líneas giradas de una pestaña en el orden de escritura: columna
/// desde la izquierda y, dentro de ella, avance desde abajo hacia arriba. Descarta
/// las copias diminutas de accesibilidad cuando también hay texto visible.
pub(crate) fn sidebar_text(doc: &PdfDocument, index: usize, rect: Rect) -> String {
    let (mx0, _, _, my1) = media_box(doc, index);
    let mut spans: Vec<_> = doc
        .extract_spans(index)
        .unwrap_or_default()
        .into_iter()
        .filter(|s| {
            let (x, y) = (s.bbox.x - mx0, my1 - s.bbox.y);
            x >= rect.x0 - 1.
                && x <= rect.x1 + 1.
                && y >= rect.top - 2.
                && y <= rect.bottom + 2.
                && !s.text.trim().is_empty()
        })
        .collect();
    let size = spans.iter().map(|s| s.font_size).fold(0., f32::max);
    spans.retain(|s| s.font_size >= 0.6 * size);
    spans.sort_by(|a, b| {
        a.bbox
            .x
            .total_cmp(&b.bbox.x)
            .then(a.bbox.y.total_cmp(&b.bbox.y))
    });
    spans
        .iter()
        .map(|s| s.text.trim())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Reglas rectas pintadas, en coordenadas físicas; los rectángulos grandes y las
/// curvas de diagramas no se interpretan como separadores de tabla.
pub(crate) fn rulings(doc: &PdfDocument, index: usize) -> Vec<Rect> {
    let (x0, _, _, y1) = media_box(doc, index);
    doc.extract_paths(index)
        .unwrap_or_default()
        .into_iter()
        .filter(|p| {
            p.bbox.width >= 40.
                && p.bbox.height <= 1.5
                && p.stroke_color.is_some()
                && p.operations.iter().all(|op| {
                    matches!(
                        op,
                        pdf_oxide::elements::PathOperation::MoveTo(..)
                            | pdf_oxide::elements::PathOperation::LineTo(..)
                    )
                })
        })
        .map(|p| Rect {
            x0: p.bbox.x - x0,
            x1: p.bbox.x + p.bbox.width - x0,
            top: y1 - p.bbox.y - p.bbox.height,
            bottom: y1 - p.bbox.y,
        })
        .collect()
}

pub(crate) fn small_controls(doc: &PdfDocument, index: usize) -> Vec<Rect> {
    let (x0, y0, x1, y1) = media_box(doc, index);
    doc.get_annotations(index)
        .unwrap_or_default()
        .iter()
        .filter_map(|a| {
            if a.subtype.as_deref() != Some("Widget") {
                return None;
            }
            let rect = annotation_rect(a.rect?, (x0, y1));
            (rect.width() > 1.
                && rect.width() < 0.15 * (x1 - x0)
                && rect.height() > 1.
                && rect.height() < 0.06 * (y1 - y0))
                .then_some(rect)
        })
        .collect()
}

pub(crate) struct Page {
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) glyphs: Vec<Glyph>,
    pub(crate) pictures: Vec<Picture>,
    /// Fuente sin glifos o texto invisible sobre la imagen de un escaneo.
    pub(crate) ocr_layer: bool,
}

/// Fracción mínima del área de la página para que una imagen no sea decorativa.
pub(crate) const DECORATIVE_COVERAGE: f32 = 0.01;

/// Abre el PDF; un archivo cifrado que no se abre con la contraseña vacía se rechaza.
pub(crate) fn open(bytes: &[u8]) -> Result<PdfDocument> {
    let doc = PdfDocument::from_bytes(bytes.to_vec())
        .map_err(|e| Rejected(format!("el PDF no se puede leer: {e}")))?;
    if doc.is_encrypted() && !doc.is_authenticated() && !doc.authenticate(b"").unwrap_or(false) {
        return Err(Rejected("el PDF está protegido con contraseña".into()).into());
    }
    Ok(doc)
}

/// Restricciones de un PDF cifrado que se abrió con la contraseña vacía: lo que sus
/// permisos (`/P`) no autorizan.
pub(crate) fn restrictions(doc: &PdfDocument) -> Vec<String> {
    if !doc.is_encrypted() {
        return Vec::new();
    }
    let Some(p) = doc.permissions() else {
        return vec!["PDF cifrado con permisos ilegibles".into()];
    };
    [
        (p.copy, "copiar texto y gráficos"),
        (p.accessibility, "extraer el texto para accesibilidad"),
        (p.print_low_res, "imprimir"),
        (p.modify, "modificar"),
    ]
    .iter()
    .filter(|(allowed, _)| !allowed)
    .map(|(_, what)| format!("el PDF no permite {what}"))
    .collect()
}

pub(crate) fn page_count(doc: &PdfDocument) -> Result<usize> {
    doc.page_count()
        .map_err(|e| Rejected(format!("el PDF no tiene un árbol de páginas legible: {e}")).into())
}

/// Solo texto invisible en el flujo de página: no confunde una cadena que contiene
/// «3 Tr» con un operador, ni olvida que q/Q restaura el modo de dibujo del texto.
fn invisible_text(data: &[u8]) -> bool {
    let Some(data) = content_without_comments(data) else {
        return false;
    };
    let Ok(ops) = parse_content_stream(&data) else {
        return false;
    };
    let mut mode = 0;
    let mut stack = Vec::new();
    let mut found = false;
    for op in ops {
        let has_text = match &op {
            Operator::Tj { text }
            | Operator::Quote { text }
            | Operator::DoubleQuote { text, .. } => !text.is_empty(),
            Operator::TJ { array } => array
                .iter()
                .any(|e| matches!(e, TextElement::String(s) if !s.is_empty())),
            _ => false,
        };
        if has_text {
            if mode != 3 {
                return false;
            }
            found = true;
        }
        match op {
            Operator::Tr { render } => mode = render,
            Operator::SaveState => stack.push(mode),
            Operator::RestoreState => match stack.pop() {
                Some(saved) => mode = saved,
                None => return false,
            },
            _ => {}
        }
    }
    found
}

/// El parser de operadores puede interpretar palabras dentro de comentarios.
/// El parser de objetos sí respeta cadenas, escapes, arrays y comentarios internos.
fn content_without_comments(mut data: &[u8]) -> Option<Vec<u8>> {
    let mut clean = Vec::with_capacity(data.len());
    while !data.is_empty() {
        data = data.trim_ascii_start();
        if data.first() == Some(&0) {
            data = &data[1..];
            continue;
        }
        if data.first() == Some(&b'%') {
            data = &data[data
                .iter()
                .position(|b| matches!(b, b'\r' | b'\n'))
                .unwrap_or(data.len())..];
            continue;
        }
        if data.is_empty() {
            break;
        }
        let len = if let Ok((rest, _)) = pdf_oxide::parser::parse_object(data) {
            data.len() - rest.len()
        } else {
            let len = data
                .iter()
                .position(|b| b.is_ascii_whitespace() || b"\0()<>[]{}/%".contains(b))
                .unwrap_or(data.len());
            // Las imágenes inline contienen bytes arbitrarios; no se infiere su modo.
            if len == 0 || &data[..len] == b"BI" {
                return None;
            }
            len
        };
        clean.extend_from_slice(&data[..len]);
        clean.push(b' ');
        data = &data[len..];
    }
    Some(clean)
}

fn invisible_ocr_layer(doc: &PdfDocument, index: usize) -> bool {
    let Ok(data) = doc.get_page_content_data(index) else {
        return false;
    };
    if !invisible_text(&data) {
        return false;
    }
    let Some(resources) = doc
        .get_page_resources(index)
        .ok()
        .and_then(|r| resolved(doc, &r))
    else {
        return false;
    };
    if resources.as_dict().is_none_or(|r| r.is_empty()) {
        return false;
    }
    forms_without_text(doc, &resources, 0, &mut 64)
}

/// Un Form puede envolver las imágenes de un escaneo. Solo se admite si sus
/// recursos anidados se comprueban también y no contienen operadores de texto.
/// Los límites evitan ciclos y expansiones de recursos en documentos hostiles.
fn forms_without_text(
    doc: &PdfDocument,
    resources: &Object,
    depth: usize,
    budget: &mut usize,
) -> bool {
    if depth >= 8 {
        return false;
    }
    let Some(resources_dict) = resources.as_dict() else {
        return false;
    };
    if let Some(xobjects) = resources_dict.get("XObject") {
        let Some(xobjects) = resolved(doc, xobjects) else {
            return false;
        };
        let Some(objects) = xobjects.as_dict() else {
            return false;
        };
        for object in objects.values() {
            if *budget == 0 {
                return false;
            }
            *budget -= 1;
            let Some(object) = resolved(doc, object) else {
                return false;
            };
            let Some(dict) = object.as_dict() else {
                return false;
            };
            match dict.get("Subtype").and_then(Object::as_name) {
                Some("Image") => {}
                Some("Form") => {
                    let Ok(data) = object.decode_stream_data() else {
                        return false;
                    };
                    let Some(data) = content_without_comments(&data) else {
                        return false;
                    };
                    let Ok(ops) = parse_content_stream(&data) else {
                        return false;
                    };
                    if ops.iter().any(|op| {
                        matches!(
                            op,
                            Operator::Tj { .. }
                                | Operator::TJ { .. }
                                | Operator::Quote { .. }
                                | Operator::DoubleQuote { .. }
                        )
                    }) {
                        return false;
                    }
                    let child = match dict.get("Resources") {
                        Some(r) => match resolved(doc, r) {
                            Some(r) => r,
                            None => return false,
                        },
                        None => resources.clone(),
                    };
                    if !forms_without_text(doc, &child, depth + 1, budget) {
                        return false;
                    }
                }
                _ => return false,
            }
        }
    }
    true
}

/// Caja de la página: origen y tamaño en el espacio de usuario.
pub(crate) fn media_box(doc: &PdfDocument, index: usize) -> (f32, f32, f32, f32) {
    match doc.get_page_media_box(index) {
        Ok((x0, y0, x1, y1)) if x1 > x0 && y1 > y0 => (x0, y0, x1, y1),
        _ => (0.0, 0.0, 612.0, 792.0),
    }
}

/// Caracteres e imágenes de una página.
#[cfg(test)]
pub(crate) fn page(doc: &PdfDocument, index: usize) -> Result<Page> {
    page_with_aliases(doc, index, &HashMap::new())
}

pub(crate) fn page_with_aliases(
    doc: &PdfDocument,
    index: usize,
    aliases: &HashMap<String, String>,
) -> Result<Page> {
    let (mx0, my0, mx1, my1) = media_box(doc, index);
    let (width, height) = (mx1 - mx0, my1 - my0);
    // Los fragmentos de texto conservan la decodificación correcta de cada carácter y
    // su posición medida; `extract_chars` desplaza los caracteres que siguen a uno no
    // ASCII mal decodificado. Pero pdf_oxide a veces arma mal un fragmento (números de
    // cita coloreados como enlace que salta de un extremo a otro de la línea): en ese
    // tramo de línea se usan los caracteres de `extract_chars`.
    let spans = doc
        .extract_spans(index)
        .with_context(|| format!("no se pudo leer el texto de la página {}", index + 1))?;
    let mut chars: Vec<(TextChar, usize)> = spans
        .iter()
        .enumerate()
        .flat_map(|(k, s)| s.to_chars().into_iter().map(move |c| (c, k)))
        .collect();
    // pdf_oxide alinea con la línea base de un glifo grande (comillas o capitular) los
    // fragmentos menores que lo rodean: la primera línea de una cita con comillas de
    // 30 pt quedaba casi en la línea de la segunda, se fundían y sus letras se
    // intercalaban. `extract_chars` conserva la posición real de esos caracteres.
    let mut direct: Option<Vec<TextChar>> = None;
    let snapped: Vec<usize> = (0..spans.len())
        .filter(|&k| {
            let s = &spans[k];
            spans.iter().any(|big| {
                big.font_size >= 1.5 * s.font_size
                    && (big.bbox.y - s.bbox.y).abs() < 0.05
                    && (big.bbox.x - s.bbox.x).abs() < 3.0 * big.font_size
            })
        })
        .collect();
    if !snapped.is_empty() {
        let real = direct.get_or_insert_with(|| doc.extract_chars(index).unwrap_or_default());
        for &k in &snapped {
            let size = spans[k].font_size;
            let deltas: Vec<f32> = chars
                .iter()
                .filter(|(c, o)| *o == k && !c.char.is_whitespace())
                .filter_map(|(c, _)| {
                    real.iter()
                        .find(|r| r.char == c.char && (r.bbox.x - c.bbox.x).abs() < 0.3 * size)
                        .map(|r| r.origin_y - c.origin_y)
                })
                .collect();
            let Some(&delta) = deltas.first() else {
                continue;
            };
            let agree = deltas.iter().filter(|d| (*d - delta).abs() < 0.5).count();
            if delta.abs() > 0.25 * size && agree * 2 > deltas.len() {
                for (c, _) in chars.iter_mut().filter(|(_, o)| *o == k) {
                    c.origin_y += delta;
                    c.bbox.y += delta;
                }
            }
        }
    }
    let suspect: Vec<usize> = (0..spans.len())
        .filter(|&k| {
            let own: Vec<TextChar> = chars
                .iter()
                .filter(|(_, s)| *s == k)
                .map(|(c, _)| c.clone())
                .collect();
            malformed(&spans[k], &own, width)
        })
        .collect();
    if !suspect.is_empty() {
        let direct = direct.unwrap_or_else(|| doc.extract_chars(index).unwrap_or_default());
        for &k in &suspect {
            let s = &spans[k];
            let same_line = |y: f32| (y - s.bbox.y).abs() <= 0.25 * s.font_size;
            // Tramo de la línea entre el fragmento vecino de otra columna a cada lado.
            let mut row: Vec<(f32, f32)> = spans
                .iter()
                .enumerate()
                .filter(|(j, o)| same_line(o.bbox.y) && !suspect.contains(j))
                .map(|(_, o)| (o.bbox.x, o.bbox.x + o.bbox.width))
                .collect();
            row.sort_by(|a, b| a.0.total_cmp(&b.0));
            // Extensión real del fragmento: la de sus caracteres, no la de su rectángulo.
            let own = chars.iter().filter(|(_, o)| *o == k).map(|(c, _)| c);
            let (mut a, mut b) = own.fold((f32::MAX, f32::MIN), |(a, b), c| {
                (a.min(c.bbox.x), b.max(c.bbox.x + c.bbox.width.max(0.0)))
            });
            if a > b {
                continue;
            }
            for &(x0, x1) in &row {
                if x0 <= b + 0.3 * s.font_size && x1 >= a - 0.3 * s.font_size {
                    a = a.min(x0);
                    b = b.max(x1);
                }
            }
            let left = row
                .iter()
                .filter(|r| r.1 < a)
                .map(|r| r.1)
                .fold(f32::MIN, f32::max);
            let right = row
                .iter()
                .filter(|r| r.0 > b)
                .map(|r| r.0)
                .fold(f32::MAX, f32::min);
            let inside =
                |c: &TextChar| same_line(c.origin_y) && c.bbox.x > left && c.bbox.x < right;
            // `extract_chars` desplaza el texto que sigue a un carácter no ASCII mal
            // decodificado: si el tramo tiene uno, se conserva el fragmento.
            let replacement: Vec<TextChar> = direct.iter().filter(|c| inside(c)).cloned().collect();
            let garbled = replacement
                .iter()
                .enumerate()
                .any(|(i, c)| c.char == '\u{FFFD}' || utf8_in_winansi(&replacement[i..]).is_some());
            if replacement.is_empty() || garbled {
                // Sin un reemplazo limpio, el fragmento conserva su texto y sus
                // caracteres se colocan seguidos con sus anchos.
                let mut x = a;
                let mut n = 0;
                for (c, o) in chars.iter_mut() {
                    if *o == k {
                        let zero_width = matches!(
                            c.char,
                            '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FEFF}'
                        );
                        let w = if zero_width {
                            0.0
                        } else {
                            s.char_widths
                                .get(n)
                                .copied()
                                .filter(|w| *w > 0.0 && *w < 2.0 * s.font_size)
                                .unwrap_or(0.5 * s.font_size)
                        };
                        c.bbox.x = x;
                        c.bbox.width = w;
                        x += w;
                        n += 1;
                    }
                }
                continue;
            }
            chars.retain(|(c, _)| !inside(c));
            chars.extend(replacement.into_iter().map(|c| (c, k)));
        }
    }
    let (chars, span_of): (Vec<TextChar>, Vec<usize>) = chars.into_iter().unzip();
    let mut ocr_layer = invisible_ocr_layer(doc, index);
    let mut glyphs: Vec<Glyph> = Vec::with_capacity(chars.len());
    let mut k = 0;
    while k < chars.len() {
        let ch = &chars[k];
        k += 1;
        // Carácter no ASCII que pdf_oxide entrega como los bytes de su UTF-8 leídos en
        // WinAnsi («â€‘» por «‑»): se reúne en un solo carácter.
        if let Some((c, len)) = utf8_in_winansi(&chars[k - 1..]) {
            let last = &chars[k - 2 + len];
            let next_x0 = chars
                .get(k - 1 + len)
                .filter(|n| (n.origin_y - ch.origin_y).abs() < 0.5 && !n.char.is_whitespace())
                .map(|n| n.bbox.x - mx0);
            let x0 = ch.bbox.x - mx0;
            let x1 = last.bbox.x - mx0 + last.bbox.width.max(0.0);
            glyphs.push(Glyph {
                c,
                x0,
                x1: next_x0
                    .filter(|&n| n > x1 && n - x1 < ch.font_size)
                    .unwrap_or(x1),
                base: my1 - ch.origin_y,
                size: ch.font_size,
                bold: ch.font_weight as u16 >= 600,
                italic: ch.is_italic || italic_font(&ch.font_name),
                mono: ch.is_monospace,
                rotated: false,
                span: span_of[k - 1],
                link: None,
            });
            k += len - 1;
            continue;
        }
        let c = match ch.char {
            '\t' => ' ',
            // Caracteres de ancho cero que algunos editores intercalan en las URL.
            '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FEFF}' => continue,
            c => c,
        };
        if c.is_control() || ch.font_size <= 0.0 {
            continue;
        }
        if ch.font_name.to_ascii_uppercase().contains("GLYPHLESS") {
            ocr_layer = true;
        }
        let x0 = ch.bbox.x - mx0;
        glyphs.push(Glyph {
            c,
            x0,
            x1: x0 + ch.bbox.width.max(0.0),
            base: my1 - ch.origin_y,
            size: ch.font_size,
            bold: ch.font_weight as u16 >= 600,
            italic: ch.is_italic || italic_font(&ch.font_name),
            mono: ch.is_monospace,
            rotated: ch.rotation_degrees.abs() > 5.0
                && (ch.rotation_degrees.abs() - 360.0).abs() > 5.0,
            span: span_of[k - 1],
            link: None,
        });
    }
    compose_accents(&mut glyphs);
    apply_links(doc, index, (mx0, my1), &mut glyphs, aliases);
    let page_rect = Rect {
        x0: 0.0,
        top: 0.0,
        x1: width,
        bottom: height,
    };
    // Las imágenes se enumeran sin descomprimirlas; la huella sale de sus bytes
    // comprimidos, de modo que una imagen repetida en el PDF se reconoce sin decodificarla.
    let mut pictures = Vec::new();
    for (k, h) in doc
        .page_image_handles(index)
        .unwrap_or_default()
        .iter()
        .enumerate()
    {
        let b = h.bbox;
        let rect = Rect {
            x0: b.x - mx0,
            top: my1 - (b.y + b.height),
            x1: b.x - mx0 + b.width,
            bottom: my1 - b.y,
        };
        let coverage = rect.intersect(&page_rect).area() / page_rect.area().max(1.0);
        if coverage <= 0.0 {
            continue;
        }
        let hash = (coverage >= DECORATIVE_COVERAGE).then(|| {
            let mut sha = Sha256::new();
            sha.update(h.width.to_le_bytes());
            sha.update(h.height.to_le_bytes());
            sha.update(h.raw_compressed_bytes().unwrap_or_default());
            crate::bundle::hex(&sha.finalize())
        });
        pictures.push(Picture {
            index: k,
            rect,
            coverage,
            hash,
        });
    }
    Ok(Page {
        width,
        height,
        glyphs,
        pictures,
        ocr_layer,
    })
}

/// ¿Nombra la fuente una variante cursiva? pdf_oxide no siempre lo deduce de sus
/// descriptores (`NimbusRomNo9L-ReguItal`, la Times de los artículos en LaTeX).
fn italic_font(name: &str) -> bool {
    let name = name.rsplit_once('+').map_or(name, |(_, n)| n);
    let style = name.rsplit_once(['-', ',']).map_or("", |(_, s)| s);
    let lower = style.to_ascii_lowercase();
    lower.contains("ital")
        || lower.contains("oblique")
        || lower == "it"
        || lower.ends_with("it") && lower.len() <= 6 && !lower.ends_with("bit")
}

/// Marca combinante de un diacrítico que el PDF dibuja como carácter propio.
fn combining(c: char) -> Option<char> {
    Some(match c {
        '\u{00A8}' => '\u{0308}', // diéresis
        '\u{00B4}' => '\u{0301}', // agudo
        '`' => '\u{0300}',        // grave
        '\u{02C6}' => '\u{0302}', // circunflejo
        '\u{02DC}' => '\u{0303}', // tilde
        '\u{00AF}' => '\u{0304}', // macrón
        '\u{02D8}' => '\u{0306}', // breve
        '\u{02D9}' => '\u{0307}', // punto
        '\u{02DA}' => '\u{030A}', // anillo
        '\u{02DD}' => '\u{030B}', // doble agudo
        '\u{02C7}' => '\u{030C}', // carón
        '\u{00B8}' => '\u{0327}', // cedilla
        '\u{02DB}' => '\u{0328}', // ogonek
        _ => return None,
    })
}

/// Un diacrítico dibujado aparte (como hace TeX con `\"u` sin una fuente con la letra
/// acentuada) se compone con la letra que tiene debajo: el centro del diacrítico cae
/// dentro de la letra, con su línea base a menos de un tamaño de letra por encima o un
/// cuarto por debajo. Si la letra con el diacrítico no existe como carácter único, los
/// dos se conservan como están.
fn compose_accents(glyphs: &mut Vec<Glyph>) {
    use unicode_normalization::UnicodeNormalization;
    let mut drop = vec![false; glyphs.len()];
    for a in 0..glyphs.len() {
        let accent = &glyphs[a];
        let Some(mark) = combining(accent.c) else {
            continue;
        };
        if accent.rotated || accent.mono {
            continue;
        }
        let center = (accent.x0 + accent.x1) / 2.0;
        let size = accent.size;
        let target = (0..glyphs.len())
            .filter(|&i| i != a && !drop[i])
            .filter(|&i| {
                let g = &glyphs[i];
                g.c.is_alphabetic()
                    && !g.rotated
                    && g.x0 <= center
                    && center <= g.x1
                    && (-size..=0.25 * size).contains(&(accent.base - g.base))
            })
            .min_by(|&i, &j| {
                let d = |k: usize| (glyphs[k].base - accent.base).abs();
                d(i).total_cmp(&d(j))
            });
        let Some(t) = target else { continue };
        let base = match glyphs[t].c {
            'ı' => 'i',
            'ȷ' => 'j',
            c => c,
        };
        let mut composed = [base, mark].into_iter().nfc();
        if let (Some(c), None) = (composed.next(), composed.next()) {
            glyphs[t].c = c;
            drop[a] = true;
        }
    }
    let mut k = 0;
    glyphs.retain(|_| {
        k += 1;
        !drop[k - 1]
    });
}

/// Resultado de preparar una imagen para adjuntarla.
pub(crate) enum Encoding {
    Ready(Encoded),
    /// Habría que decodificarla y supera `OKF_MAX_IMAGE_PIXELS`.
    TooLarge,
    /// No se pudo leer ni codificar.
    Failed,
}

/// Bytes listos para adjuntar de las imágenes `wanted` de una página (por su posición
/// en la enumeración de pdf_oxide): un JPEG que no esté en CMYK se copia tal cual y lo
/// demás se decodifica a PNG, salvo que tenga más de `max_pixels` píxeles.
pub(crate) fn encoded(
    doc: &PdfDocument,
    index: usize,
    wanted: &[usize],
    max_pixels: u64,
) -> Vec<Encoding> {
    let handles = doc.page_image_handles(index).unwrap_or_default();
    let turned = orientations(doc, index);
    wanted
        .iter()
        .map(|&k| {
            let Some(h) = handles.get(k) else {
                return Encoding::Failed;
            };
            let orientation = turned.get(k).copied().unwrap_or(Orientation::Upright);
            let dct = matches!(h.filter_chain.as_slice(), [PdfFilter::DCTDecode]);
            let cmyk = matches!(h.color_space, ColorSpace::DeviceCMYK);
            // Un JPEG se copia tal cual si la página no lo voltea ni lo gira; si lo hace,
            // se adjunta como lo ve el lector, con los píxeles reorientados.
            let upright = orientation == Orientation::Upright;
            let jpeg = dct && !cmyk && upright;
            if !jpeg && u64::from(h.width).saturating_mul(u64::from(h.height)) > max_pixels {
                return Encoding::TooLarge;
            }
            let pixels = || {
                h.decode()
                    .ok()?
                    .to_dynamic_image()
                    .ok()
                    .map(|i| orientation.apply(i))
            };
            let encoded = if jpeg {
                h.raw_compressed_bytes().ok().map(|b| (b, "jpg"))
            } else if dct {
                // Una foto en CMYK se pasa a RGB y vuelve a JPEG: como PNG ocupaba
                // varias veces más y su codificación dominaba la de las imágenes.
                pixels()
                    .and_then(|i| rgb_jpeg(&i.into_rgb8()))
                    .map(|b| (b, "jpg"))
            } else if upright {
                h.decode()
                    .ok()
                    .and_then(|d| d.to_png_bytes().ok())
                    .map(|b| (b, "png"))
            } else {
                pixels()
                    .and_then(|i| {
                        let mut out = std::io::Cursor::new(Vec::new());
                        i.write_to(&mut out, image::ImageFormat::Png).ok()?;
                        Some(out.into_inner())
                    })
                    .map(|b| (b, "png"))
            };
            match encoded {
                Some((bytes, ext)) if !bytes.is_empty() => Encoding::Ready(Encoded { bytes, ext }),
                _ => Encoding::Failed,
            }
        })
        .collect()
}

/// Calidad JPEG de las imágenes CMYK convertidas a RGB.
const CMYK_JPEG_QUALITY: u8 = 90;

fn rgb_jpeg(image: &image::RgbImage) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, CMYK_JPEG_QUALITY)
        .encode_image(image)
        .ok()?;
    Some(out)
}

/// Cómo dibuja la página una imagen respecto al orden de sus píxeles: la fila 0 es la de
/// arriba cuando la matriz de la imagen no la voltea ni la gira. Algunos productores
/// guardan los píxeles invertidos y los enderezan con la matriz.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Orientation {
    Upright,
    FlipVertical,
    FlipHorizontal,
    Rotate180,
    /// Giro de 90° en el sentido de las agujas del reloj.
    Rotate90,
    /// Giro de 90° en el sentido contrario.
    Rotate270,
    /// Giro de 90° en sentido contrario y espejo horizontal.
    Transpose,
    /// Giro de 90° en el sentido del reloj y espejo horizontal.
    Transverse,
}

impl Orientation {
    /// Orientación de la matriz `[a b c d]` (convención de PDF: `x' = a·x + c·y + e`,
    /// `y' = b·x + d·y + f`, con la fila 0 de la imagen en `y = 1`).
    fn of(m: [f32; 6]) -> Self {
        let [a, b, c, d, _, _] = m;
        if a.abs() + d.abs() >= b.abs() + c.abs() {
            match (a >= 0.0, d >= 0.0) {
                (true, true) => Orientation::Upright,
                (true, false) => Orientation::FlipVertical,
                (false, true) => Orientation::FlipHorizontal,
                (false, false) => Orientation::Rotate180,
            }
        } else {
            match (b >= 0.0, c >= 0.0) {
                (true, false) => Orientation::Rotate270,
                (false, true) => Orientation::Rotate90,
                (true, true) => Orientation::Transpose,
                (false, false) => Orientation::Transverse,
            }
        }
    }
    /// La imagen como la ve el lector de la página.
    pub(crate) fn apply(self, image: image::DynamicImage) -> image::DynamicImage {
        match self {
            Orientation::Upright => image,
            Orientation::FlipVertical => image.flipv(),
            Orientation::FlipHorizontal => image.fliph(),
            Orientation::Rotate180 => image.rotate180(),
            Orientation::Rotate90 => image.rotate90(),
            Orientation::Rotate270 => image.rotate270(),
            Orientation::Transpose => image.rotate270().fliph(),
            Orientation::Transverse => image.rotate90().fliph(),
        }
    }
}

type Placement = ([f32; 6], [f32; 4]);

fn multiply(m: [f32; 6], n: [f32; 6]) -> [f32; 6] {
    [
        m[0] * n[0] + m[1] * n[2],
        m[0] * n[1] + m[1] * n[3],
        m[2] * n[0] + m[3] * n[2],
        m[2] * n[1] + m[3] * n[3],
        m[4] * n[0] + m[5] * n[2] + n[4],
        m[4] * n[1] + m[5] * n[3] + n[5],
    ]
}

/// Caja `[x, y, ancho, alto]` del cuadrado unidad transformado por `m`.
fn unit_box(m: [f32; 6]) -> [f32; 4] {
    let corners = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)]
        .map(|(x, y)| (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]));
    let (x0, x1) = corners
        .iter()
        .fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.0), b.max(p.0)));
    let (y0, y1) = corners
        .iter()
        .fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
    [x0, y0, x1 - x0, y1 - y0]
}

/// Matriz y caja de cada imagen dibujada por `ops` (también dentro de Forms), con el
/// mismo recorrido que la enumeración de pdf_oxide. Los límites evitan ciclos y
/// expansiones en documentos hostiles.
fn placements(
    doc: &PdfDocument,
    ops: Vec<Operator>,
    resources: Option<&Object>,
    ctm: [f32; 6],
    depth: usize,
    out: &mut Vec<Placement>,
) {
    let xobjects = resources
        .and_then(|r| r.as_dict()?.get("XObject").cloned())
        .and_then(|x| resolved(doc, &x));
    let mut stack = vec![ctm];
    for op in ops {
        match op {
            Operator::SaveState => stack.push(*stack.last().unwrap()),
            Operator::RestoreState if stack.len() > 1 => {
                stack.pop();
            }
            Operator::Cm { a, b, c, d, e, f } => {
                let top = stack.last_mut().unwrap();
                *top = multiply([a, b, c, d, e, f], *top);
            }
            Operator::InlineImage { .. } => {
                let m = *stack.last().unwrap();
                out.push((m, unit_box(m)));
            }
            Operator::Do { name } => {
                let m = *stack.last().unwrap();
                let Some(object) = xobjects
                    .as_ref()
                    .and_then(|x| x.as_dict()?.get(&name).cloned())
                    .and_then(|o| resolved(doc, &o))
                else {
                    continue;
                };
                let Some(dict) = object.as_dict() else {
                    continue;
                };
                match dict.get("Subtype").and_then(Object::as_name) {
                    Some("Image") => out.push((m, unit_box(m))),
                    Some("Form") if depth < 8 && out.len() < 4096 => {
                        let form = dict
                            .get("Matrix")
                            .and_then(Object::as_array)
                            .filter(|v| v.len() == 6)
                            .and_then(|v| {
                                let n: Vec<f32> = v
                                    .iter()
                                    .filter_map(|x| {
                                        x.as_real()
                                            .map(|r| r as f32)
                                            .or_else(|| x.as_integer().map(|i| i as f32))
                                    })
                                    .collect();
                                <[f32; 6]>::try_from(n).ok()
                            })
                            .unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
                        let own = dict.get("Resources").and_then(|r| resolved(doc, r));
                        let Ok(data) = object.decode_stream_data() else {
                            continue;
                        };
                        let Ok(inner) = parse_content_stream_images_only(&data) else {
                            continue;
                        };
                        placements(
                            doc,
                            inner,
                            own.as_ref().or(resources),
                            multiply(form, m),
                            depth + 1,
                            out,
                        );
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

/// Orientación de cada imagen de la página, en el orden de `page_image_handles`. Cada
/// imagen se empareja con la colocación de su misma caja; sin una, se supone derecha.
pub(crate) fn orientations(doc: &PdfDocument, index: usize) -> Vec<Orientation> {
    let handles = doc.page_image_handles(index).unwrap_or_default();
    let mut found = Vec::new();
    if let (Ok(data), Ok(resources)) = (
        doc.get_page_content_data(index),
        doc.get_page_resources(index),
    ) && let Ok(ops) = parse_content_stream_images_only(&data)
    {
        let resources = resolved(doc, &resources);
        placements(
            doc,
            ops,
            resources.as_ref(),
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            0,
            &mut found,
        );
    }
    let mut used = vec![false; found.len()];
    handles
        .iter()
        .map(|h| {
            let b = [h.bbox.x, h.bbox.y, h.bbox.width, h.bbox.height];
            let hit = (0..found.len())
                .find(|&i| !used[i] && found[i].1.iter().zip(b).all(|(p, q)| (p - q).abs() < 0.5));
            match hit {
                Some(i) => {
                    used[i] = true;
                    Orientation::of(found[i].0)
                }
                None => Orientation::Upright,
            }
        })
        .collect()
}

/// Píxeles de la imagen `wanted` (en la enumeración de pdf_oxide) de la página, para el
/// OCR, si se pueden decodificar sin superar `max_pixels`.
pub(crate) fn picture_pixels(
    doc: &PdfDocument,
    index: usize,
    wanted: usize,
    max_pixels: u64,
) -> Option<image::DynamicImage> {
    let handles = doc.page_image_handles(index).ok()?;
    let h = handles.get(wanted)?;
    if u64::from(h.width).saturating_mul(u64::from(h.height)) > max_pixels {
        return None;
    }
    let orientation = orientations(doc, index)
        .get(wanted)
        .copied()
        .unwrap_or(Orientation::Upright);
    Some(orientation.apply(h.decode().ok()?.to_dynamic_image().ok()?))
}

/// Fragmento armado de forma inconsistente: su rectángulo es mucho más ancho que la
/// página o sus caracteres visibles retroceden.
fn malformed(s: &pdf_oxide::layout::TextSpan, chars: &[TextChar], page_width: f32) -> bool {
    let xs: Vec<(f32, f32)> = chars
        .iter()
        .filter(|c| !c.char.is_whitespace())
        .map(|c| (c.bbox.x, c.bbox.width))
        .collect();
    // Un carácter que retrocede, o que monta sobre el siguiente más de medio ancho: con
    // una escala horizontal en la matriz de texto (expansión de microtype), pdf_oxide
    // puede colocar la primera letra de un `TJ` después del ajuste que la sigue.
    s.bbox.width > 1.5 * page_width
        || xs.windows(2).any(|w| {
            let ((x0, w0), (x1, w1)) = (w[0], w[1]);
            x1 < x0 - 1.0 || (w0 > 0.0 && w1 > 0.0 && x1 < x0 + 0.5 * w0.min(w1))
        })
}

/// Byte WinAnsi (cp1252) de un carácter, si lo tiene.
fn winansi_byte(c: char) -> Option<u8> {
    const HIGH: [(char, u8); 27] = [
        ('€', 0x80),
        ('‚', 0x82),
        ('ƒ', 0x83),
        ('„', 0x84),
        ('…', 0x85),
        ('†', 0x86),
        ('‡', 0x87),
        ('ˆ', 0x88),
        ('‰', 0x89),
        ('Š', 0x8A),
        ('‹', 0x8B),
        ('Œ', 0x8C),
        ('Ž', 0x8E),
        ('‘', 0x91),
        ('’', 0x92),
        ('“', 0x93),
        ('”', 0x94),
        ('•', 0x95),
        ('–', 0x96),
        ('—', 0x97),
        ('˜', 0x98),
        ('™', 0x99),
        ('š', 0x9A),
        ('›', 0x9B),
        ('œ', 0x9C),
        ('ž', 0x9E),
        ('Ÿ', 0x9F),
    ];
    match c as u32 {
        0x80..=0xFF => Some(c as u32 as u8),
        _ => HIGH.iter().find(|(h, _)| *h == c).map(|(_, b)| *b),
    }
}

/// Carácter y número de caracteres si los primeros forman una secuencia UTF-8 de
/// varios bytes escrita en WinAnsi.
fn utf8_in_winansi(chars: &[TextChar]) -> Option<(char, usize)> {
    let lead = winansi_byte(chars.first()?.char)?;
    let len = match lead {
        0xC2..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,
        _ => return None,
    };
    let bytes: Vec<u8> = chars
        .get(..len)?
        .iter()
        .map(|c| winansi_byte(c.char).filter(|b| (0x80..=0xBF).contains(b)))
        .enumerate()
        .map(|(i, b)| if i == 0 { Some(lead) } else { b })
        .collect::<Option<_>>()?;
    let s = std::str::from_utf8(&bytes).ok()?;
    let mut it = s.chars();
    match (it.next(), it.next()) {
        (Some(c), None) => Some((c, len)),
        _ => None,
    }
}

/// Página renderizada en RGBA premultiplicado, sobre fondo blanco.
pub(crate) struct Raster {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl Raster {
    /// PNG sin pérdida con los canales que la página usa: gris si todos los píxeles lo
    /// son (un escaneo en blanco y negro ocupa un cuarto) y sin alfa si es opaca.
    pub(crate) fn png(&self) -> Result<Vec<u8>> {
        use image::{
            ExtendedColorType, ImageEncoder,
            codecs::png::{CompressionType, FilterType, PngEncoder},
        };
        let pixels = self.rgba.as_chunks::<4>().0;
        let opaque = pixels.iter().all(|p| p[3] == 255);
        let gray = opaque && pixels.iter().all(|p| p[0] == p[1] && p[1] == p[2]);
        let (data, color): (Vec<u8>, _) = if gray {
            (pixels.iter().map(|p| p[0]).collect(), ExtendedColorType::L8)
        } else if opaque {
            (
                pixels.iter().flat_map(|p| [p[0], p[1], p[2]]).collect(),
                ExtendedColorType::Rgb8,
            )
        } else {
            // El PNG espera alfa sin premultiplicar.
            let straight = |c: u8, a: u8| match a {
                0 | 255 => c,
                a => ((u32::from(c) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8,
            };
            (
                pixels
                    .iter()
                    .flat_map(|p| {
                        [
                            straight(p[0], p[3]),
                            straight(p[1], p[3]),
                            straight(p[2], p[3]),
                            p[3],
                        ]
                    })
                    .collect(),
                ExtendedColorType::Rgba8,
            )
        };
        let mut out = Vec::new();
        PngEncoder::new_with_quality(&mut out, CompressionType::Fast, FilterType::Adaptive)
            .write_image(&data, self.width, self.height, color)
            .context("no se pudo codificar la vista en PNG")?;
        Ok(out)
    }

    /// Píxeles RGB, compuestos sobre blanco.
    pub(crate) fn rgb(&self) -> image::RgbImage {
        let data = self
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| {
                let under = 255 - p[3];
                [p[0] + under, p[1] + under, p[2] + under]
            })
            .collect();
        image::RgbImage::from_raw(self.width, self.height, data).expect("tamaño del raster")
    }
}

/// Vista de la página, con la mayor dimensión limitada a `max_px` píxeles.
pub(crate) fn render_view(doc: &PdfDocument, index: usize, max_px: f32) -> Result<Raster> {
    let (x0, y0, x1, y1) = media_box(doc, index);
    let dpi = (max_px * 72.0 / (x1 - x0).max(y1 - y0)).clamp(36.0, 150.0) as u32;
    let img = render_page(doc, index, &RenderOptions::with_dpi(dpi).as_raw())
        .map_err(|e| anyhow::anyhow!("no se pudo renderizar la página {}: {e}", index + 1))?;
    if img.data.len() != img.width as usize * img.height as usize * 4 {
        anyhow::bail!(
            "la página {} se renderizó con un tamaño inesperado",
            index + 1
        )
    }
    Ok(Raster {
        width: img.width,
        height: img.height,
        rgba: img.data,
    })
}

/// ¿La página renderizada es de un solo color fuera de los rectángulos indicados?
/// Sirve para distinguir una página en blanco (o con solo un número de página) de una
/// con dibujos vectoriales sin texto.
pub(crate) fn looks_blank(doc: &PdfDocument, index: usize, ignore: &[Rect]) -> Result<bool> {
    const DPI: u32 = 36;
    let img = render_page(doc, index, &RenderOptions::with_dpi(DPI).as_raw())
        .map_err(|e| anyhow::anyhow!("no se pudo renderizar la página {}: {e}", index + 1))?;
    let scale = DPI as f32 / 72.0;
    let (w, h) = (img.width as usize, img.height as usize);
    if w == 0 || h == 0 || img.data.len() < w * h * 4 {
        return Ok(false);
    }
    let masked = |x: usize, y: usize| {
        let (px, py) = (x as f32 / scale, y as f32 / scale);
        ignore.iter().any(|r| {
            px >= r.x0 - 2.0 && px <= r.x1 + 2.0 && py >= r.top - 2.0 && py <= r.bottom + 2.0
        })
    };
    let mut reference: Option<[u8; 4]> = None;
    let mut different = 0usize;
    for y in 0..h {
        for x in 0..w {
            if masked(x, y) {
                continue;
            }
            let i = (y * w + x) * 4;
            let px = [
                img.data[i],
                img.data[i + 1],
                img.data[i + 2],
                img.data[i + 3],
            ];
            match reference {
                None => reference = Some(px),
                Some(r) => {
                    if r.iter().zip(px).any(|(a, b)| a.abs_diff(b) > 24) {
                        different += 1;
                    }
                }
            }
        }
    }
    // Tolera el antialiasing de un borde o un punto suelto.
    Ok(different <= (w * h) / 500)
}

/// Entrada del esquema (marcadores) con su profundidad y página de destino.
pub(crate) struct Bookmark {
    pub(crate) title: String,
    pub(crate) depth: u8,
    pub(crate) page: Option<usize>,
}

pub(crate) fn bookmarks(doc: &PdfDocument) -> Vec<Bookmark> {
    fn walk(items: &[OutlineItem], depth: u8, out: &mut Vec<Bookmark>) {
        for i in items {
            if out.len() >= 10_000 {
                return;
            }
            out.push(Bookmark {
                title: i.title.trim().to_string(),
                depth,
                page: match &i.dest {
                    Some(Destination::PageIndex(p)) => Some(*p),
                    _ => None,
                },
            });
            walk(&i.children, depth.saturating_add(1), out);
        }
    }
    let mut out = Vec::new();
    if let Ok(Some(items)) = doc.get_outline() {
        walk(&items, 0, &mut out);
    }
    out
}

/// Etiquetas de página internas (`/PageLabels`), como texto por página física.
pub(crate) fn internal_labels(doc: &PdfDocument, pages: usize) -> Vec<String> {
    use pdf_oxide::extractors::page_labels::PageLabelExtractor;
    match PageLabelExtractor::extract(doc) {
        Ok(ranges) if !ranges.is_empty() => (0..pages)
            .map(|p| PageLabelExtractor::get_label(&ranges, p))
            .collect(),
        _ => Vec::new(),
    }
}

/// Metadatos del diccionario `/Info`.
#[derive(Default)]
pub(crate) struct Info {
    pub(crate) title: Option<String>,
    pub(crate) author: Option<String>,
    pub(crate) producer: Option<String>,
    pub(crate) creator: Option<String>,
}

pub(crate) fn info(doc: &PdfDocument) -> Info {
    let resolve = |o: &Object| match o.as_reference() {
        Some(r) => doc.load_object(r).ok(),
        None => Some(o.clone()),
    };
    let dict = doc
        .trailer()
        .as_dict()
        .and_then(|t| t.get("Info").or_else(|| t.get("/Info")))
        .and_then(resolve);
    let Some(dict) = dict.as_ref().and_then(Object::as_dict).cloned() else {
        return Info::default();
    };
    let text = |key: &str| {
        dict.get(key)
            .or_else(|| dict.get(&format!("/{key}")))
            .and_then(resolve)
            .and_then(|o| o.as_string().map(text_string))
            .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|s| !s.is_empty())
    };
    Info {
        title: text("Title"),
        author: text("Author"),
        producer: text("Producer"),
        creator: text("Creator"),
    }
}

/// Cadena de texto PDF: UTF-16BE con marca de orden o PDFDocEncoding (casi Latin-1).
fn text_string(b: &[u8]) -> String {
    if b.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = b[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_be_bytes(*c))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    if b.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(&b[3..]).into_owned();
    }
    b.iter().map(|&c| char::from(c)).collect()
}

#[cfg(test)]
pub(super) fn synthetic_slides() -> Vec<u8> {
    synthetic_slides_with_title("First slide")
}

#[cfg(test)]
pub(super) fn synthetic_slides_with_title(title: &str) -> Vec<u8> {
    let stream = |title: &str| {
        format!(
            "BT /F1 24 Tf 30 260 Td ({title}) Tj ET\nBT /F1 10 Tf 0 1 -1 0 380 80 Tm (Menu) Tj ET\n0 G 30 205 m 330 205 l S 30 160 m 330 160 l S 30 115 m 330 115 l S 30 70 m 330 70 l S\nBT /F1 12 Tf 30 215 Td (Key) Tj 120 0 Td (Description) Tj ET\nBT /F1 12 Tf 30 190 Td (One) Tj 120 0 Td (First row) Tj 0 -14 Td (continued) Tj ET\nBT /F1 12 Tf 30 145 Td (Two) Tj 120 0 Td (Second row) Tj ET\nBT /F1 12 Tf 30 100 Td (Three) Tj 120 0 Td (Third row) Tj ET\n0 0 1 RG 240 35 m 275 50 l 300 35 l S\n"
        )
    };
    let content1 = stream(title);
    let content2 = stream("Second slide");
    let objects=std::collections::BTreeMap::from([
        (1,"<< /Type /Catalog /Pages 2 0 R /Outlines 10 0 R >>".into()),
        (2,"<< /Type /Pages /Kids [3 0 R] /Count 2 >>".into()),
        (3,"<< /Type /Pages /Parent 2 0 R /Kids [40 0 R 70 0 R] /Count 2 >>".into()),
        (4,"<< /Type /Pages /Parent 2 0 R /Kids [70 0 R] /Count 1 >>".into()),
        (5,"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into()),
        (10,"<< /Type /Outlines /First 11 0 R /Last 12 0 R /Count 2 >>".into()),
        (11,"<< /Title (Slide 1: First slide) /Parent 10 0 R /Next 12 0 R /Dest [40 0 R /Fit] >>".into()),
        (12,"<< /Title (Slide 2: Second slide) /Parent 10 0 R /Prev 11 0 R /Dest [70 0 R /Fit] >>".into()),
        (40,"<< /Type /Page /Parent 3 0 R /MediaBox [0 0 400 300] /Resources << /Font << /F1 5 0 R >> >> /Contents 50 0 R /Annots [60 0 R] >>".into()),
        (50,format!("<< /Length {} >>\nstream\n{}endstream",content1.len(),content1)),
        (60,"<< /Type /Annot /Subtype /Link /Rect [360 0 400 300] /Dest [70 0 R /Fit] >>".into()),
        (70,"<< /Type /Page /Parent 3 0 R /MediaBox [0 0 400 300] /Resources << /Font << /F1 5 0 R >> >> /Contents 80 0 R /Annots [60 0 R] >>".into()),
        (80,format!("<< /Length {} >>\nstream\n{}endstream",content2.len(),content2)),
    ]);
    let mut text = String::from("%PDF-1.7\n");
    let mut offsets = std::collections::BTreeMap::new();
    for (id, object) in &objects {
        offsets.insert(*id, text.len());
        text.push_str(&format!("{id} 0 obj\n{object}\nendobj\n"));
    }
    let xref = text.len();
    text.push_str("xref\n0 81\n0000000000 65535 f \n");
    for id in 1..=80 {
        match offsets.get(&id) {
            Some(offset) => text.push_str(&format!("{offset:010} 00000 n \n")),
            None => text.push_str("0000000000 00000 f \n"),
        }
    }
    text.push_str(&format!(
        "trailer\n<< /Size 81 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n"
    ));
    text.into_bytes()
}

#[cfg(test)]
pub(super) fn synthetic_ocr_overlay(mode: u8, form: Option<&str>) -> Vec<u8> {
    // Fuente normal sobre una imagen grande: la señal es Tr, no el nombre de fuente.
    let content = format!(
        "q 300 0 0 300 0 0 cm /Im Do Q\nBT /F1 12 Tf {mode} Tr 30 150 Td (Example OCR text.) Tj ET\n"
    );
    let image_ref = if form.is_some() { 7 } else { 5 };
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 300] /Resources << /Font << /F1 4 0 R >> /XObject << /Im {image_ref} 0 R >> >> /Contents 6 0 R >>"),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        "<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Length 3 >>\nstream\nabc\nendstream".into(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
    ];
    if let Some(form) = form {
        objects.push(format!("<< /Type /XObject /Subtype /Form /BBox [0 0 1 1] /Resources << /Font << /F1 4 0 R >> /XObject << /Im 5 0 R >> >> /Length {} >>\nstream\n{form}\nendstream",form.len()));
    }
    let count = objects.len() + 1;
    let mut text = String::from("%PDF-1.7\n");
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(text.len());
        text.push_str(&format!("{} 0 obj\n{object}\nendobj\n", i + 1));
    }
    let xref = text.len();
    text.push_str(&format!("xref\n0 {count}\n0000000000 65535 f \n"));
    for offset in offsets {
        text.push_str(&format!("{offset:010} 00000 n \n"));
    }
    text.push_str(&format!(
        "trailer\n<< /Size {count} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n"
    ));
    text.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bare_uris_require_an_unambiguous_destination_inside_the_same_pdf() {
        let aliases = aliases_for_uris(
            [
                "Example.org",
                "example.org/docs/",
                "absent.example",
                "https://www.example.org/",
                "https://www.example.org",
                "https://www.example.org/docs/",
            ]
            .map(str::to_string),
        );
        assert_eq!(
            aliases.get("Example.org").map(String::as_str),
            Some("https://www.example.org")
        );
        assert_eq!(
            aliases.get("example.org/docs/").map(String::as_str),
            Some("https://www.example.org/docs/")
        );
        assert!(!aliases.contains_key("absent.example"));
        let ambiguous = aliases_for_uris(
            ["example.org", "http://example.org", "https://example.org"].map(str::to_string),
        );
        assert!(ambiguous.is_empty());
        assert_ne!(uri_key("example.org/docs"), uri_key("example.org/docs/"));
    }
    #[test]
    fn invisible_text_requires_real_show_operators_and_preserves_graphics_state() {
        for data in [
            b"BT 3 Tr (Hidden) Tj ET".as_slice(),
            b"q 3 Tr BT [(Hidden) 20 (text)] TJ ET Q",
            b"3 Tr q 0 Tr Q BT (Hidden) ' ET",
            b"3 Tr BT 0 0 (Hidden) \" ET",
            b"3 Tr BT (First) Tj ET BT (Second) Tj ET",
        ] {
            assert!(invisible_text(data), "{}", String::from_utf8_lossy(data));
        }
        for data in [
            b"BT (Literal 3 Tr) Tj ET".as_slice(),
            b"BT 3 Tr () Tj [] TJ ET",
            b"q 3 Tr BT (Hidden) Tj ET Q BT (Visible) Tj ET",
            b"3 Tr BT (Hidden) Tj 0 Tr (Visible) Tj ET",
            b"BT 7 Tr (Clipping) Tj ET",
            b"% 3 Tr\nBT (Visible) Tj ET",
            b"3 Tr Q BT (Unbalanced) Tj ET",
        ] {
            assert!(!invisible_text(data), "{}", String::from_utf8_lossy(data));
        }
    }
    #[test]
    fn explicit_page_references_use_the_nested_page_tree_not_object_numbers() {
        let doc = open(&synthetic_slides()).unwrap();
        assert_eq!(
            bookmarks(&doc).iter().map(|b| b.page).collect::<Vec<_>>(),
            vec![Some(0), Some(1)]
        );
        assert_eq!(
            referenced_page(&doc, pdf_oxide::object::ObjectRef::new(70, 0), 2),
            Some(1)
        );
        let a = doc.get_annotations(0).unwrap();
        assert_eq!(link_target(&doc, &a[0], 2), Some(format!("{PAGE_LINK}2")));
        let links = lateral_links(&doc, 0);
        assert_eq!(links.len(), 1);
        assert_eq!(sidebar_text(&doc, 0, links[0].rect), "Menu");
    }
    #[test]
    fn uri_annotations_without_a_scheme_do_not_point_inside_the_bundle() {
        for uri in ["www.example.org", "../manual.pdf", "//example.org"] {
            assert_eq!(
                uri_target(uri).as_deref(),
                Some(crate::bundle::UNRESOLVED_DEST)
            );
        }
        assert_eq!(
            uri_target(" https://example.org ").as_deref(),
            Some("https://example.org")
        );
        assert_eq!(
            uri_target("mailto:autor@example.org").as_deref(),
            Some("mailto:autor@example.org")
        );
        assert_eq!(uri_target("  "), None);
    }

    fn glyph(c: char, x0: f32, x1: f32, base: f32) -> Glyph {
        Glyph {
            c,
            x0,
            x1,
            base,
            size: 10.0,
            bold: false,
            italic: false,
            mono: false,
            rotated: false,
            span: 0,
            link: None,
        }
    }

    #[test]
    fn accents_drawn_apart_are_composed_with_the_letter_below() {
        // «J», «u» con la diéresis encima, «n»; y una «e» con el agudo detrás de la coma.
        let mut g = vec![
            glyph('J', 0.0, 5.0, 100.0),
            glyph('u', 5.0, 10.5, 100.0),
            glyph('n', 10.5, 16.0, 100.0),
            glyph('\u{a8}', 6.1, 9.4, 99.9),
            glyph('R', 30.0, 35.0, 100.0),
            glyph('e', 35.0, 39.0, 100.0),
            glyph('\u{b4}', 45.0, 48.0, 100.0),
        ];
        compose_accents(&mut g);
        let text: String = g.iter().map(|g| g.c).collect();
        assert_eq!(text, "JünRe\u{b4}");
    }

    #[test]
    fn italic_is_also_read_from_the_font_style() {
        for name in [
            "ABCDEF+NimbusRomNo9L-ReguItal",
            "Helvetica-Oblique",
            "MinionPro-It",
            "MinionPro-BoldIt",
            "Times-Roman,Italic",
        ] {
            assert!(italic_font(name), "{name}");
        }
        for name in [
            "NimbusRomNo9L-Regu",
            "LMRoman10-Regular",
            "Arial-Light",
            "CMMI10",
        ] {
            assert!(!italic_font(name), "{name}");
        }
    }

    #[test]
    fn text_strings_decode_utf16_and_latin1() {
        assert_eq!(text_string(&[0xFE, 0xFF, 0x00, 0x41, 0x00, 0xF3]), "Aó");
        assert_eq!(text_string(b"Taller formaci\xF3n"), "Taller formación");
    }
}
