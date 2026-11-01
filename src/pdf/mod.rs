//! Adaptador PDF: lee cada página con pdf_oxide (caracteres, imágenes y vistas), separa
//! encabezados y pies repetidos, reconstruye el orden de lectura y los bloques con la
//! maquetación propia de `layout`, clasifica las páginas (texto digital, escaneo,
//! mixta, solo visual o en blanco) y divide el documento en conceptos por secciones.
mod layout;
mod read;

use crate::bundle::*;
use crate::worker::{Config, Rejected};
use anyhow::{Context, Result, bail};
use layout::{Block, Item, Line, Para, Region, Run};
use pdf_oxide::PdfDocument;
use read::{Bookmark, Info, Picture, Rect};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path as FsPath, PathBuf},
    time::Instant,
};
use yaml_serde::Value as YamlValue;
use zip::{ZipArchive, ZipWriter};

/// Por debajo de este número de caracteres el documento es breve y forma un solo
/// concepto aunque tenga encabezados (como en DOCX).
const BRIEF_MAX_CHARS: usize = 1500;
/// Longitud máxima de un párrafo que puede inferirse como encabezado.
const INFERRED_HEADING_MAX_CHARS: usize = 150;
/// Franja superior e inferior de la página donde se buscan encabezados y pies.
const FURNITURE_BAND: f32 = 0.12;
/// Cobertura a partir de la cual una imagen domina la página (escaneo o página mixta).
const LARGE_PICTURE: f32 = 0.3;
/// Mayor dimensión, en píxeles, de las vistas de página.
const VIEW_PX: f32 = 1600.0;
/// Por encima de este tamaño el documento se reabre en cada página, para que las
/// cachés de pdf_oxide no acumulen las imágenes de todo el archivo.
const REOPEN_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Class {
    Digital,
    Mixed,
    Scanned,
    ScannedOcr,
    Visual,
    Blank,
}

impl Class {
    fn label(self) -> &'static str {
        match self {
            Class::Digital => "texto digital",
            Class::Mixed => "mixta (texto e imagen)",
            Class::Scanned => "escaneo sin texto",
            Class::ScannedOcr => "escaneo con capa de OCR",
            Class::Visual => "solo visual",
            Class::Blank => "en blanco",
        }
    }
    fn needs_view(self) -> bool {
        matches!(self, Class::Scanned | Class::ScannedOcr | Class::Visual)
    }
}

/// Página leída: líneas sin encabezados ni pies, imágenes y clasificación.
pub(crate) struct PageInfo {
    pub(crate) height: f32,
    pub(crate) lines: Vec<Line>,
    pub(crate) rotated: Vec<Line>,
    pub(crate) pictures: Vec<Picture>,
    pub(crate) ocr_layer: bool,
    /// Rectángulos de las líneas de encabezado o pie de página.
    pub(crate) furniture: Vec<Rect>,
    pub(crate) class: Class,
    /// Etiqueta impresa (número de página visible).
    pub(crate) label: Option<String>,
    pub(crate) view: Option<String>,
    /// Ruta adjunta de cada imagen de contenido, por su posición en `pictures`.
    pub(crate) attached: HashMap<usize, String>,
    pub(crate) columns: bool,
}

/// Unidad del documento en orden de lectura, con sus páginas físicas (desde 1).
#[derive(Clone, Debug)]
pub(crate) struct Unit {
    pub(crate) kind: Kind,
    pub(crate) from: usize,
    pub(crate) to: usize,
}

#[derive(Clone, Debug)]
pub(crate) enum Kind {
    Para(Para),
    Table(Vec<Vec<String>>),
    Image(String),
    /// Página sin texto propio: en blanco, escaneada o solo visual.
    PageNote(usize),
    Rotated(String),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Heading {
    pub(crate) level: u8,
    pub(crate) inferred: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum TitleSource {
    Heading,
    LoneHeading,
    Metadata,
    Filename,
}

pub(crate) struct Section {
    pub(crate) title: String,
    pub(crate) heading: Option<usize>,
    pub(crate) from: usize,
    pub(crate) to: usize,
    pub(crate) preamble: bool,
    pub(crate) inferred: bool,
}

/// Texto repetido en encabezados o pies de página, con las páginas donde aparece.
pub(crate) struct Furniture {
    pub(crate) text: String,
    pub(crate) pages: Vec<usize>,
}

#[derive(Default)]
pub(crate) struct Tally {
    pub(crate) dehyphenated: usize,
    pub(crate) continued: Vec<(usize, usize)>,
    pub(crate) tables: Vec<usize>,
    pub(crate) images: usize,
    pub(crate) decorative: usize,
    pub(crate) broken: Vec<usize>,
    pub(crate) rotated: Vec<usize>,
    pub(crate) views_failed: Vec<usize>,
    pub(crate) unreadable: Vec<usize>,
}

pub(crate) struct Extraction {
    pub(crate) pages: Vec<PageInfo>,
    pub(crate) units: Vec<Unit>,
    pub(crate) furniture: Vec<Furniture>,
    pub(crate) furniture_images: Vec<(String, Vec<usize>)>,
    pub(crate) bookmarks: Vec<Bookmark>,
    pub(crate) internal_labels: Vec<String>,
    pub(crate) info: Info,
    pub(crate) tally: Tally,
}

/// Normaliza un texto para compararlo: minúsculas, sin espacios repetidos, guiones y
/// comillas unificados.
fn norm(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\u{00A0}' | '\u{2007}' | '\u{202F}' | '\t' => ' ',
            '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' => '-',
            '\u{2019}' | '\u{2018}' => '\'',
            c => c,
        })
        .filter(|c| !matches!(c, '\u{200B}' | '\u{00AD}' | '\u{FEFF}'))
        .collect::<String>()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches(['.', ':', ' '])
        .to_string()
}

/// Clave de un encabezado o pie: el número de la página física no cuenta, para
/// reconocer «Informe · 3» e «Informe · 4» como el mismo texto; los demás números sí
/// («5 min» y «10 min» son textos distintos).
fn furniture_key(s: &str, page: usize) -> String {
    let n = norm(s);
    if roman(&n).is_some() {
        return "#".into();
    }
    let page = page.to_string();
    let mut out = String::new();
    let mut digits = String::new();
    for c in n.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_digit() {
            digits.push(c);
            continue;
        }
        if !digits.is_empty() {
            let first = digits == page && !out.contains('#');
            out.push_str(if first { "#" } else { &digits });
            digits.clear();
        }
        out.push(c);
    }
    out.trim_end().to_string()
}

/// Texto de un encabezado o pie para documento.md: el número de la página física se
/// escribe «N», de modo que «Página 3 de 9» y «Página 4 de 9» se registran una vez.
fn furniture_text(s: &str, page: usize) -> String {
    let page = page.to_string();
    let mut out = String::new();
    let mut digits = String::new();
    let mut replaced = false;
    let text = s.split_whitespace().collect::<Vec<_>>().join(" ");
    for c in text.chars().chain(std::iter::once('\0')) {
        if c.is_ascii_digit() {
            digits.push(c);
            continue;
        }
        if !digits.is_empty() {
            let first = digits == page && !replaced;
            replaced |= first;
            out.push_str(if first { "N" } else { &digits });
            digits.clear();
        }
        if c != '\0' {
            out.push(c);
        }
    }
    out
}

fn roman(s: &str) -> Option<u32> {
    let s = s.trim().to_ascii_lowercase();
    if s.is_empty() || s.len() > 8 || !s.chars().all(|c| "ivxlcdm".contains(c)) {
        return None;
    }
    let value = |c: char| match c {
        'i' => 1,
        'v' => 5,
        'x' => 10,
        'l' => 50,
        'c' => 100,
        'd' => 500,
        _ => 1000,
    };
    let v: Vec<u32> = s.chars().map(value).collect();
    let mut total = 0i64;
    for (k, &x) in v.iter().enumerate() {
        if v.get(k + 1).is_some_and(|&n| n > x) {
            total -= x as i64;
        } else {
            total += x as i64;
        }
    }
    (total > 0).then_some(total as u32)
}

/// Número de página impreso en una línea: «12», «xii», «- 12 -», «Página 12 de 40»,
/// «Page 4 of 9» o «4 / 9», también al final de un encabezado. Devuelve la etiqueta
/// literal y el total si lo indica.
fn printed_number(text: &str) -> Option<(String, Option<usize>)> {
    let t = text.trim();
    let words: Vec<&str> = t.split_whitespace().collect();
    let digits = |w: &str| w.len() <= 5 && !w.is_empty() && w.chars().all(|c| c.is_ascii_digit());
    match words.as_slice() {
        [w] if digits(w) || roman(w).is_some() => return Some((w.to_string(), None)),
        ["-" | "–" | "—", w, "-" | "–" | "—"] if digits(w) => {
            return Some((w.to_string(), None));
        }
        [a, "/" | "of" | "de", b] if digits(a) && digits(b) => {
            return Some((a.to_string(), b.parse().ok()));
        }
        _ => {}
    }
    // «… Página 12 de 40» o «… Page 12».
    let n = words.len();
    for k in (0..n).rev().take(4) {
        let w = words[k].to_lowercase();
        if matches!(
            w.as_str(),
            "page" | "página" | "pagina" | "pág." | "pág" | "p."
        ) && let Some(num) = words.get(k + 1).filter(|w| digits(w))
        {
            let total = match words.get(k + 2..k + 4) {
                Some([sep, tot]) if matches!(*sep, "of" | "de" | "/") && digits(tot) => {
                    tot.parse().ok()
                }
                _ => None,
            };
            let consumed = k + 2 + if total.is_some() { 2 } else { 0 };
            if consumed == n {
                return Some((num.to_string(), total));
            }
        }
    }
    None
}

/// Lee todas las páginas, escribe en el ZIP las imágenes y vistas que se adjuntan y
/// devuelve las unidades del documento en orden de lectura.
pub(crate) fn extract(
    bytes: &[u8],
    max_pages: usize,
    z: &mut ZipWriter<fs::File>,
) -> Result<Extraction> {
    let reopen = bytes.len() > REOPEN_BYTES;
    let first = read::open(bytes)?;
    let n = read::page_count(&first)?;
    if n == 0 {
        return Err(Rejected("el PDF no tiene páginas".into()).into());
    }
    if n > max_pages {
        return Err(Rejected(format!(
            "el PDF tiene {n} páginas y el límite es {max_pages} (OKF_MAX_PAGES)"
        ))
        .into());
    }
    let info = read::info(&first);
    let bookmarks = read::bookmarks(&first);
    let internal_labels = read::internal_labels(&first, n);
    // Un documento grande se reabre en cada página y no se conserva el primero.
    let doc = (!reopen).then_some(first);
    let with_doc = |f: &mut dyn FnMut(&PdfDocument) -> Result<()>| -> Result<()> {
        match &doc {
            Some(d) => f(d),
            None => f(&read::open(bytes)?),
        }
    };
    let mut tally = Tally::default();
    // 1. Líneas e imágenes de cada página.
    let mut pages: Vec<PageInfo> = Vec::with_capacity(n);
    for p in 0..n {
        let mut raw = None;
        with_doc(&mut |d| {
            raw = Some(read::page(d, p)?);
            Ok(())
        })?;
        let raw = raw.expect("página leída");
        if raw.glyphs.iter().any(|g| g.c == '\u{FFFD}') {
            tally.unreadable.push(p + 1);
        }
        let (lines, rotated) = layout::lines(&raw.glyphs);
        if !rotated.is_empty() {
            tally.rotated.push(p + 1);
        }
        tally.decorative += raw.pictures.iter().filter(|x| x.hash.is_none()).count();
        pages.push(PageInfo {
            height: raw.height,
            lines,
            rotated,
            pictures: raw.pictures,
            ocr_layer: raw.ocr_layer,
            furniture: Vec::new(),
            class: Class::Digital,
            label: None,
            view: None,
            attached: HashMap::new(),
            columns: false,
        });
    }
    // 2. Encabezados y pies de página: textos de las franjas superior e inferior que se
    //    repiten, y números de página impresos.
    let mut keys: HashMap<(bool, String), Vec<usize>> = HashMap::new();
    for (p, page) in pages.iter().enumerate() {
        let mut seen = HashSet::new();
        for line in &page.lines {
            let top = line.bottom() < FURNITURE_BAND * page.height;
            let bottom = line.top() > (1.0 - FURNITURE_BAND) * page.height;
            if !(top || bottom) {
                continue;
            }
            // Dos claves: el texto literal y el texto sin el número de la página.
            for key in [norm(&line.text()), furniture_key(&line.text(), p + 1)] {
                if seen.insert((top, key.clone())) {
                    keys.entry((top, key)).or_default().push(p);
                }
            }
        }
    }
    let threshold = 2.max((n as f32 * 0.3).ceil() as usize);
    let repeated = |top: bool, text: &str, page: usize| {
        n >= 2
            && [norm(text), furniture_key(text, page)].iter().any(|k| {
                keys.get(&(top, k.clone()))
                    .is_some_and(|v| v.len() >= threshold)
            })
    };
    // Números impresos: se aceptan los que mantienen la misma diferencia con la página
    // física en al menos dos páginas, o los que declaran el total de páginas.
    let mut candidates: Vec<(usize, usize, String, i64)> = Vec::new();
    for (p, page) in pages.iter().enumerate() {
        for (i, line) in page.lines.iter().enumerate() {
            let in_band = line.bottom() < FURNITURE_BAND * page.height
                || line.top() > (1.0 - FURNITURE_BAND) * page.height;
            if !in_band || n < 2 {
                continue;
            }
            if let Some((label, total)) = printed_number(&line.text()) {
                let value = label
                    .parse::<i64>()
                    .ok()
                    .or_else(|| roman(&label).map(i64::from));
                let Some(value) = value else { continue };
                let offset = if total == Some(n) {
                    i64::MIN
                } else {
                    value - (p as i64 + 1) + if roman(&label).is_some() { 1 << 32 } else { 0 }
                };
                candidates.push((p, i, label, offset));
            }
        }
    }
    let mut offsets: HashMap<i64, HashSet<usize>> = HashMap::new();
    for (p, _, _, o) in &candidates {
        offsets.entry(*o).or_default().insert(*p);
    }
    let mut numbered: HashSet<(usize, usize)> = HashSet::new();
    for (p, i, label, o) in &candidates {
        if *o == i64::MIN || offsets[o].len() >= 2 {
            numbered.insert((*p, *i));
            if pages[*p].label.is_none() {
                pages[*p].label = Some(label.clone());
            }
        }
    }
    let mut furniture: Vec<Furniture> = Vec::new();
    for (p, page) in pages.iter_mut().enumerate() {
        let height = page.height;
        let mut kept = Vec::with_capacity(page.lines.len());
        for (i, line) in std::mem::take(&mut page.lines).into_iter().enumerate() {
            let top = line.bottom() < FURNITURE_BAND * height;
            let bottom = line.top() > (1.0 - FURNITURE_BAND) * height;
            let text = line.text();
            if numbered.contains(&(p, i)) || ((top || bottom) && repeated(top, &text, p + 1)) {
                page.furniture.push(line.rect());
                // El texto literal si se repite tal cual; si no, sin el número de página.
                let literal = keys
                    .get(&(top, norm(&text)))
                    .is_some_and(|v| v.len() >= threshold);
                let shown = if literal {
                    text.split_whitespace().collect::<Vec<_>>().join(" ")
                } else {
                    furniture_text(&text, p + 1)
                };
                // Un número de página suelto queda en `page_labels`, no en la lista.
                if !numbered.contains(&(p, i)) || text.chars().any(char::is_alphabetic) {
                    match furniture.iter_mut().find(|f| f.text == shown) {
                        Some(f) => {
                            if f.pages.last() != Some(&(p + 1)) {
                                f.pages.push(p + 1);
                            }
                        }
                        None => furniture.push(Furniture {
                            text: shown,
                            pages: vec![p + 1],
                        }),
                    }
                }
            } else {
                kept.push(line);
            }
        }
        page.lines = kept;
    }
    // 3. Imágenes repetidas en muchas páginas (logotipos, fondos de plantilla): se
    //    registran una vez en documento.md.
    let mut by_hash: HashMap<String, Vec<usize>> = HashMap::new();
    for (p, page) in pages.iter().enumerate() {
        let mut seen = HashSet::new();
        for pic in &page.pictures {
            if let Some(h) = &pic.hash
                && seen.insert(h.clone())
            {
                by_hash.entry(h.clone()).or_default().push(p);
            }
        }
    }
    let template = |h: &str| {
        by_hash
            .get(h)
            .is_some_and(|v| v.len() >= 2 && (v.len() >= 3 || v.len() * 2 >= n))
    };
    // 4. Clasificación de cada página.
    for (p, page) in pages.iter_mut().enumerate() {
        let text_chars: usize = page
            .lines
            .iter()
            .chain(&page.rotated)
            .map(|l| l.text().chars().filter(|c| !c.is_whitespace()).count())
            .sum();
        let large = page
            .pictures
            .iter()
            .filter(|x| x.hash.as_deref().is_some_and(|h| !template(h)))
            .map(|x| x.coverage)
            .fold(0.0f32, f32::max);
        let class = if text_chars == 0 {
            if large >= LARGE_PICTURE {
                Class::Scanned
            } else {
                let mut ignore = page.furniture.clone();
                ignore.extend(
                    page.pictures
                        .iter()
                        .filter(|x| x.hash.is_none())
                        .map(|x| x.rect),
                );
                let mut blank = true;
                with_doc(&mut |d| {
                    blank = read::looks_blank(d, p, &ignore).unwrap_or(false);
                    Ok(())
                })?;
                if blank && large == 0.0 {
                    Class::Blank
                } else {
                    Class::Visual
                }
            }
        } else if page.ocr_layer && large >= LARGE_PICTURE {
            Class::ScannedOcr
        } else if large >= LARGE_PICTURE {
            Class::Mixed
        } else {
            Class::Digital
        };
        page.class = class;
    }
    // 5. Imágenes adjuntas y vistas de las páginas que no se pueden transcribir.
    let mut written: HashMap<String, String> = HashMap::new();
    let mut furniture_images: Vec<(String, Vec<usize>)> = Vec::new();
    for (p, page) in pages.iter_mut().enumerate() {
        let class = page.class;
        let wanted: Vec<(usize, usize, String)> = page
            .pictures
            .iter()
            .enumerate()
            .filter_map(|(k, x)| x.hash.clone().map(|h| (k, x.index, h)))
            .filter(|(_, _, h)| template(h) || !class.needs_view())
            .collect();
        let missing: Vec<usize> = wanted
            .iter()
            .filter(|(_, _, h)| !written.contains_key(h))
            .map(|(_, i, _)| *i)
            .collect();
        let mut encoded = Vec::new();
        let mut view = None;
        with_doc(&mut |d| {
            if !missing.is_empty() {
                encoded = read::encoded(d, p, &missing);
            }
            if class.needs_view() {
                view = Some(read::render_png(d, p, VIEW_PX));
            }
            Ok(())
        })?;
        let mut encoded = missing.iter().zip(encoded);
        let mut ordinal = 0;
        for (k, index, hash) in wanted {
            let path = match written.get(&hash) {
                Some(path) => path.clone(),
                None => {
                    let Some((_, e)) = encoded.find(|(i, _)| **i == index) else {
                        continue;
                    };
                    let Some(e) = e else {
                        tally.broken.push(p + 1);
                        continue;
                    };
                    ordinal += 1;
                    let path = format!("assets/pagina-{:03}-imagen-{ordinal:02}.{}", p + 1, e.ext);
                    write_zip_file(z, &path, &e.bytes)?;
                    written.insert(hash.clone(), path.clone());
                    path
                }
            };
            if template(&hash) {
                match furniture_images.iter_mut().find(|(x, _)| *x == path) {
                    Some((_, v)) => v.push(p + 1),
                    None => furniture_images.push((path, vec![p + 1])),
                }
            } else {
                page.attached.insert(k, path);
            }
        }
        match view {
            Some(Ok(png)) => {
                let path = format!("assets/pagina-{:03}.png", p + 1);
                write_zip_file(z, &path, &png)?;
                page.view = Some(path);
            }
            Some(Err(_)) => tally.views_failed.push(p + 1),
            None => {}
        }
    }
    tally.broken.dedup();
    // 6. Orden de lectura y bloques de cada página.
    let mut units: Vec<Unit> = Vec::new();
    let mut leaves_of: Vec<(usize, usize)> = Vec::new();
    for (p, page) in pages.iter_mut().enumerate() {
        let start = units.len();
        if page.class != Class::Digital && page.class != Class::Mixed {
            units.push(Unit {
                kind: Kind::PageNote(p + 1),
                from: p + 1,
                to: p + 1,
            });
            leaves_of.push((p, usize::MAX));
        }
        if matches!(page.class, Class::Scanned | Class::Visual | Class::Blank) {
            continue;
        }
        // Las imágenes que tapan texto (fondos) no participan en los cortes; se colocan
        // antes del primer bloque que empieza debajo de su borde superior.
        let text_rects: Vec<Rect> = page.lines.iter().map(Line::rect).collect();
        let mut inline: Vec<(usize, Rect)> = Vec::new();
        let mut floating: Vec<(usize, Rect)> = Vec::new();
        let mut keys: Vec<&usize> = page.attached.keys().collect();
        keys.sort();
        for &k in keys {
            let r = page.pictures[k].rect;
            if text_rects.iter().any(|t| t.overlaps(&r)) {
                floating.push((k, r));
            } else {
                inline.push((k, r));
            }
        }
        let rects: Vec<Rect> = inline.iter().map(|(_, r)| *r).collect();
        let region = Region {
            lines: &page.lines,
            pictures: &rects,
        };
        let mut items: Vec<Item> = (0..page.lines.len()).map(Item::Line).collect();
        items.extend((0..rects.len()).map(Item::Picture));
        let leaves = layout::reading_order(&region, items);
        page.columns = columns(&region, &leaves);
        let blocks = layout::blocks(&region, &leaves);
        let mut page_units: Vec<(Unit, Rect, usize)> = Vec::new();
        for (block, rect, leaf) in blocks {
            let kind = match block {
                Block::Para(para) => Kind::Para(para),
                Block::Table(rows) => {
                    tally.tables.push(p + 1);
                    Kind::Table(rows)
                }
                Block::Picture(i) => Kind::Image(page.attached[&inline[i].0].clone()),
            };
            page_units.push((
                Unit {
                    kind,
                    from: p + 1,
                    to: p + 1,
                },
                rect,
                leaf,
            ));
        }
        for (k, r) in floating {
            let unit = Unit {
                kind: Kind::Image(page.attached[&k].clone()),
                from: p + 1,
                to: p + 1,
            };
            let at = page_units
                .iter()
                .position(|(_, b, _)| b.top >= r.top - 1.0)
                .unwrap_or(page_units.len());
            page_units.insert(at, (unit, r, usize::MAX - 1));
        }
        for (u, _, leaf) in page_units {
            units.push(u);
            leaves_of.push((p, leaf));
        }
        for line in &page.rotated {
            units.push(Unit {
                kind: Kind::Rotated(line.text()),
                from: p + 1,
                to: p + 1,
            });
            leaves_of.push((p, usize::MAX - 2));
        }
        if units.len() == start {
            // Página con texto solo en encabezados o pies.
            page.class = Class::Blank;
            units.push(Unit {
                kind: Kind::PageNote(p + 1),
                from: p + 1,
                to: p + 1,
            });
            leaves_of.push((p, usize::MAX));
        }
    }
    tally.tables.dedup();
    // 7. Párrafos que continúan en otra columna o página: la línea anterior no termina
    //    la frase y la siguiente empieza en minúscula, con el mismo formato. Las notas
    //    en letra menor que quedan en medio (afiliaciones, notas al pie) se dejan
    //    después del párrafo unido.
    let mut merged: Vec<(Unit, (usize, usize))> = Vec::with_capacity(units.len());
    for (u, leaf) in units.into_iter().zip(leaves_of) {
        if let Kind::Para(b) = &u.kind
            && leaf.1 < usize::MAX - 2
            && b.marker.is_none()
            && b.lines[0]
                .text()
                .chars()
                .next()
                .is_some_and(char::is_lowercase)
        {
            let mut j = merged.len();
            while j > 0
                && merged.len() - j < 3
                && matches!(&merged[j - 1].0.kind, Kind::Para(n) if n.size < 0.9 * b.size && merged[j - 1].0.from + 1 >= u.from)
            {
                j -= 1;
            }
            if j > 0
                && let (prev, prev_leaf) = &mut merged[j - 1]
                && *prev_leaf != leaf
                && let Kind::Para(a) = &mut prev.kind
                && a.marker.is_none()
                && (a.size - b.size).abs() < 0.5
                && a.bold == b.bold
                && a.mono == b.mono
                && !terminal_text(&a.last().text())
            {
                tally.continued.push(if u.from != prev.to {
                    (prev.to, u.from)
                } else {
                    (u.from, u.from)
                });
                a.lines.extend(b.lines.iter().cloned());
                prev.to = u.to;
                *prev_leaf = leaf;
                continue;
            }
        }
        merged.push((u, leaf));
    }
    let mut merged: Vec<Unit> = merged.into_iter().map(|(u, _)| u).collect();
    // Guion final de línea: se conserva si la palabra compuesta aparece con guion dentro
    // de alguna línea del documento (`human-generated`), se suprime si la palabra unida
    // aparece entera (`Chat-` + `GPT`) y, si no, se une solo entre minúsculas.
    let mut compounds: HashSet<String> = HashSet::new();
    let mut words: HashSet<String> = HashSet::new();
    for u in &merged {
        if let Kind::Para(p) = &u.kind {
            for line in &p.lines {
                let t = line.text().to_lowercase();
                words.extend(
                    t.split(|c: char| !c.is_alphanumeric())
                        .filter(|w| w.chars().count() > 2)
                        .map(str::to_string),
                );
                let chars: Vec<char> = t.chars().collect();
                for (i, &c) in chars.iter().enumerate() {
                    if c == '-'
                        && i > 0
                        && chars[i - 1].is_alphabetic()
                        && chars.get(i + 1).is_some_and(|c| c.is_alphabetic())
                    {
                        let l: String = chars[..i]
                            .iter()
                            .rev()
                            .take_while(|c| c.is_alphabetic())
                            .collect::<Vec<_>>()
                            .into_iter()
                            .rev()
                            .collect();
                        let r: String = chars[i + 1..]
                            .iter()
                            .take_while(|c| c.is_alphabetic())
                            .collect();
                        compounds.insert(format!("{l}-{r}"));
                    }
                }
            }
        }
    }
    for u in &mut merged {
        if let Kind::Para(p) = &mut u.kind {
            for k in 0..p.lines.len().saturating_sub(1) {
                let t = p.lines[k].text().to_lowercase();
                let Some(head) = t.strip_suffix('-') else {
                    continue;
                };
                let l: String = head
                    .chars()
                    .rev()
                    .take_while(|c| c.is_alphabetic())
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                let next = p.lines[k + 1].text().to_lowercase();
                let r: String = next.chars().take_while(|c| c.is_alphabetic()).collect();
                if l.is_empty() || r.is_empty() {
                    continue;
                }
                if compounds.contains(&format!("{l}-{r}")) {
                    p.keep_hyphen.push(k);
                } else if words.contains(&format!("{l}{r}")) {
                    p.join_hyphen.push(k);
                }
            }
            tally.dehyphenated += layout::join_runs(&p.runs()).1;
        }
    }
    tally.images = written.len() - furniture_images.len();
    Ok(Extraction {
        pages,
        units: merged,
        furniture,
        furniture_images,
        bookmarks,
        internal_labels,
        info,
        tally,
    })
}

fn terminal_text(s: &str) -> bool {
    s.trim_end()
        .ends_with(['.', '!', '?', ':', ';', '…', '»', '"', '”', ')'])
}

/// ¿La página se leyó en varias columnas? Hay al menos dos hojas de texto corrido una
/// junto a otra.
fn columns(region: &Region, leaves: &[layout::Leaf]) -> bool {
    let spans: Vec<(f32, f32, f32, f32)> = leaves
        .iter()
        .filter(|l| !l.row && l.items.len() >= 3)
        .map(|l| {
            let rects: Vec<Rect> = l
                .items
                .iter()
                .map(|&i| match i {
                    Item::Line(k) => region.lines[k].rect(),
                    Item::Picture(k) => region.pictures[k],
                })
                .collect();
            (
                rects.iter().map(|r| r.x0).fold(f32::MAX, f32::min),
                rects.iter().map(|r| r.x1).fold(f32::MIN, f32::max),
                rects.iter().map(|r| r.top).fold(f32::MAX, f32::min),
                rects.iter().map(|r| r.bottom).fold(f32::MIN, f32::max),
            )
        })
        .collect();
    spans.iter().enumerate().any(|(i, a)| {
        spans[i + 1..]
            .iter()
            .any(|b| (a.1 <= b.0 || b.1 <= a.0) && a.2 < b.3 && b.2 < a.3)
    })
}

/// Encabezados, título y división en secciones.
pub(crate) struct Outline {
    pub(crate) headings: Vec<Option<Heading>>,
    pub(crate) title: String,
    pub(crate) title_unit: Option<usize>,
    pub(crate) title_source: TitleSource,
    /// Los encabezados proceden del esquema (marcadores) del PDF.
    pub(crate) from_bookmarks: bool,
    pub(crate) unmatched_bookmarks: usize,
    pub(crate) split: Option<u8>,
    pub(crate) sections: Vec<Section>,
}

impl Outline {
    fn hashes(&self, level: u8) -> String {
        let base = self.split.unwrap_or(0);
        "#".repeat((level.saturating_sub(base) as usize + 1).clamp(2, 6))
    }
    pub(crate) fn file(&self, k: usize) -> String {
        if self.split.is_none() {
            "texto.md".into()
        } else {
            format!("seccion-{:03}.md", k + 1)
        }
    }
}

fn para_text(u: &Unit) -> Option<String> {
    match &u.kind {
        Kind::Para(p) => Some(match &p.marker {
            Some(m) => format!("{m} {}", p.text()),
            None => p.text(),
        }),
        _ => None,
    }
}

fn unit_chars(u: &Unit) -> usize {
    match &u.kind {
        Kind::Para(_) => para_text(u).map_or(0, |t| t.chars().count()),
        Kind::Table(rows) => rows.iter().flatten().map(|c| c.chars().count()).sum(),
        Kind::Rotated(t) => t.chars().count(),
        _ => 0,
    }
}

pub(crate) fn outline(ex: &mut Extraction, filename: &str) -> Outline {
    // Encabezados del esquema: cada marcador se busca, en orden, como un párrafo con el
    // mismo texto (o como sus primeras líneas) a partir de su página de destino.
    let mut headings: Vec<Option<Heading>> = vec![None; ex.units.len()];
    let mut matched = 0;
    let mut cursor = 0;
    let mut splits: Vec<(usize, usize)> = Vec::new();
    for b in &ex.bookmarks {
        let want = norm(&b.title);
        if want.is_empty() {
            continue;
        }
        let found = (cursor..ex.units.len()).find_map(|i| {
            let u = &ex.units[i];
            if b.page.is_some_and(|p| u.to < p + 1) {
                return None;
            }
            let Kind::Para(p) = &u.kind else { return None };
            if norm(&para_text(u)?) == want {
                return Some((i, p.lines.len()));
            }
            // Las primeras líneas del párrafo forman el título.
            let mut acc = String::new();
            for (k, line) in p.lines.iter().enumerate().take(p.lines.len() - 1) {
                acc.push_str(&line.text());
                acc.push(' ');
                if norm(&acc) == want {
                    return Some((i, k + 1));
                }
                if !want.starts_with(&norm(&acc)) {
                    break;
                }
            }
            None
        });
        if let Some((i, k)) = found {
            matched += 1;
            cursor = i + 1;
            splits.push((i, k));
            headings[i] = Some(Heading {
                level: b.depth + 1,
                inferred: false,
            });
        }
    }
    let usable = ex
        .bookmarks
        .iter()
        .filter(|b| !norm(&b.title).is_empty())
        .count();
    let from_bookmarks = usable >= 2 && matched * 5 >= usable * 4;
    if from_bookmarks {
        // Un párrafo cuyo comienzo es el título de un marcador se divide.
        for &(i, k) in splits.iter().rev() {
            let Kind::Para(p) = &ex.units[i].kind else {
                continue;
            };
            if k >= p.lines.len() {
                continue;
            }
            let mut head = p.clone();
            let mut tail = p.clone();
            head.lines.truncate(k);
            tail.lines.drain(..k);
            tail.marker = None;
            let rest = Unit {
                kind: Kind::Para(tail),
                from: ex.units[i].from,
                to: ex.units[i].to,
            };
            ex.units[i].kind = Kind::Para(head);
            ex.units.insert(i + 1, rest);
            headings.insert(i + 1, None);
        }
    } else {
        headings = vec![None; ex.units.len()];
        // Tamaño del texto corriente: el más frecuente por caracteres.
        let mut weights: HashMap<i32, usize> = HashMap::new();
        for u in &ex.units {
            if let Kind::Para(p) = &u.kind {
                *weights.entry((p.size * 2.0).round() as i32).or_default() += unit_chars(u);
            }
        }
        let body = weights
            .iter()
            .max_by_key(|(s, w)| (**w, std::cmp::Reverse(**s)))
            .map(|(s, _)| *s);
        let candidate = |u: &Unit| -> Option<i32> {
            let Kind::Para(p) = &u.kind else { return None };
            let t = para_text(u)?;
            let size = (p.size * 2.0).round() as i32;
            (t.chars().count() <= INFERRED_HEADING_MAX_CHARS
                && p.lines.len() <= 3
                && !p.mono
                && t.chars().any(char::is_alphabetic)
                && !t.trim_end().ends_with(['.', ',', ';', ':'])
                && body.is_some_and(|b| size as f32 >= b as f32 * 1.1))
            .then_some(size)
        };
        let mut sizes: Vec<i32> = ex.units.iter().filter_map(candidate).collect();
        sizes.sort_unstable_by(|a, b| b.cmp(a));
        sizes.dedup();
        let levels: HashMap<i32, u8> = sizes
            .into_iter()
            .take(6)
            .enumerate()
            .map(|(k, s)| (s, k as u8 + 1))
            .collect();
        for (i, u) in ex.units.iter().enumerate() {
            if let Some(level) = candidate(u).and_then(|s| levels.get(&s)) {
                headings[i] = Some(Heading {
                    level: *level,
                    inferred: true,
                });
            }
        }
    }
    let n = ex.units.len();
    let has_text = |i: usize| unit_chars(&ex.units[i]) > 0;
    // Título: el primer encabezado de la primera página, si le sigue directamente otro
    // de igual o mayor rango o si es el único de su nivel y supera a todos los demás.
    let mut title_unit = None;
    let mut title_source = TitleSource::Filename;
    if let Some(f) = (0..n).find(|&i| headings[i].is_some() && ex.units[i].from == 1) {
        let h = headings[f].expect("encabezado");
        let next = (f + 1..n).find(|&i| has_text(i));
        let first_text = (0..n).find(|&i| has_text(i)) == Some(f);
        if first_text && next.is_some_and(|x| headings[x].is_some_and(|y| y.level <= h.level)) {
            title_unit = Some(f);
            title_source = TitleSource::Heading;
        } else if headings
            .iter()
            .enumerate()
            .all(|(i, x)| i == f || x.is_none_or(|x| x.level > h.level))
        {
            title_unit = Some(f);
            title_source = TitleSource::LoneHeading;
        }
    }
    let title = match title_unit {
        Some(t) => para_text(&ex.units[t]).unwrap_or_default(),
        None => match &ex.info.title {
            Some(t) => {
                title_source = TitleSource::Metadata;
                t.clone()
            }
            None => filename.to_string(),
        },
    };
    let total: usize = ex.units.iter().map(unit_chars).sum();
    let mut counts: HashMap<u8, usize> = HashMap::new();
    for (i, h) in headings.iter().enumerate() {
        if let Some(h) = h
            && Some(i) != title_unit
        {
            *counts.entry(h.level).or_default() += 1;
        }
    }
    // División: el nivel más alto con al menos dos encabezados, salvo que su primer
    // encabezado llegue después de otros de menor rango (un «¡Gracias!» de cierre en
    // letra más grande que los títulos de cada parte no divide el documento).
    let first_of = |level: u8| {
        (0..n).find(|&i| Some(i) != title_unit && headings[i].is_some_and(|h| h.level == level))
    };
    let split = if total < BRIEF_MAX_CHARS {
        None
    } else {
        let mut levels: Vec<u8> = counts
            .iter()
            .filter(|(_, c)| **c >= 2)
            .map(|(l, _)| *l)
            .collect();
        levels.sort_unstable();
        levels
            .iter()
            .copied()
            .find(|&l| {
                let first = first_of(l).unwrap_or(0);
                !(0..first)
                    .any(|i| Some(i) != title_unit && headings[i].is_some_and(|h| h.level > l))
            })
            .or(levels.first().copied())
    };
    let mut sections = Vec::new();
    let body: Vec<usize> = (0..n).filter(|&i| Some(i) != title_unit).collect();
    match split {
        None => sections.push(Section {
            title: title.clone(),
            heading: None,
            from: 0,
            to: n.saturating_sub(1),
            preamble: false,
            inferred: false,
        }),
        Some(l) => {
            let starts: Vec<usize> = body
                .iter()
                .copied()
                .filter(|&i| headings[i].is_some_and(|h| h.level == l))
                .collect();
            let first = starts[0];
            if body.iter().any(|&i| i < first) {
                let from = body[0];
                sections.push(Section {
                    title: title.clone(),
                    heading: None,
                    from,
                    to: first - 1,
                    preamble: true,
                    inferred: false,
                });
            }
            for (k, &s) in starts.iter().enumerate() {
                let to = starts.get(k + 1).map_or(n - 1, |x| x - 1);
                sections.push(Section {
                    title: para_text(&ex.units[s]).unwrap_or_default(),
                    heading: Some(s),
                    from: s,
                    to,
                    preamble: false,
                    inferred: headings[s].is_some_and(|h| h.inferred),
                });
            }
        }
    }
    Outline {
        headings,
        title,
        title_unit,
        title_source,
        from_bookmarks,
        unmatched_bookmarks: if from_bookmarks { usable - matched } else { 0 },
        split,
        sections,
    }
}

/// Texto de los tramos con su énfasis en Markdown.
fn runs_md(runs: &[Run]) -> String {
    let (runs, _) = layout::join_runs(runs);
    let mut out = String::new();
    for r in &runs {
        let text = escape_inline(&r.text);
        let core = text.trim();
        if core.is_empty() || (!r.bold && !r.italic) {
            out.push_str(&text);
            continue;
        }
        let lead = &text[..text.len() - text.trim_start().len()];
        let trail = &text[text.trim_end().len()..];
        let mark = match (r.bold, r.italic) {
            (true, true) => "***",
            (true, false) => "**",
            _ => "*",
        };
        out.push_str(&format!("{lead}{mark}{core}{mark}{trail}"));
    }
    out
}

struct Render<'a> {
    ex: &'a Extraction,
    outline: &'a Outline,
    inferred: Vec<usize>,
}

impl Render<'_> {
    fn marker(
        out: &mut String,
        spans: &mut Vec<(String, usize, usize)>,
        from: usize,
        to: usize,
        multi: bool,
    ) {
        if !multi {
            return;
        }
        let id = if from == to {
            format!("p-{from}")
        } else {
            format!("p-{from}-{to}")
        };
        if !spans.iter().any(|(s, _, _)| *s == id) {
            spans.push((id.clone(), from, to));
        }
        out.push_str(&format!("<!-- okf-source-span: {id} -->\n"));
    }

    fn page_note(&self, p: usize) -> String {
        let page = &self.ex.pages[p - 1];
        let view = page
            .view
            .as_ref()
            .map(|v| format!("![Vista de la página {p}](../{v})\n"))
            .unwrap_or_default();
        let note = match page.class {
            Class::Scanned => "> Página escaneada sin capa de texto: su contenido solo está en la imagen; no se aplicó OCR.".to_string(),
            Class::ScannedOcr => "> Página escaneada con una capa de texto de OCR previo: el texto que sigue procede de esa capa y su exactitud no se ha verificado.".to_string(),
            Class::Visual => "> Página sin texto con contenido gráfico: se conserva su vista.".to_string(),
            _ if !page.furniture.is_empty() => format!("_Página {p} sin contenido propio: solo tiene encabezado, pie o número de página._"),
            _ => format!("_Página {p} en blanco._"),
        };
        if page.class.needs_view() && page.view.is_none() {
            return format!("{note}\n> No se pudo generar la vista de la página {p}.\n");
        }
        format!("{view}{note}\n")
    }

    fn concept(&mut self, k: usize, filename: &str, generated: &str) -> String {
        let ex = self.ex;
        let outline = self.outline;
        let sec = &outline.sections[k];
        let units = &ex.units;
        let range: Vec<usize> = (sec.from..=sec.to.min(units.len().saturating_sub(1)))
            .filter(|&i| Some(i) != outline.title_unit)
            .collect();
        let pages_from = range.iter().map(|&i| units[i].from).min().unwrap_or(1);
        let pages_to = range
            .iter()
            .map(|&i| units[i].to)
            .max()
            .unwrap_or(pages_from);
        let multi = pages_from != pages_to;
        let mut body = String::new();
        let mut spans: Vec<(String, usize, usize)> = Vec::new();
        if sec.inferred
            && let Some(h) = sec.heading
        {
            self.inferred.push(units[h].from);
        }
        let mut idx = 0;
        while idx < range.len() {
            let i = range[idx];
            let u = &units[i];
            if Some(i) == sec.heading {
                idx += 1;
                continue;
            }
            if let Some(h) = outline.headings[i] {
                if h.inferred {
                    self.inferred.push(u.from);
                }
                Self::marker(&mut body, &mut spans, u.from, u.to, multi);
                body.push_str(&format!(
                    "{} {}\n\n",
                    outline.hashes(h.level),
                    escape_inline(&para_text(u).unwrap_or_default())
                ));
                idx += 1;
                continue;
            }
            match &u.kind {
                Kind::Para(p) if p.marker.is_some() => {
                    // Una lista contigua lleva un solo marcador.
                    let mut j = idx;
                    while j + 1 < range.len()
                        && outline.headings[range[j + 1]].is_none()
                        && matches!(&units[range[j + 1]].kind, Kind::Para(q) if q.marker.is_some())
                    {
                        j += 1;
                    }
                    let (a, b) = (u.from, units[range[j]].to);
                    Self::marker(&mut body, &mut spans, a, b, multi);
                    for &x in &range[idx..=j] {
                        let Kind::Para(q) = &units[x].kind else {
                            continue;
                        };
                        let label = q.marker.as_deref().unwrap_or("-");
                        let numbered = label.len() <= 4
                            && label.ends_with(['.', ')'])
                            && label[..label.len() - 1].chars().all(|c| c.is_ascii_digit())
                            && label.len() > 1;
                        let text = runs_md(&q.runs());
                        if numbered {
                            body.push_str(&format!("{label} {text}\n"));
                        } else if label.chars().count() == 1 {
                            body.push_str(&format!("- {text}\n"));
                        } else {
                            body.push_str(&format!("- {} {text}\n", escape_inline(label)));
                        }
                    }
                    body.push('\n');
                    idx = j + 1;
                    continue;
                }
                Kind::Para(p) if p.mono => {
                    Self::marker(&mut body, &mut spans, u.from, u.to, multi);
                    // La sangría de cada línea se reconstruye con espacios del ancho de
                    // un carácter monoespaciado.
                    let left = p.lines.iter().map(|l| l.x0).fold(f32::MAX, f32::min);
                    let code: Vec<String> = p
                        .lines
                        .iter()
                        .map(|l| {
                            let indent = ((l.x0 - left) / (0.6 * l.size)).round().max(0.0) as usize;
                            format!("{}{}", " ".repeat(indent.min(40)), l.text())
                        })
                        .collect();
                    let code = code.join("\n");
                    let fence = if code.contains("```") { "~~~~" } else { "```" };
                    body.push_str(&format!("{fence}\n{code}\n{fence}\n\n"));
                }
                Kind::Para(p) => {
                    Self::marker(&mut body, &mut spans, u.from, u.to, multi);
                    body.push_str(&escape_line_start(&runs_md(&p.runs())));
                    body.push_str("\n\n");
                }
                Kind::Table(rows) => {
                    Self::marker(&mut body, &mut spans, u.from, u.to, multi);
                    let cells: Vec<Vec<String>> = rows
                        .iter()
                        .map(|r| {
                            r.iter()
                                .map(|c| escape_inline(c).replace('|', "\\|"))
                                .collect()
                        })
                        .collect();
                    body.push_str(&table_markdown(&cells));
                    body.push('\n');
                }
                Kind::Image(path) => {
                    Self::marker(&mut body, &mut spans, u.from, u.to, multi);
                    body.push_str(&format!("![Imagen de la página {}](../{path})\n\n", u.from));
                }
                Kind::PageNote(p) => {
                    Self::marker(&mut body, &mut spans, u.from, u.to, multi);
                    body.push_str(&self.page_note(*p));
                    body.push('\n');
                }
                Kind::Rotated(t) => {
                    Self::marker(&mut body, &mut spans, u.from, u.to, multi);
                    body.push_str(&format!("_Texto girado:_ {}\n\n", escape_inline(t)));
                }
            }
            idx += 1;
        }
        let first = range
            .iter()
            .filter(|&&i| outline.headings[i].is_none())
            .find_map(|&i| {
                para_text(&units[i])
                    .filter(|t| t.chars().filter(|c| c.is_alphabetic()).count() >= 3)
            });
        let mut description = match (outline.split, sec.preamble) {
            (None, _) => format!("Texto completo de {filename}."),
            (Some(_), true) => format!("Inicio de {filename}, antes de la primera sección."),
            (Some(_), false) => format!("Sección {} de {filename}: {}.", k + 1, sec.title),
        };
        if let Some(line) = first {
            description.push(' ');
            description.push_str(&truncate_chars(&line, 160));
        }
        let draft = range.iter().any(|&i| {
            matches!(units[i].kind, Kind::PageNote(p) if matches!(ex.pages[p - 1].class, Class::Scanned | Class::ScannedOcr))
        });
        let mut out = format!(
            "---\ntype: {}\ntitle: {}\ndescription: {}\nsources:\n  - id: original\n    resource: /references/original.pdf\ngenerated:\n  by: process:document-conversion\n  at: {}\nsource_spans:\n  - {{ id: seccion, source_id: original, kind: pdf_pages, from: {pages_from}, to: {pages_to} }}\n",
            if outline.split.is_some() {
                "Source Section"
            } else {
                "Source Text"
            },
            yaml(&sec.title),
            yaml(&description),
            yaml(generated),
        );
        for (id, a, b) in &spans {
            out.push_str(&format!(
                "  - {{ id: {id}, source_id: original, kind: pdf_pages, from: {a}, to: {b} }}\n"
            ));
        }
        if sec.inferred {
            out.push_str("title_inferred: true\n");
        }
        if draft {
            out.push_str("status: draft\n");
        }
        out.push_str("---\n\n");
        out.push_str(&format!("# {}\n\n", escape_inline(&sec.title)));
        if sec.inferred {
            out.push_str("> Encabezado inferido por su formato tipográfico: el PDF no lo marca como encabezado.\n\n");
        }
        if body.is_empty() {
            out.push_str("Sin texto extraído.\n");
        } else {
            out.push_str(&body);
        }
        out
    }
}

/// Páginas de un tramo de unidades, para el informe.
fn pages_phrase(ns: &[usize]) -> String {
    let mut v = ns.to_vec();
    v.sort_unstable();
    v.dedup();
    units_phrase("Página", "Páginas", &v)
}

pub(crate) fn build_bundle(
    bytes: &[u8],
    filename: &str,
    hash: &str,
    max_pages: usize,
    attempt: &FsPath,
) -> Result<(PathBuf, String)> {
    let started = Instant::now();
    let generated = chrono_now();
    let zip_path = attempt.join("bundle.zip");
    let mut z = ZipWriter::new(fs::File::create(&zip_path)?);
    let mut ex = extract(bytes, max_pages, &mut z)?;
    let outline = outline(&mut ex, filename);
    let n = ex.pages.len();
    let mut r = Render {
        ex: &ex,
        outline: &outline,
        inferred: Vec::new(),
    };
    let mut entries = String::new();
    let mut content_entries = String::new();
    for k in 0..outline.sections.len() {
        let md = r.concept(k, filename, &generated);
        let file = outline.file(k);
        write_zip_file(&mut z, &format!("contenido/{file}"), md.as_bytes())?;
        let sec = &outline.sections[k];
        let mut entry = format!("[{}]", escape_inline(&sec.title));
        if sec.preamble {
            entry.push_str(" — inicio del documento");
        }
        let link = |href: &str| {
            let (label, rest) = entry.split_at(entry.find(']').map_or(0, |i| i + 1));
            format!("- {label}({href}){rest}\n")
        };
        entries.push_str(&link(&format!("contenido/{file}")));
        content_entries.push_str(&link(&file));
    }
    let inferred = std::mem::take(&mut r.inferred);
    // Estructura de encabezados para documento.md.
    let mut structure = String::new();
    if outline.split.is_none() {
        structure.push_str("- [Texto completo](contenido/texto.md)\n");
    }
    let min_level = outline
        .headings
        .iter()
        .flatten()
        .map(|h| h.level)
        .min()
        .unwrap_or(1);
    for (k, sec) in outline.sections.iter().enumerate() {
        if sec.preamble {
            structure.push_str(&format!("- [Inicio](contenido/{})\n", outline.file(k)));
        }
        for i in sec.from..=sec.to.min(ex.units.len().saturating_sub(1)) {
            let Some(h) = outline.headings[i] else {
                continue;
            };
            if Some(i) == outline.title_unit {
                continue;
            }
            let text =
                escape_line_start(&escape_inline(&para_text(&ex.units[i]).unwrap_or_default()));
            let page = ex.units[i].from;
            let suffix = if h.inferred {
                format!(" · página {page} (inferido por formato)")
            } else {
                format!(" · página {page}")
            };
            match outline.split {
                Some(l) if Some(i) == sec.heading => structure.push_str(&format!(
                    "{}- [{text}](contenido/{}){suffix}\n",
                    "  ".repeat(h.level.saturating_sub(l) as usize),
                    outline.file(k)
                )),
                Some(l) => structure.push_str(&format!(
                    "{}- {text}{suffix}\n",
                    "  ".repeat(h.level.saturating_sub(l).max(1) as usize)
                )),
                None => structure.push_str(&format!(
                    "{}- {text}{suffix}\n",
                    "  ".repeat((h.level - min_level + 1) as usize)
                )),
            }
        }
    }
    // Encabezados y pies de página e imágenes de plantilla.
    let mut furniture_md = String::new();
    if !ex.furniture.is_empty() || !ex.furniture_images.is_empty() {
        furniture_md.push_str("\n## Encabezados y pies de página\n\nTexto e imágenes que se repiten en las páginas; no forman parte del flujo del documento.\n\n");
        for f in &ex.furniture {
            furniture_md.push_str(&format!(
                "- {}: {}\n",
                pages_phrase(&f.pages),
                escape_line_start(&escape_inline(&f.text))
            ));
        }
        for (path, pages) in &ex.furniture_images {
            furniture_md.push_str(&format!(
                "- {}: ![Imagen repetida]({path})\n",
                pages_phrase(pages)
            ));
        }
    }
    let labels: Vec<(usize, &String)> = ex
        .pages
        .iter()
        .enumerate()
        .filter_map(|(p, x)| x.label.as_ref().map(|l| (p + 1, l)))
        .collect();
    let labels_yaml = if labels.is_empty() {
        String::new()
    } else {
        let mut s = "page_labels:\n".to_string();
        for (p, l) in &labels {
            s.push_str(&format!("  \"{p}\": {}\n", yaml(l)));
        }
        s
    };
    let mut doc_extra = String::new();
    if !labels.is_empty() {
        doc_extra.push_str(&format!(
            "\n## Números de página impresos\n\n{} con número visible; `page_labels` relaciona cada página física con su número impreso.\n",
            pages_phrase(&labels.iter().map(|(p, _)| *p).collect::<Vec<_>>())
        ));
    }
    if !ex.internal_labels.is_empty() {
        doc_extra.push_str("\n## Etiquetas internas del PDF\n\nEtiquetas declaradas en `/PageLabels`; no son necesariamente las impresas.\n\n");
        let shown: Vec<String> = ex
            .internal_labels
            .iter()
            .enumerate()
            .take(200)
            .map(|(p, l)| format!("{} → «{}»", p + 1, escape_inline(l)))
            .collect();
        doc_extra.push_str(&shown.join(", "));
        if ex.internal_labels.len() > 200 {
            doc_extra.push_str(&format!(" y {} más", ex.internal_labels.len() - 200));
        }
        doc_extra.push('\n');
    }
    if !ex.bookmarks.is_empty() {
        doc_extra.push_str("\n## Marcadores del PDF\n\n");
        doc_extra.push_str(if outline.from_bookmarks {
            "Esquema de navegación del original; sus entradas determinan los encabezados.\n\n"
        } else {
            "Esquema de navegación del original; no coincide con el texto de forma suficiente para determinar los encabezados.\n\n"
        });
        for b in ex.bookmarks.iter().take(200) {
            let page = b
                .page
                .map(|p| format!(" · página {}", p + 1))
                .unwrap_or_default();
            doc_extra.push_str(&format!(
                "{}- {}{page}\n",
                "  ".repeat(b.depth.min(5) as usize),
                escape_line_start(&escape_inline(&truncate_chars(&b.title, 160)))
            ));
        }
        if ex.bookmarks.len() > 200 {
            doc_extra.push_str(&format!(
                "- … y {} marcadores más\n",
                ex.bookmarks.len() - 200
            ));
        }
    }
    let mut meta = String::new();
    if let Some(a) = &ex.info.author {
        meta.push_str(&format!("- Autor: {}\n", escape_inline(a)));
    }
    if let Some(c) = &ex.info.creator {
        meta.push_str(&format!("- Aplicación de origen: {}\n", escape_inline(c)));
    }
    if let Some(p) = &ex.info.producer {
        meta.push_str(&format!("- Productor del PDF: {}\n", escape_inline(p)));
    }
    if let Some(t) = &ex.info.title
        && outline.title_source != TitleSource::Metadata
    {
        meta.push_str(&format!(
            "- Título en las propiedades: {}\n",
            escape_inline(t)
        ));
    }
    write_zip_file(&mut z, "references/original.pdf", bytes)?;
    // Informe.
    let t = &ex.tally;
    let by_class = |c: Class| -> Vec<usize> {
        ex.pages
            .iter()
            .enumerate()
            .filter(|(_, x)| x.class == c)
            .map(|(p, _)| p + 1)
            .collect()
    };
    let mut issues = Vec::new();
    let scanned = by_class(Class::Scanned);
    if !scanned.is_empty() {
        issues.push(Issue::loss(format!(
            "{}: escaneada sin capa de texto; se adjunta su vista y no se aplicó OCR",
            pages_phrase(&scanned)
        )));
    }
    if !t.views_failed.is_empty() {
        issues.push(Issue::loss(format!(
            "{}: no se pudo generar la vista de una página sin texto transcribible",
            pages_phrase(&t.views_failed)
        )));
    }
    if !t.broken.is_empty() {
        issues.push(Issue::loss(format!(
            "{}: imagen que no se pudo decodificar ni adjuntar",
            pages_phrase(&t.broken)
        )));
    }
    if !t.unreadable.is_empty() {
        issues.push(Issue::loss(format!(
            "{}: caracteres sin correspondencia Unicode (fuente sin tabla ToUnicode), escritos como «\u{FFFD}»",
            pages_phrase(&t.unreadable)
        )));
    }
    let ocr = by_class(Class::ScannedOcr);
    if !ocr.is_empty() {
        issues.push(Issue::note(format!(
            "{}: escaneo con una capa de texto de OCR previo; se conserva ese texto sin verificar y se adjunta la vista",
            pages_phrase(&ocr)
        )));
    }
    let visual = by_class(Class::Visual);
    if !visual.is_empty() {
        issues.push(Issue::note(format!(
            "{}: sin texto, con contenido gráfico; se adjunta su vista",
            pages_phrase(&visual)
        )));
    }
    let mixed = by_class(Class::Mixed);
    if !mixed.is_empty() {
        issues.push(Issue::note(format!(
            "{}: texto con imágenes grandes; las imágenes se adjuntan sin transcribir (no se aplicó OCR)",
            pages_phrase(&mixed)
        )));
    }
    let blank = by_class(Class::Blank);
    if !blank.is_empty() {
        issues.push(Issue::note(format!(
            "{}: en blanco o solo con encabezado, pie o número de página",
            pages_phrase(&blank)
        )));
    }
    let multi: Vec<usize> = ex
        .pages
        .iter()
        .enumerate()
        .filter(|(_, x)| x.columns)
        .map(|(p, _)| p + 1)
        .collect();
    if !multi.is_empty() {
        issues.push(Issue::note(format!(
            "{}: varias columnas; el orden de lectura se infiere de la disposición (columna por columna, de izquierda a derecha)",
            pages_phrase(&multi)
        )));
    }
    if !t.continued.is_empty() {
        let crossings: Vec<String> = t
            .continued
            .iter()
            .take(30)
            .map(|(a, b)| {
                if a == b {
                    format!("{a}")
                } else {
                    format!("{a}→{b}")
                }
            })
            .collect();
        issues.push(Issue::note(format!(
            "{} párrafo(s) unidos con su continuación en otra columna o página (la frase sigue en minúscula con el mismo formato): páginas {}",
            t.continued.len(),
            crossings.join(", ")
        )));
    }
    if t.dehyphenated > 0 {
        issues.push(Issue::note(format!(
            "{} palabra(s) partidas con guion al final de línea, unidas sin el guion",
            t.dehyphenated
        )));
    }
    if !t.tables.is_empty() {
        issues.push(Issue::note(format!(
            "{}: tabla reconstruida por la alineación de sus celdas; puede no coincidir con las líneas de la tabla original",
            pages_phrase(&t.tables)
        )));
    }
    if !t.rotated.is_empty() {
        issues.push(Issue::note(format!(
            "{}: texto girado, escrito al final del contenido de su página",
            pages_phrase(&t.rotated)
        )));
    }
    if !inferred.is_empty() {
        issues.push(Issue::note(format!(
            "{}: encabezado inferido por su tamaño de letra (mayor que el texto corriente); el nivel sale del tamaño",
            pages_phrase(&inferred)
        )));
    }
    if outline.from_bookmarks {
        issues.push(Issue::note(format!(
            "Encabezados tomados de los marcadores del PDF ({} de {} encontrados en el texto)",
            ex.bookmarks.len() - outline.unmatched_bookmarks,
            ex.bookmarks.len()
        )));
    }
    if !ex.furniture.is_empty() || !ex.furniture_images.is_empty() {
        issues.push(Issue::note(format!(
            "Encabezados y pies de página registrados una vez en documento.md: {} texto(s) y {} imagen(es) repetidas",
            ex.furniture.len(),
            ex.furniture_images.len()
        )));
    }
    if !labels.is_empty() {
        issues.push(Issue::note(format!(
            "Números de página impresos detectados en {} página(s) y registrados en `page_labels`",
            labels.len()
        )));
    }
    if !ex.internal_labels.is_empty() {
        issues.push(Issue::note(
            "El PDF declara etiquetas internas (/PageLabels), listadas en documento.md; no sustituyen a los números impresos".into(),
        ));
    }
    if t.decorative > 0 {
        issues.push(Issue::note(format!(
            "{} imagen(es) de menos del 1 % de la página consideradas decorativas; no se adjuntan",
            t.decorative
        )));
    }
    issues.push(Issue::note(format!(
        "Título del documento tomado {}: «{}»",
        match outline.title_source {
            TitleSource::Heading =>
                "del primer encabezado, seguido directamente por otro de igual o mayor rango",
            TitleSource::LoneHeading =>
                "del primer encabezado de la primera página, único de su nivel y de rango superior a los demás",
            TitleSource::Metadata => "de las propiedades del documento",
            TitleSource::Filename => "del nombre del archivo; el documento no lo declara",
        },
        truncate_chars(&outline.title, 120)
    )));
    let (losses, notes): (Vec<&Issue>, Vec<&Issue>) =
        issues.iter().partition(|i| i.severity == Severity::Loss);
    let state = if losses.is_empty() {
        "complete"
    } else {
        "partial"
    };
    let list = |items: &[&Issue], empty: &str| -> String {
        if items.is_empty() {
            format!("{empty}\n")
        } else {
            items
                .iter()
                .map(|i| format!("- {}\n", escape_inline(&i.text)))
                .collect()
        }
    };
    // Cobertura: tramos consecutivos de páginas con la misma clase y el mismo concepto.
    let mut concept_of = vec![String::new(); n];
    for (k, sec) in outline.sections.iter().enumerate() {
        for i in sec.from..=sec.to.min(ex.units.len().saturating_sub(1)) {
            let u = &ex.units[i];
            for p in u.from..=u.to {
                let file = outline.file(k);
                let c = &mut concept_of[p - 1];
                if !c.split(", ").any(|x| x == file) {
                    if !c.is_empty() {
                        c.push_str(", ");
                    }
                    c.push_str(&file);
                }
            }
        }
    }
    if let Some(t) = outline.title_unit {
        let c = &mut concept_of[ex.units[t].from - 1];
        if c.is_empty() {
            c.push_str("documento.md");
        }
    }
    let mut coverage = String::from("| Páginas | Clase | Conceptos |\n| --- | --- | --- |\n");
    let mut p = 0;
    while p < n {
        let mut q = p;
        while q + 1 < n
            && ex.pages[q + 1].class == ex.pages[p].class
            && concept_of[q + 1] == concept_of[p]
        {
            q += 1;
        }
        let range = if p == q {
            format!("{}", p + 1)
        } else {
            format!("{}–{}", p + 1, q + 1)
        };
        coverage.push_str(&format!(
            "| {range} | {} | {} |\n",
            ex.pages[p].class.label(),
            concept_of[p]
        ));
        p = q + 1;
    }
    let count = |c: Class| by_class(c).len();
    let division = match outline.split {
        Some(l) => format!(
            "{} concepto(s), divididos por los encabezados de nivel {l}",
            outline.sections.len()
        ),
        None => "1 concepto (documento breve o sin secciones)".into(),
    };
    let headings = outline.headings.iter().flatten().count();
    let report = format!(
        "---\ntype: Conversion Report\ntitle: Informe de conversión\ngenerated:\n  by: process:document-conversion\n  at: {}\nstatus: {state}\nformat: pdf\nsha256: {hash}\n---\n\n# Informe de conversión PDF\n\n- Estado: `{state}`\n- Páginas: {n} (texto digital: {}; mixtas: {}; escaneadas: {}; escaneadas con OCR previo: {}; solo visuales: {}; en blanco: {})\n- Unidades representadas: {} (párrafos, listas, tablas, imágenes y notas de página)\n- Conceptos: {division}\n- Encabezados: {headings} ({})\n- Tablas reconstruidas: {}; imágenes adjuntas: {}; vistas de página: {}\n- Duración: {} ms\n\n## Cobertura de páginas\n\nToda página física figura en algún concepto; las páginas sin texto propio llevan una nota (y su vista, si tienen contenido gráfico).\n\n{coverage}\n## Incidencias\n\nPérdidas o incertidumbres sustantivas; determinan el estado `partial`.\n\n{}\n## Observaciones\n\nInformación sobre la conversión que no afecta el estado.\n\n{}",
        yaml(&generated),
        count(Class::Digital),
        count(Class::Mixed),
        count(Class::Scanned),
        count(Class::ScannedOcr),
        count(Class::Visual),
        count(Class::Blank),
        ex.units.len(),
        if outline.from_bookmarks {
            "de los marcadores del PDF".to_string()
        } else {
            format!(
                "inferidos por tamaño de letra: {}",
                outline
                    .headings
                    .iter()
                    .flatten()
                    .filter(|h| h.inferred)
                    .count()
            )
        },
        t.tables.len(),
        t.images,
        ex.pages.iter().filter(|x| x.view.is_some()).count(),
        started.elapsed().as_millis(),
        list(&losses, "Sin pérdidas ni incertidumbres sustantivas."),
        list(&notes, "Sin observaciones.")
    );
    let title_span = outline
        .title_unit
        .map(|t| {
            format!(
                "  - {{ id: titulo, source_id: original, kind: pdf_pages, from: {}, to: {} }}\n",
                ex.units[t].from, ex.units[t].to
            )
        })
        .unwrap_or_default();
    let title_marker = if outline.title_unit.is_some() {
        "<!-- okf-source-span: titulo -->\n"
    } else {
        ""
    };
    let summary = match outline.split {
        Some(l) => format!(
            "Documento de {n} páginas, dividido en {} conceptos por sus encabezados de nivel {l}.",
            outline.sections.len()
        ),
        None => format!("Documento de {n} páginas, representado en un solo concepto."),
    };
    let meta_md = if meta.is_empty() {
        String::new()
    } else {
        format!("\n## Propiedades\n\n{meta}")
    };
    let doc = format!(
        "---\ntype: Source Document\ntitle: {}\ndescription: {}\nsources:\n  - id: original\n    resource: /references/original.pdf\ngenerated:\n  by: process:document-conversion\n  at: {}\nsource_file:\n  path: references/original.pdf\n  sha256: {hash}\nsource_spans:\n  - {{ id: documento, source_id: original, kind: pdf_pages, from: 1, to: {n} }}\n{title_span}{labels_yaml}---\n\n{title_marker}# {}\n\n{summary}\n\n## Estructura\n\n{structure}{meta_md}{doc_extra}{furniture_md}",
        yaml(&outline.title),
        yaml(&format!(
            "Documento fuente {filename} convertido a conceptos OKF por secciones."
        )),
        yaml(&generated),
        escape_inline(&outline.title),
    );
    let index = format!(
        "---\nokf_version: \"0.2\"\n---\n\n# Bundle OKF\n\n- [Documento fuente](documento.md)\n- [Índice de contenido](contenido/index.md)\n- [Informe de conversión](conversion-report.md)\n\n## Secciones\n\n{entries}"
    );
    let contents_index = format!("# Contenido\n\n## Secciones\n\n{content_entries}");
    write_zip_file(&mut z, "index.md", index.as_bytes())?;
    write_zip_file(&mut z, "contenido/index.md", contents_index.as_bytes())?;
    write_zip_file(&mut z, "documento.md", doc.as_bytes())?;
    write_zip_file(&mut z, "conversion-report.md", report.as_bytes())?;
    z.finish()?;
    validate_bundle(&zip_path)?;
    Ok((zip_path, state.into()))
}

/// Extrae y ensambla el bundle de un PDF.
pub(crate) fn convert(
    cfg: &Config,
    bytes: &[u8],
    filename: &str,
    hash: &str,
    dir: &FsPath,
) -> Result<(PathBuf, String)> {
    build_bundle(bytes, filename, hash, cfg.max_pages, dir)
}

/// Destinos de los enlaces Markdown que empiezan por `prefix`, en orden.
fn listed(md: &str, prefix: &str) -> Vec<String> {
    md.match_indices(prefix)
        .filter_map(|(i, _)| {
            let rest = &md[i + prefix.len()..];
            rest.find(')').map(|end| rest[..end].to_string())
        })
        .collect()
}

/// Comprobaciones PDF del bundle: los conceptos de contenido siguen el orden de las
/// páginas y las cubren todas, cada localizador cae dentro de su concepto, todo bloque
/// de un concepto de varias páginas lleva su marcador y los índices enumeran los
/// conceptos en orden.
pub(crate) fn validate(
    z: &mut ZipArchive<fs::File>,
    names: &HashSet<String>,
    original: &[u8],
) -> Result<()> {
    let doc = read::open(original)?;
    let total = read::page_count(&doc)? as u64;
    let mut concepts: Vec<&String> = names
        .iter()
        .filter(|n| n.starts_with("contenido/") && n.ends_with(".md") && *n != "contenido/index.md")
        .collect();
    concepts.sort();
    let expected: Vec<String> = if concepts.len() == 1 && concepts[0] == "contenido/texto.md" {
        vec!["contenido/texto.md".into()]
    } else {
        (1..=concepts.len())
            .map(|k| format!("contenido/seccion-{k:03}.md"))
            .collect()
    };
    if concepts.is_empty() || concepts.iter().zip(&expected).any(|(a, b)| *a != b) {
        bail!("los conceptos de contenido no siguen la numeración esperada")
    }
    let span_range = |s: &YamlValue| -> Option<(u64, u64)> {
        (s.get("kind").and_then(YamlValue::as_str) == Some("pdf_pages")).then_some(())?;
        let a = s.get("from").and_then(YamlValue::as_u64)?;
        let b = s.get("to").and_then(YamlValue::as_u64)?;
        (a >= 1 && a <= b && b <= total).then_some((a, b))
    };
    let mut covered = vec![false; total as usize + 1];
    let mut prev: Option<(u64, u64)> = None;
    for name in &expected {
        let md = read_text(z, name)?;
        let fm = frontmatter(&md)?;
        let spans = fm
            .get("source_spans")
            .and_then(YamlValue::as_sequence)
            .with_context(|| format!("{name} carece de source_spans"))?;
        let section = spans
            .iter()
            .find(|s| s.get("id").and_then(YamlValue::as_str) == Some("seccion"))
            .and_then(span_range)
            .with_context(|| {
                format!("{name} carece de un localizador pdf_pages «seccion» válido")
            })?;
        if let Some(p) = prev
            && (section.0 < p.0 || section.0 > p.1 + 1)
        {
            bail!(
                "{name} empieza en la página {} y el concepto anterior cubre {}–{}",
                section.0,
                p.0,
                p.1
            )
        }
        prev = Some(section);
        let mut ids: HashMap<String, (u64, u64)> = HashMap::new();
        for s in spans {
            match span_range(s) {
                Some((a, b)) if a >= section.0 && b <= section.1 => {
                    if let Some(id) = s.get("id").and_then(YamlValue::as_str) {
                        ids.insert(id.to_string(), (a, b));
                    }
                }
                _ => bail!("{name} tiene un localizador de páginas fuera de su sección"),
            }
        }
        let body = md.split_once("\n---\n").map_or("", |x| x.1);
        // La primera página del concepto tiene su título o su primer bloque.
        covered[section.0 as usize] = true;
        if section.0 == section.1 {
            continue;
        }
        // Cada bloque del cuerpo, tras el título y su nota, lleva un marcador.
        let mut used = HashSet::new();
        let mut started = false;
        for chunk in body.split("\n\n").map(str::trim).filter(|c| !c.is_empty()) {
            if !started
                && (chunk.starts_with("# ")
                    || chunk.starts_with("> ")
                    || chunk == "Sin texto extraído.")
            {
                continue;
            }
            started = true;
            let Some(rest) = chunk.strip_prefix("<!-- okf-source-span: ") else {
                bail!(
                    "{name} tiene un bloque sin marcador de página: «{}»",
                    truncate_chars(chunk, 60)
                )
            };
            let id = rest.split(" -->").next().unwrap_or_default();
            let Some(&(a, b)) = ids.get(id) else {
                bail!("{name} usa un marcador sin localizador: {id}")
            };
            used.insert(id.to_string());
            for p in a..=b {
                covered[p as usize] = true;
            }
        }
        if let Some(unused) = ids
            .keys()
            .find(|id| *id != "seccion" && !used.contains(*id))
        {
            bail!("{name} declara el localizador {unused} y ningún bloque lo usa")
        }
    }
    let doc_md = read_text(z, "documento.md")?;
    let docfm = frontmatter(&doc_md)?;
    let doc_spans = docfm
        .get("source_spans")
        .and_then(YamlValue::as_sequence)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let whole = doc_spans
        .iter()
        .find(|s| s.get("id").and_then(YamlValue::as_str) == Some("documento"))
        .and_then(span_range);
    if whole != Some((1, total)) {
        bail!("documento.md no localiza las {total} páginas del original")
    }
    if let Some((a, b)) = doc_spans
        .iter()
        .find(|s| s.get("id").and_then(YamlValue::as_str) == Some("titulo"))
        .and_then(span_range)
    {
        for p in a..=b {
            covered[p as usize] = true;
        }
    }
    if let Some(p) = (1..=total).find(|&p| !covered[p as usize]) {
        bail!("la página {p} no figura en ningún concepto")
    }
    if let Some(labels) = docfm.get("page_labels") {
        let map = labels
            .as_mapping()
            .context("page_labels de documento.md no es un mapa")?;
        for (k, v) in map {
            let page: u64 = k
                .as_str()
                .and_then(|s| s.parse().ok())
                .context("page_labels tiene una clave que no es una página física")?;
            if page == 0 || page > total || v.as_str().is_none_or(|s| s.trim().is_empty()) {
                bail!("page_labels tiene una entrada inválida para la página {page}")
            }
        }
    }
    let files: Vec<String> = expected
        .iter()
        .map(|e| e.trim_start_matches("contenido/").to_string())
        .collect();
    let root = listed(&read_text(z, "index.md")?, "](contenido/");
    let root: Vec<String> = root.into_iter().filter(|f| f != "index.md").collect();
    let content = listed(&read_text(z, "contenido/index.md")?, "](");
    if root != files || content != files {
        bail!("un índice no enumera los conceptos en el orden del documento")
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::io::Read;

    /// Convierte un PDF y devuelve el estado, el contenido de cada entrada Markdown y
    /// los nombres de todas las entradas del bundle.
    fn convert_example(path: &str) -> (String, HashMap<String, String>, HashSet<String>) {
        let bytes = fs::read(path).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let hash = hex(&Sha256::digest(&bytes));
        let name = path.rsplit('/').next().unwrap();
        let (zip, state) = build_bundle(&bytes, name, &hash, 2000, tmp.path()).unwrap();
        let mut z = ZipArchive::new(fs::File::open(zip).unwrap()).unwrap();
        let mut md = HashMap::new();
        let mut names = HashSet::new();
        for i in 0..z.len() {
            let mut e = z.by_index(i).unwrap();
            names.insert(e.name().to_string());
            if e.name().ends_with(".md") {
                let mut s = String::new();
                e.read_to_string(&mut s).unwrap();
                md.insert(e.name().to_string(), s);
            }
        }
        (state, md, names)
    }

    fn sample(name: &str) -> (String, HashMap<String, String>, HashSet<String>) {
        convert_example(&format!("examples/pdf/muestras/{name}.pdf"))
    }

    fn corpus(name: &str) -> (String, HashMap<String, String>, HashSet<String>) {
        let dir =
            std::env::var("OKF_PDF_CORPUS_DIR").unwrap_or_else(|_| "examples/pdf/corpus".into());
        convert_example(&format!("{dir}/{name}.pdf"))
    }

    fn titles(md: &HashMap<String, String>) -> Vec<String> {
        let mut names: Vec<&String> = md
            .keys()
            .filter(|k| k.starts_with("contenido/") && *k != "contenido/index.md")
            .collect();
        names.sort();
        names
            .iter()
            .map(|n| {
                frontmatter(&md[*n])
                    .unwrap()
                    .get("title")
                    .and_then(YamlValue::as_str)
                    .unwrap()
                    .to_string()
            })
            .collect()
    }

    fn labels(md: &HashMap<String, String>) -> Vec<(String, String)> {
        let fm = frontmatter(&md["documento.md"]).unwrap();
        let mut out: Vec<(String, String)> = fm
            .get("page_labels")
            .and_then(YamlValue::as_mapping)
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.as_str().unwrap().into(), v.as_str().unwrap().into()))
                    .collect()
            })
            .unwrap_or_default();
        out.sort_by_key(|(k, _)| k.parse::<u32>().unwrap());
        out
    }

    #[test]
    fn brief_pdf_is_a_single_text_concept() {
        let (state, md, _) = sample("breve");
        assert_eq!(state, "complete");
        assert_eq!(titles(&md), ["Préstamo de equipos audiovisuales"]);
        let text = &md["contenido/texto.md"];
        assert!(text.contains("type: Source Text"));
        assert!(
            text.contains("- Las reservas se hacen con dos días de anticipación.\n- Los daños")
        );
        // Un concepto de una sola página no lleva marcadores de pasaje.
        assert!(!text.contains("okf-source-span"));
        assert!(
            md["documento.md"]
                .contains("<!-- okf-source-span: titulo -->\n# Préstamo de equipos audiovisuales")
        );
        assert!(labels(&md).is_empty());
        assert!(md["conversion-report.md"].contains("Páginas: 1 (texto digital: 1;"));
    }

    #[test]
    fn two_column_article_keeps_reading_order_page_labels_and_continuations() {
        let (state, md, names) = sample("articulo_columnas");
        assert_eq!(state, "complete");
        assert_eq!(
            titles(&md),
            [
                "Lectura de documentos con varias columnas: un estudio con materiales artificiales",
                "Resumen",
                "1. Introducción",
                "2. Método",
                "3. Resultados",
                "4. Conclusiones",
                "Referencias"
            ]
        );
        // La portada no tiene número: las etiquetas impresas empiezan en la página 2.
        assert_eq!(
            labels(&md),
            [
                ("2".into(), "1".into()),
                ("3".into(), "2".into()),
                ("4".into(), "3".into())
            ]
        );
        let intro = &md["contenido/seccion-003.md"];
        assert!(
            intro.contains(
                "de modo que el texto conserve su sentido. El análisis de costo-beneficio"
            )
        );
        let method = &md["contenido/seccion-004.md"];
        assert!(
            method.contains("<!-- okf-source-span: p-1-2 -->\nSe prepararon veinte documentos")
        );
        assert!(method.contains("anotando cualquier diferencia en el orden"));
        assert!(method.contains("La conversión se consideró correcta"));
        assert!(method.contains("estimar el costo-beneficio de cada procedimiento"));
        let results = &md["contenido/seccion-005.md"];
        assert!(results.contains(
            "| Procedimiento | Correctos | Tiempo |\n| --- | --- | --- |\n| Primero | 17 | 1,2 s |"
        ));
        // Un concepto de una sola página no lleva marcadores.
        assert!(results.contains(
            "![Imagen de la página 2](../assets/pagina-002-imagen-01.png)\n\n*Figura 1."
        ));
        assert!(names.contains("assets/pagina-002-imagen-01.png"));
        assert!(
            !md.iter()
                .any(|(n, t)| n.starts_with("contenido/") && t.contains("Revista de Pruebas"))
        );
        assert!(
            md["documento.md"]
                .contains("- Páginas 2–4: Revista de Pruebas Documentales · Vol. 3 · 2026")
        );
        let report = &md["conversion-report.md"];
        assert!(report.contains(
            "Encabezados tomados de los marcadores del PDF (7 de 7 encontrados en el texto)"
        ));
        assert!(report.contains("Páginas 2 y 3: varias columnas"));
        assert!(report.contains("Página 2: tabla reconstruida"));
    }

    #[test]
    fn report_without_bookmarks_infers_headings_lists_code_and_page_numbers() {
        let (state, md, _) = sample("informe_tecnico");
        assert_eq!(state, "complete");
        assert_eq!(
            titles(&md),
            [
                "Informe técnico de migración",
                "1. Contexto",
                "2. Arquitectura",
                "3. Plan de migración",
                "4. Riesgos"
            ]
        );
        assert!(md["contenido/seccion-002.md"].contains("title_inferred: true"));
        assert!(md["contenido/seccion-002.md"].contains("La migración es **importante** porque el contrato de soporte vence en marzo y *no se renovará*."));
        let arch = &md["contenido/seccion-003.md"];
        assert!(arch.contains("1. El servicio de catálogo guarda la descripción de cada producto, sus códigos y sus proveedores, y publica un mensaje cada vez que cambia un producto.\n2. El servicio"));
        assert!(arch.contains("```\nservicio: existencias\nreplicas: 2\nbase_de_datos:\n  motor: postgres\n  version: 16\n```"));
        let plan = &md["contenido/seccion-004.md"];
        assert!(plan.contains("## 3.1 Fase de preparación"));
        assert!(plan.contains(
            "- Inventario de datos y limpieza de registros duplicados.\n- Pruebas de carga"
        ));
        assert_eq!(
            labels(&md),
            [("1".into(), "1".into()), ("2".into(), "2".into())]
        );
        assert!(md["documento.md"].contains("- Páginas 1 y 2: Informe técnico — Página N de 2"));
        assert!(
            md["documento.md"]
                .contains("- 3\\.1 Fase de preparación · página 2 (inferido por formato)")
        );
        assert!(md["conversion-report.md"].contains("párrafo(s) unidos con su continuación en otra columna o página (la frase sigue en minúscula con el mismo formato): páginas 1→2"));
    }

    #[test]
    fn page_classes_scanned_blank_visual_and_mixed_are_declared() {
        let (state, md, names) = sample("escaneado_mixto");
        assert_eq!(state, "partial");
        let text = &md["contenido/texto.md"];
        assert!(text.contains("status: draft"));
        assert!(text.contains("<!-- okf-source-span: p-2 -->\n![Vista de la página 2](../assets/pagina-002.png)\n> Página escaneada sin capa de texto"));
        assert!(text.contains("<!-- okf-source-span: p-3 -->\n_Página 3 sin contenido propio"));
        assert!(text.contains("<!-- okf-source-span: p-4 -->\n![Vista de la página 4](../assets/pagina-004.png)\n> Página sin texto con contenido gráfico"));
        assert!(text.contains("![Imagen de la página 5](../assets/pagina-005-imagen-01.png)"));
        assert!(text.contains("_Texto girado:_ Copia controlada · no imprimir"));
        for asset in [
            "assets/pagina-002.png",
            "assets/pagina-004.png",
            "assets/pagina-005-imagen-01.png",
        ] {
            assert!(names.contains(asset), "{asset}");
        }
        assert_eq!(labels(&md).len(), 4);
        assert!(md["documento.md"].contains("1 → «Exp-1», 2 → «Exp-2»"));
        let report = &md["conversion-report.md"];
        assert!(report.contains(
            "- Página 2: escaneada sin capa de texto; se adjunta su vista y no se aplicó OCR"
        ));
        assert!(report.contains("| 3 | en blanco | texto.md |"));
        assert!(report.contains("| 4 | solo visual | texto.md |"));
        assert!(report.contains("| 5 | mixta (texto e imagen) | texto.md |"));
    }

    #[test]
    fn corpus_p2_two_column_article_keeps_citations_compounds_and_references() {
        let (state, md, _) = corpus("p2");
        assert_eq!(state, "complete");
        assert_eq!(titles(&md).len(), 9);
        assert_eq!(titles(&md)[2], "Introduction");
        let all: String = md
            .iter()
            .filter(|(n, _)| n.starts_with("contenido/"))
            .map(|(_, t)| t.as_str())
            .collect();
        assert!(all.contains("JAMA Network Journals \\[7\\], and the World Association of Medical Editors \\[11\\] have all issued"));
        assert!(all.contains(
            "text pulled from human-generated content, designed to build coherent responses"
        ));
        assert!(
            all.contains(
                "Unlike these previous tools, however, generative AI tools have introduced"
            )
        );
        assert!(all.contains("12. McNutt MK, Bradford M"));
        assert!(all.contains("10. Editors ICoMJ. Recommendations for the conduct, reporting, editing, and publication of scholarly work in medical journals 2025. Available from: https://www.icmje.org/icmje-recommendations.pdf."));
        assert!(!all.contains("Chat-GPT"));
        assert_eq!(labels(&md).len(), 8);
    }

    #[test]
    fn corpus_p4_word_export_uses_bookmarks_and_keeps_every_word() {
        let (state, md, _) = corpus("p4");
        assert_eq!(state, "complete");
        // El primer concepto (la imagen bajo el título) lleva el título del documento.
        assert_eq!(
            titles(&md),
            [
                "¿Qué es un agente de inteligencia artificial?",
                "¿Qué es un agente? La pregunta que importa",
                "Cinco atributos que definen a un agente",
                "Lo que un agente NO es",
                "La arquitectura mínima de un sistema agéntico",
                "El doble paradigma",
                "El principio de agencia mínima",
                "Referencias"
            ]
        );
        let all: String = md.values().map(String::as_str).collect();
        assert!(all.contains("El harness establece las transiciones permitidas"));
        assert!(all.contains("en los gateways de API"));
        assert!(
            md["conversion-report.md"].contains(
                "primer encabezado, seguido directamente por otro de igual o mayor rango"
            )
        );
    }

    #[test]
    fn corpus_p1_slides_split_by_workshop_and_record_template_images() {
        let (state, md, _) = corpus("p1");
        assert_eq!(state, "complete");
        assert_eq!(
            titles(&md),
            vec!["COMUNICACIÓN EFECTIVA EN ENTORNOS LABORALES"; 3]
        );
        assert_eq!(labels(&md).len(), 52);
        let report = &md["conversion-report.md"];
        assert!(report.contains("| 40 | en blanco |"));
        assert!(report.contains("imagen(es) repetidas"));
        // Las duraciones de cada diapositiva no son encabezados de página.
        assert!(md["contenido/seccion-001.md"].contains("7 min"));
    }

    #[test]
    fn printed_page_numbers_and_furniture_keys() {
        assert_eq!(printed_number("12"), Some(("12".into(), None)));
        assert_eq!(printed_number("xii"), Some(("xii".into(), None)));
        assert_eq!(printed_number("Page 4 of 9"), Some(("4".into(), Some(9))));
        assert_eq!(
            printed_number("Informe técnico — Página 2 de 5"),
            Some(("2".into(), Some(5)))
        );
        assert_eq!(printed_number("4 / 9"), Some(("4".into(), Some(9))));
        assert_eq!(printed_number("5 min"), None);
        assert_eq!(furniture_key("Página 3 de 3", 3), "página # de 3");
        assert_eq!(furniture_key("Vol. 3 · 2026", 4), "vol. 3 · 2026");
        assert_eq!(furniture_text("Página 2 de 2", 2), "Página N de 2");
    }
}
