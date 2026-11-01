//! Adaptador PPTX: un concepto `Slide` por diapositiva, con su vista renderizada.
pub(crate) mod slide_xml;

use crate::bundle::*;
use crate::format::Format;
use crate::ooxml::*;
use crate::output::*;
use crate::worker::{Config, Rejected};
use anyhow::{Context, Result, bail};
use betteroffice_pptx::{Presentation, RenderOptions};
use std::{
    collections::HashMap,
    fs,
    io::Cursor,
    path::{Path as FsPath, PathBuf},
    time::Instant,
};
use yaml_serde::Value as YamlValue;
use zip::ZipArchive;

/// Elemento del flujo de una diapositiva, en orden del árbol de formas.
#[derive(Clone)]
pub(crate) enum Item {
    Text(Vec<slide_xml::Paragraph>),
    Table(String),
    Image { path: String, alt: Option<String> },
    Chart(String),
    AltText(String),
}

#[derive(Clone, Default)]
pub(crate) struct Slide {
    pub(crate) idx: usize,
    pub(crate) slide_id: Option<u32>,
    pub(crate) title: Option<String>,
    pub(crate) title_inferred: bool,
    pub(crate) kicker: Option<String>,
    pub(crate) section: Option<String>,
    pub(crate) items: Vec<Item>,
    pub(crate) notes: Vec<String>,
    pub(crate) image_paths: Vec<String>,
    pub(crate) decorative_images: usize,
    pub(crate) chart_paths: Vec<String>,
    pub(crate) visible_label: Option<String>,
    pub(crate) hidden: bool,
    /// El contenido sustantivo está solo en una imagen, sin transcripción.
    pub(crate) visual_only: bool,
    /// La vista estática muestra una imagen sobre texto que una animación presenta en secuencia.
    pub(crate) view_hides_text: bool,
    pub(crate) comments: Vec<Comment>,
}

/// Números de las diapositivas afectadas por cada incidencia que se informa agrupada.
#[derive(Default)]
pub(crate) struct GroupedIssues {
    pub(crate) visual_only: Vec<usize>,
    pub(crate) view_hides_text: Vec<usize>,
    pub(crate) animated: Vec<usize>,
    pub(crate) empty: Vec<usize>,
    pub(crate) untitled: Vec<usize>,
    pub(crate) inferred: Vec<usize>,
}

pub(crate) struct Extraction {
    pub(crate) slides: Vec<Slide>,
    pub(crate) issues: Vec<Issue>,
    /// Texto de formas fijas del diseño (layout o patrón) y cuántas diapositivas lo muestran.
    pub(crate) design_texts: Vec<(String, usize)>,
}

/// Umbral de área (fracción de la diapositiva) bajo el cual una imagen sin texto
/// alternativo útil se considera un icono decorativo.
pub(crate) const DECORATIVE_AREA: f64 = 0.01;

/// Detecta un contador visible "05 / 29" cuyo total coincide con la presentación.
pub(crate) fn page_label(text: &str, total: usize) -> Option<String> {
    let (num, den) = text.split_once('/')?;
    let (num, den) = (num.trim(), den.trim());
    let digits = |s: &str| !s.is_empty() && s.len() <= 4 && s.chars().all(|c| c.is_ascii_digit());
    (digits(num) && digits(den) && den.parse::<usize>().ok() == Some(total))
        .then(|| num.to_string())
}

/// Elige como título el texto de mayor tamaño cuando no hay marcador de título:
/// debe ser único, breve, estar en la parte superior y destacar sobre otro texto.
pub(crate) fn infer_title(blocks: &[Option<slide_xml::Block>], slide_height: i64) -> Option<usize> {
    // Los glifos decorativos (comillas, números en círculos) no compiten por el título.
    let sized: Vec<(usize, u32, &slide_xml::TextShape)> = blocks
        .iter()
        .enumerate()
        .filter_map(|(i, b)| match b {
            Some(slide_xml::Block::Text(t)) => t.max_size.map(|s| (i, s, t)),
            _ => None,
        })
        .filter(|(_, _, t)| t.flat_text().chars().filter(|c| c.is_alphabetic()).count() >= 3)
        .collect();
    let (best, size, shape) = sized.iter().max_by_key(|(_, s, _)| *s).copied()?;
    let ties = sized.iter().filter(|(_, s, _)| *s == size).count();
    let smaller = sized.iter().any(|(_, s, _)| *s < size);
    let near_top = shape
        .rect
        .is_none_or(|r| (r.y as f64) < slide_height as f64 * 0.6);
    (ties == 1 && smaller && near_top && shape.flat_text().chars().count() <= 150).then_some(best)
}

/// Antetítulo: texto breve en mayúsculas situado justo encima del título. Devuelve
/// la posición de su bloque.
pub(crate) fn find_kicker(
    blocks: &[Option<slide_xml::Block>],
    title: slide_xml::Rect,
    title_size: Option<u32>,
) -> Option<usize> {
    blocks
        .iter()
        .enumerate()
        .filter_map(|(i, b)| match b {
            Some(slide_xml::Block::Text(t)) => Some((i, t)),
            _ => None,
        })
        .filter_map(|(i, t)| {
            let r = t.rect?;
            let text = t.flat_text();
            let letters = text.chars().filter(|c| c.is_alphabetic()).count() >= 3;
            let upper = text
                .chars()
                .filter(|c| c.is_alphabetic())
                .all(|c| !c.is_lowercase());
            let bottom = r.y + r.cy;
            let above = bottom <= title.y + title.cy / 4 && title.y - bottom <= title.cy;
            let overlaps = r.x < title.x + title.cx && r.x + r.cx > title.x;
            let smaller = match (t.max_size, title_size) {
                (Some(a), Some(b)) => a < b,
                _ => true,
            };
            (letters && upper && above && overlaps && smaller && text.chars().count() <= 80)
                .then_some((bottom, i))
        })
        .max_by_key(|(bottom, _)| *bottom)
        .map(|(_, i)| i)
}

/// Texto de un título. Los párrafos con el mismo formato forman una línea partida y se
/// unen con un espacio; un cambio de formato (p. ej. un subtítulo en otro color) se
/// separa con « · ».
pub(crate) fn title_text(t: &slide_xml::TextShape) -> String {
    let mut segments: Vec<(&str, Vec<&str>)> = Vec::new();
    for p in &t.paragraphs {
        let words: Vec<&str> = p.plain.split_whitespace().collect();
        if words.is_empty() {
            continue;
        }
        match segments.last_mut() {
            Some((style, w)) if *style == p.style => w.extend(words),
            _ => segments.push((&p.style, words)),
        }
    }
    segments
        .iter()
        .map(|(_, w)| w.join(" "))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// Número breve que ocupa por sí solo una forma: el distintivo de un paso o tarjeta.
pub(crate) fn badge_number(t: &slide_xml::TextShape) -> Option<&str> {
    let [p] = t.paragraphs.as_slice() else {
        return None;
    };
    let n = p.plain.trim();
    (!n.is_empty() && n.len() <= 2 && n.chars().all(|c| c.is_ascii_digit())).then_some(n)
}

/// Une cada número de paso con la forma de texto que le sigue cuando están contiguas
/// (a menos de un tamaño del distintivo), para que el contenido diga «1. Tarea» y no
/// «1» y «Tarea» por separado.
pub(crate) fn attach_badges(blocks: &mut [Option<slide_xml::Block>]) {
    for i in 0..blocks.len() {
        let Some(slide_xml::Block::Text(badge)) = &blocks[i] else {
            continue;
        };
        let (Some(n), Some(r)) = (badge_number(badge), badge.rect) else {
            continue;
        };
        let n = n.to_string();
        let Some(j) = (i + 1..blocks.len()).find(|&j| blocks[j].is_some()) else {
            continue;
        };
        let Some(slide_xml::Block::Text(label)) = &mut blocks[j] else {
            continue;
        };
        let reach = r.cx.max(r.cy);
        let near = label.rect.is_some_and(|l| {
            l.x < r.x + r.cx + reach
                && l.x + l.cx > r.x - reach
                && l.y < r.y + r.cy + reach
                && l.y + l.cy > r.y - reach
        });
        if !near || badge_number(label).is_some() {
            continue;
        }
        if let Some(first) = label.paragraphs.first_mut() {
            first.plain = format!("{n}. {}", first.plain);
            first.md = escape_line_start(&format!("{n}. {}", first.md));
            blocks[i] = None;
        }
    }
}

/// «Diapositiva 4» o «Diapositivas 4–6, 11 y 19».
pub(crate) fn slides_phrase(ns: &[usize]) -> String {
    units_phrase("Diapositiva", "Diapositivas", ns)
}

/// Texto de las formas que no son marcadores de posición; en un layout o patrón se
/// muestra en cada diapositiva que lo usa.
pub(crate) fn static_texts(xml: &str) -> Result<Vec<String>> {
    Ok(slide_xml::parse_part(xml, &|_| None)?
        .blocks
        .into_iter()
        .filter_map(|b| match b {
            slide_xml::Block::Text(t) if t.placeholder.is_none() => Some(t.flat_text()),
            _ => None,
        })
        // Un carácter suelto (p. ej. un glifo de una fuente de símbolos) no es texto legible.
        .filter(|t| t.chars().filter(|c| c.is_alphanumeric()).count() >= 2)
        .collect())
}

/// Fracción del área de `a` cubierta por `b`.
pub(crate) fn covered_fraction(a: slide_xml::Rect, b: slide_xml::Rect) -> f64 {
    let w = (a.x + a.cx).min(b.x + b.cx) - a.x.max(b.x);
    let h = (a.y + a.cy).min(b.y + b.cy) - a.y.max(b.y);
    let area = a.cx as f64 * a.cy as f64;
    if w <= 0 || h <= 0 || area <= 0.0 {
        return 0.0;
    }
    (w as f64 * h as f64) / area
}

/// Una imagen que cubre al menos esta fracción de la diapositiva, sin texto que la
/// acompañe, constituye contenido solo visual.
pub(crate) const VISUAL_ONLY_AREA: f64 = 0.15;

pub(crate) const VISUAL_ONLY_MAX_CHARS: usize = 40;

pub(crate) fn extract(bytes: &[u8], attempt: &FsPath) -> Result<Extraction> {
    let mut z = ZipArchive::new(Cursor::new(bytes))?;
    enforce_package_limits(&mut z)?;
    let pres = String::from_utf8(read_entry(&mut z, "ppt/presentation.xml")?)?;
    let rels = String::from_utf8(read_entry(&mut z, "ppt/_rels/presentation.xml.rels")?)?;
    let relmap = relationship_map(&rels);
    let mut order = Vec::<(u32, String, bool)>::new();
    let mut sections = HashMap::<u32, String>::new();
    let mut current_section: Option<String> = None;
    let mut slide_size = (12_192_000i64, 6_858_000i64);
    let mut rd = quick_xml::Reader::from_str(&pres);
    loop {
        match rd.read_event() {
            Ok(quick_xml::events::Event::Start(e)) | Ok(quick_xml::events::Event::Empty(e))
                if e.local_name().as_ref() == b"sldId" =>
            {
                let id = attr(&e, b"id").and_then(|x| x.parse().ok());
                let rid = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref().ends_with(b":id"))
                    .and_then(|a| String::from_utf8(a.value.into_owned()).ok());
                match (id, rid) {
                    (Some(id), Some(rid)) => {
                        if let Some(target) = relmap.get(&rid) {
                            order.push((
                                id,
                                resolve("ppt/presentation.xml", target),
                                attr(&e, b"show").as_deref() == Some("0"),
                            ));
                        }
                    }
                    // Las secciones (p14:sectionLst) enumeran diapositivas sin r:id.
                    (Some(id), None) => {
                        if let Some(name) = &current_section {
                            sections.insert(id, name.clone());
                        }
                    }
                    _ => {}
                }
            }
            Ok(quick_xml::events::Event::Start(e)) if e.local_name().as_ref() == b"section" => {
                current_section = attr(&e, b"name")
            }
            Ok(quick_xml::events::Event::End(e)) if e.local_name().as_ref() == b"section" => {
                current_section = None
            }
            Ok(quick_xml::events::Event::Start(e)) | Ok(quick_xml::events::Event::Empty(e))
                if e.local_name().as_ref() == b"sldSz" =>
            {
                if let (Some(cx), Some(cy)) = (
                    attr(&e, b"cx").and_then(|v| v.parse().ok()),
                    attr(&e, b"cy").and_then(|v| v.parse().ok()),
                ) {
                    slide_size = (cx, cy);
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(e) => return Err(e.into()),
            _ => {}
        }
    }
    if order.is_empty() {
        bail!("PPTX no contiene diapositivas en la secuencia de presentación")
    }
    let total = order.len();
    let positions: HashMap<String, usize> = order
        .iter()
        .enumerate()
        .map(|(i, (_, path, _))| (path.clone(), i + 1))
        .collect();
    let mut slides = Vec::new();
    let mut all_issues = Vec::new();
    for i in 0..z.len() {
        let name = z.by_index(i)?.name().to_string().to_ascii_lowercase();
        if name.starts_with("ppt/embeddings/") {
            all_issues.push(Issue::loss(format!(
                "Objeto incrustado no interpretado: {name}"
            )));
        }
        if name.starts_with("ppt/media/")
            && [".mp3", ".wav", ".m4a", ".mp4", ".mov", ".avi"]
                .iter()
                .any(|ext| name.ends_with(ext))
        {
            all_issues.push(Issue::loss(format!(
                "Audio o video conservado solo en el original: {name}"
            )));
        }
    }
    fs::create_dir_all(attempt.join("assets"))?;
    let rels_of = |z: &mut ZipArchive<Cursor<&[u8]>>, part: &str| -> HashMap<String, String> {
        let relpath = part
            .rsplit_once('/')
            .map(|(p, file)| format!("{p}/_rels/{file}.rels"))
            .unwrap_or_default();
        relationship_map(
            &String::from_utf8(read_entry(z, &relpath).unwrap_or_default()).unwrap_or_default(),
        )
    };
    // Destino Markdown de un hipervínculo: URL externa o concepto de otra diapositiva.
    let link_target = |map: &HashMap<String, String>, part: &str, rid: &str| -> Option<String> {
        let target = map.get(rid)?;
        let lower = target.to_ascii_lowercase();
        if lower.starts_with("http://")
            || lower.starts_with("https://")
            || lower.starts_with("mailto:")
        {
            return Some(target.clone());
        }
        positions
            .get(&resolve(part, target))
            .map(|n| format!("slide-{n:03}.md"))
    };
    let authors = comment_authors(&mut z);
    let mut layout_texts = HashMap::<String, Vec<String>>::new();
    let mut design_texts = Vec::<(String, usize)>::new();
    let mut grouped = GroupedIssues::default();
    let slide_area = (slide_size.0 as f64) * (slide_size.1 as f64);
    let cover =
        |r: Option<slide_xml::Rect>| r.map_or(0.0, |r| (r.cx as f64 * r.cy as f64) / slide_area);
    for (index, (slide_id, path, hidden)) in order.into_iter().enumerate() {
        let raw = String::from_utf8(read_entry(&mut z, &path)?)?;
        let animated = raw.contains("<p:timing");
        let smap = rels_of(&mut z, &path);
        // Texto fijo del layout y del patrón, visible salvo que la diapositiva lo oculte.
        if root_attr(&raw, b"showMasterSp").as_deref() != Some("0")
            && let Some(layout) = smap
                .values()
                .find(|t| t.contains("slideLayouts/"))
                .map(|t| resolve(&path, t))
        {
            if !layout_texts.contains_key(&layout) {
                let mut texts = Vec::new();
                if let Ok(b) = read_entry(&mut z, &layout) {
                    let xml = String::from_utf8(b)?;
                    texts.extend(static_texts(&xml)?);
                    if root_attr(&xml, b"showMasterSp").as_deref() != Some("0") {
                        let lmap = rels_of(&mut z, &layout);
                        if let Some(master) = lmap
                            .values()
                            .find(|t| t.contains("slideMasters/"))
                            .map(|t| resolve(&layout, t))
                            && let Ok(b) = read_entry(&mut z, &master)
                        {
                            texts.extend(static_texts(&String::from_utf8(b)?)?);
                        }
                    }
                }
                layout_texts.insert(layout.clone(), texts);
            }
            for text in &layout_texts[&layout] {
                match design_texts.iter_mut().find(|(t, _)| t == text) {
                    Some((_, n)) => *n += 1,
                    None => design_texts.push((text.clone(), 1)),
                }
            }
        }
        let raw_hidden = quick_xml::Reader::from_str(&raw)
            .read_event()
            .ok()
            .and_then(|e| match e {
                quick_xml::events::Event::Start(e) => attr(&e, b"show"),
                _ => None,
            })
            .as_deref()
            == Some("0");
        let mut s = Slide {
            idx: index + 1,
            slide_id: Some(slide_id),
            hidden: hidden || raw_hidden,
            section: sections.get(&slide_id).cloned(),
            ..Slide::default()
        };
        let part = slide_xml::parse_part(&raw, &|rid| link_target(&smap, &path, rid))?;
        if part.smartart_frames > 0 {
            all_issues.push(Issue::loss(format!(
                "Diapositiva {} contiene SmartArt cuyo texto no se extrajo",
                index + 1
            )));
        }
        s.decorative_images += part.background_images;
        let equations = part.equations;
        let mut blocks: Vec<Option<slide_xml::Block>> = part.blocks.into_iter().map(Some).collect();
        // Numeración visible: marcador sldNum o contador "NN / total" en un cuadro de texto.
        for b in blocks.iter_mut() {
            if let Some(slide_xml::Block::Text(t)) = b {
                let text = t.flat_text();
                let label = match t.placeholder.as_deref() {
                    Some("sldNum") => {
                        Some(text.chars().all(|c| c.is_ascii_digit()).then_some(text))
                    }
                    None => page_label(&text, total).map(Some),
                    _ => None,
                };
                if let Some(label) = label {
                    if label.is_some() {
                        s.visible_label = label;
                    }
                    *b = None;
                }
            }
        }
        // La vista dibuja todas las formas a la vez: una imagen grande posterior a un texto
        // lo tapa, aunque en la presentación la animación los muestre en secuencia.
        if animated {
            let mut earlier: Vec<Option<slide_xml::Rect>> = Vec::new();
            for b in blocks.iter().flatten() {
                match b {
                    slide_xml::Block::Text(t) => earlier.push(t.rect),
                    slide_xml::Block::Picture(p) => {
                        let c = cover(p.rect);
                        if c >= 0.5
                            && earlier.iter().any(|t| {
                                c >= 0.9
                                    || t.zip(p.rect)
                                        .is_some_and(|(t, p)| covered_fraction(t, p) >= 0.5)
                            })
                        {
                            s.view_hides_text = true;
                        }
                    }
                    _ => {}
                }
            }
        }
        let explicit = blocks.iter().position(|b| {
            matches!(b, Some(slide_xml::Block::Text(t))
                if matches!(t.placeholder.as_deref(), Some("title") | Some("ctrTitle")))
        });
        let title_index = explicit.or_else(|| {
            let inferred = infer_title(&blocks, slide_size.1);
            s.title_inferred = inferred.is_some();
            inferred
        });
        if let Some(i) = title_index
            && let Some(slide_xml::Block::Text(t)) = blocks[i].take()
        {
            s.title = Some(title_text(&t));
            // El antetítulo queda en el frontmatter y no se repite en el contenido.
            if let Some(r) = t.rect
                && let Some(k) = find_kicker(&blocks, r, t.max_size)
                && let Some(slide_xml::Block::Text(kicker)) = blocks[k].take()
            {
                s.kicker = Some(kicker.flat_text());
            }
        }
        attach_badges(&mut blocks);
        let mut assets = HashMap::<String, String>::new();
        let mut largest_image = 0.0f64;
        for block in blocks.into_iter().flatten() {
            match block {
                slide_xml::Block::Text(t) => s.items.push(Item::Text(t.paragraphs)),
                slide_xml::Block::Table(rows) => s.items.push(Item::Table(table_markdown(&rows))),
                slide_xml::Block::Picture(p) => {
                    let alt = meaningful_alt(p.descr.as_deref());
                    let small = p.rect.is_some_and(|r| {
                        (r.cx as f64) * (r.cy as f64) < slide_area * DECORATIVE_AREA
                    });
                    if alt.is_none() && small {
                        s.decorative_images += 1;
                        continue;
                    }
                    let Some(target) = smap.get(&p.rid) else {
                        all_issues.push(Issue::loss(format!(
                            "Diapositiva {}: imagen sin relación resoluble ({})",
                            index + 1,
                            p.rid
                        )));
                        continue;
                    };
                    if target.contains("://") {
                        all_issues.push(Issue::loss(format!(
                            "Diapositiva {}: imagen vinculada externamente no incluida: {target}",
                            index + 1
                        )));
                        continue;
                    }
                    if alt.is_none() {
                        largest_image = largest_image.max(cover(p.rect));
                    }
                    let rp = resolve(&path, target);
                    let asset = match assets.get(&rp) {
                        Some(a) => a.clone(),
                        None => {
                            let data = read_entry(&mut z, &rp)?;
                            let ext = FsPath::new(&rp)
                                .extension()
                                .and_then(|x| x.to_str())
                                .unwrap_or("bin")
                                .to_ascii_lowercase();
                            let name = format!(
                                "assets/slide-{:03}-image-{:02}.{ext}",
                                index + 1,
                                assets.len() + 1
                            );
                            fs::write(attempt.join(&name), data)?;
                            assets.insert(rp, name.clone());
                            s.image_paths.push(name.clone());
                            name
                        }
                    };
                    s.items.push(Item::Image { path: asset, alt });
                }
                slide_xml::Block::AltText(d) => {
                    if let Some(d) = meaningful_alt(Some(&d)) {
                        s.items.push(Item::AltText(d));
                    }
                }
                slide_xml::Block::Chart(rid) => {
                    if let Some(target) = smap.get(&rid) {
                        let cp = resolve(&path, target);
                        s.chart_paths.push(cp.clone());
                        if let Ok(b) = read_entry(&mut z, &cp) {
                            let text = chart_text(std::str::from_utf8(&b)?);
                            if !text.is_empty() {
                                s.items.push(Item::Chart(format!(
                                    "Datos del gráfico: {}",
                                    escape_inline(&text)
                                )));
                            }
                        }
                    }
                }
            }
        }
        for target in smap.values().filter(|t| t.contains("comments/")) {
            if let Ok(b) = read_entry(&mut z, &resolve(&path, target)) {
                s.comments
                    .extend(parse_comments(&String::from_utf8(b)?, &authors)?);
            }
        }
        // Las notas son una parte OOXML aparte; se omiten sus marcadores de imagen,
        // número, encabezado, pie y fecha.
        if let Some(target) = smap.values().find(|t| t.contains("notesSlides/")) {
            let np = resolve(&path, target);
            if let Ok(b) = read_entry(&mut z, &np) {
                let nmap = rels_of(&mut z, &np);
                let notes = slide_xml::parse_part(std::str::from_utf8(&b)?, &|rid| {
                    link_target(&nmap, &np, rid)
                })?;
                for block in notes.blocks {
                    if let slide_xml::Block::Text(t) = block
                        && matches!(t.placeholder.as_deref(), None | Some("body"))
                    {
                        s.notes.extend(t.paragraphs.iter().flat_map(|p| {
                            p.md.split('\n')
                                .map(str::trim)
                                .filter(|l| !l.is_empty())
                                .map(str::to_string)
                                .collect::<Vec<_>>()
                        }));
                    }
                }
            }
        }
        let body_chars: usize = s
            .items
            .iter()
            .map(|i| match i {
                Item::Text(p) => p
                    .iter()
                    .map(|p| p.plain.chars().filter(|c| c.is_alphanumeric()).count())
                    .sum(),
                Item::Table(md) | Item::Chart(md) | Item::AltText(md) => {
                    md.chars().filter(|c| c.is_alphanumeric()).count()
                }
                Item::Image { .. } => 0,
            })
            .sum();
        s.visual_only = largest_image >= VISUAL_ONLY_AREA && body_chars < VISUAL_ONLY_MAX_CHARS;
        let n = index + 1;
        if s.visual_only {
            grouped.visual_only.push(n);
        }
        if equations > 0 {
            all_issues.push(Issue::note(format!(
                "Diapositiva {n}: {equations} ecuación(es) convertidas a notación lineal; la vista conserva su forma"
            )));
        }
        if !s.comments.is_empty() {
            let total: usize = s.comments.iter().map(|c| 1 + c.replies.len()).sum();
            all_issues.push(Issue::note(format!(
                "Diapositiva {n}: {total} comentario(s) de revisión conservados como contenido auxiliar"
            )));
        }
        if s.view_hides_text {
            grouped.view_hides_text.push(n);
        } else if animated {
            grouped.animated.push(n);
        }
        if s.items.is_empty() && s.title.is_none() {
            grouped.empty.push(n);
        } else if s.title.is_none() {
            grouped.untitled.push(n);
        } else if s.title_inferred {
            grouped.inferred.push(n);
        }
        slides.push(s);
    }
    // Una línea por tipo de incidencia, con la lista de diapositivas afectadas.
    for (ns, severity, text) in [
        (
            &grouped.visual_only,
            Severity::Loss,
            ": contenido solo visual; la imagen se adjunta sin transcripción",
        ),
        (
            &grouped.view_hides_text,
            Severity::Loss,
            ": la vista estática muestra una imagen sobre texto que la animación presenta en secuencia; el texto está en el concepto, no en la vista",
        ),
        (
            &grouped.animated,
            Severity::Note,
            ": contiene animaciones; la vista muestra todas las formas a la vez",
        ),
        (
            &grouped.empty,
            Severity::Note,
            " sin contenido propio extraíble; lo visible procede del diseño y queda en la vista",
        ),
        (&grouped.untitled, Severity::Note, " sin título explícito"),
        (
            &grouped.inferred,
            Severity::Note,
            ": título inferido por tamaño y posición del texto",
        ),
    ] {
        if !ns.is_empty() {
            all_issues.push(Issue {
                severity,
                text: format!("{}{text}", slides_phrase(ns)),
            });
        }
    }
    for (text, count) in &design_texts {
        all_issues.push(Issue::note(format!(
            "Texto del diseño visible en {count} diapositiva(s), registrado en documento.md y no repetido en sus conceptos: «{}»",
            truncate_chars(text, 120)
        )));
    }
    Ok(Extraction {
        slides,
        issues: all_issues,
        design_texts,
    })
}

pub(crate) fn slide_description(s: &Slide, deck: &str) -> String {
    let mut d = format!("Diapositiva {} de {deck}", s.idx);
    if s.hidden {
        d.push_str(" (oculta)");
    }
    if let Some(t) = &s.title {
        d.push_str(": ");
        d.push_str(t);
    }
    d.push('.');
    // Primera línea con palabras; un número de paso no describe la diapositiva.
    let first = s.items.iter().find_map(|i| match i {
        Item::Text(p) => p
            .iter()
            .map(|p| p.plain.split_whitespace().collect::<Vec<_>>().join(" "))
            .find(|l| l.chars().filter(|c| c.is_alphabetic()).count() >= 3),
        _ => None,
    });
    match first {
        Some(line) => {
            d.push(' ');
            d.push_str(&truncate_chars(&line, 160));
        }
        None if s.visual_only => d.push_str(" Contenido solo visual."),
        None => {}
    }
    d
}

pub(crate) fn index_entry(s: &Slide, href: &str) -> String {
    let label = match &s.title {
        Some(t) => format!("Diapositiva {}: {}", s.idx, escape_inline(t)),
        None => format!("Diapositiva {}", s.idx),
    };
    let mut flags = Vec::new();
    if s.hidden {
        flags.push("oculta");
    }
    if s.visual_only {
        flags.push("contenido solo visual");
    }
    let suffix = if flags.is_empty() {
        String::new()
    } else {
        format!(" — {}", flags.join(", "))
    };
    format!("- [{label}]({href}){suffix}\n")
}

/// Frontmatter y cuerpo del concepto de una diapositiva.
pub(crate) fn markdown_slide(s: &Slide, deck: &str, instant: &str) -> (Fields, String) {
    let span = span(&format!("slide-{}", s.idx), "pptx_slide")
        .with("slide_index", s.idx)
        .with_opt("slide_id", s.slide_id)
        .with_opt("display_label", s.visible_label.as_ref());
    let fields = concept_fields(
        Format::Pptx,
        "Slide",
        s.title.as_deref().unwrap_or("Diapositiva sin título"),
        &slide_description(s, deck),
        instant,
    )
    .with("source_spans", vec![span])
    .flag("title_inferred", s.title_inferred)
    .with_opt("kicker", s.kicker.as_ref())
    .with_opt("section", s.section.as_ref())
    .flag("visual_only", s.visual_only)
    .flag("view_hides_text", s.view_hides_text)
    // Un título inferido cumple reglas estrictas (único, breve, arriba, destacado) y no
    // vuelve borrador el concepto; queda marcado con `title_inferred`.
    .with_opt(
        "status",
        (s.title.is_none() || s.visual_only).then_some("draft"),
    );
    let mut out = String::new();
    if let Some(title) = &s.title {
        out.push_str(&format!("# {}\n\n", escape_inline(title)));
        if s.title_inferred {
            out.push_str("> Título inferido por tamaño y posición del texto; la diapositiva no tiene marcador de título.\n\n");
        }
    } else {
        out.push_str(&format!(
            "# Diapositiva {} (sin título explícito)\n\n",
            s.idx
        ));
    }
    if s.hidden {
        out.push_str("> Diapositiva oculta en la presentación original.\n\n");
    }
    if s.visual_only {
        out.push_str("> Contenido solo visual: lo sustantivo está en la imagen y no tiene transcripción.\n\n");
    }
    if s.view_hides_text {
        out.push_str("> La vista estática muestra una imagen sobre texto que la animación original presenta en secuencia; el texto se conserva abajo.\n\n");
    }
    out.push_str("## Contenido\n\n");
    let mut in_list = false;
    let mut image_no = 0;
    for item in &s.items {
        if in_list && !matches!(item, Item::Text(_)) {
            out.push('\n');
            in_list = false;
        }
        match item {
            Item::Text(paragraphs) => {
                for p in paragraphs {
                    let indent = "  ".repeat(p.level as usize);
                    let mut lines = p.md.split('\n');
                    out.push_str(&format!("{indent}- {}\n", lines.next().unwrap_or("")));
                    for line in lines {
                        out.push_str(&format!("{indent}  {line}\n"));
                    }
                }
                in_list = true;
            }
            Item::Table(md) => out.push_str(&format!("{md}\n")),
            Item::Image { path, alt } => {
                image_no += 1;
                let alt = alt
                    .as_deref()
                    .map(escape_inline)
                    .unwrap_or_else(|| format!("Imagen {image_no} de la diapositiva {}", s.idx));
                out.push_str(&format!("![{alt}](../{path})\n\n"));
            }
            Item::Chart(text) => out.push_str(&format!("{text}\n\n")),
            Item::AltText(text) => {
                out.push_str(&format!("> Texto alternativo: {}\n\n", escape_inline(text)))
            }
        }
    }
    if s.items.is_empty() {
        out.push_str("Sin texto extraído.\n");
    }
    while out.ends_with("\n\n") {
        out.pop();
    }
    if !s.notes.is_empty() {
        out.push_str("\n## Notas del presentador\n\n");
        out.push_str(&s.notes.join("\n\n"));
        out.push('\n');
    }
    if !s.comments.is_empty() {
        out.push('\n');
        out.push_str(&comments_markdown(&s.comments));
    }
    (fields, out)
}

/// Autores de comentarios: `commentAuthors.xml` (clásico) y `authors.xml` (moderno).
pub(crate) fn comment_authors(z: &mut ZipArchive<Cursor<&[u8]>>) -> HashMap<String, String> {
    let mut authors = HashMap::new();
    for part in ["ppt/commentAuthors.xml", "ppt/authors.xml"] {
        let Ok(b) = read_entry(z, part) else { continue };
        let xml = String::from_utf8_lossy(&b);
        let mut reader = quick_xml::Reader::from_str(&xml);
        loop {
            match reader.read_event() {
                Ok(quick_xml::events::Event::Start(e)) | Ok(quick_xml::events::Event::Empty(e))
                    if matches!(e.local_name().as_ref(), b"cmAuthor" | b"author") =>
                {
                    if let (Some(id), Some(name)) = (attr(&e, b"id"), attr(&e, b"name")) {
                        authors.insert(id, name);
                    }
                }
                Ok(quick_xml::events::Event::Eof) | Err(_) => break,
                _ => {}
            }
        }
    }
    authors
}

/// Lee comentarios clásicos (`p:cm` con `p:text`) y modernos (`p188:cm` con párrafos
/// DrawingML y `p188:reply`).
pub(crate) fn parse_comments(xml: &str, authors: &HashMap<String, String>) -> Result<Vec<Comment>> {
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut open: Vec<Comment> = Vec::new();
    let mut done = Vec::new();
    let mut in_text = false;
    loop {
        match reader.read_event()? {
            quick_xml::events::Event::Start(e) => match e.local_name().as_ref() {
                b"cm" | b"reply" => open.push(Comment {
                    author: attr(&e, b"authorId").map(|id| authors.get(&id).cloned().unwrap_or(id)),
                    date: attr(&e, b"dt")
                        .or_else(|| attr(&e, b"created"))
                        .map(|d| d.chars().take(10).collect()),
                    ..Comment::default()
                }),
                b"text" | b"t" => in_text = true,
                _ => {}
            },
            quick_xml::events::Event::Text(t) if in_text => {
                if let Some(c) = open.last_mut() {
                    c.text.push_str(&t.decode()?);
                }
            }
            quick_xml::events::Event::GeneralRef(r) if in_text => {
                if let Some(c) = open.last_mut()
                    && let Some(s) = r.resolve_char_ref()?.map(String::from).or_else(|| {
                        r.decode().ok().and_then(|n| {
                            quick_xml::escape::resolve_predefined_entity(&n).map(str::to_string)
                        })
                    })
                {
                    c.text.push_str(&s);
                }
            }
            quick_xml::events::Event::End(e) => match e.local_name().as_ref() {
                b"text" | b"t" => in_text = false,
                b"p" => {
                    if let Some(c) = open.last_mut() {
                        c.text.push('\n');
                    }
                }
                b"reply" => {
                    if let Some(r) = open.pop()
                        && let Some(parent) = open.last_mut()
                    {
                        parent.replies.push(r);
                    }
                }
                b"cm" => {
                    if let Some(c) = open.pop() {
                        done.push(c);
                    }
                }
                _ => {}
            },
            quick_xml::events::Event::Eof => break,
            _ => {}
        }
    }
    Ok(done)
}

pub(crate) fn render_font() -> Result<Vec<u8>> {
    if let Ok(path) = std::env::var("OKF_FONT_PATH") {
        return fs::read(path).context("no se pudo leer OKF_FONT_PATH");
    }
    for path in [
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
    ] {
        if let Ok(b) = fs::read(path) {
            return Ok(b);
        }
    }
    bail!("no hay fuente compatible; configura OKF_FONT_PATH con una fuente TrueType")
}

pub(crate) fn build_bundle(
    bytes: &[u8],
    filename: &str,
    hash: &str,
    extraction: &Extraction,
    attempt: &FsPath,
) -> Result<(PathBuf, String)> {
    let slides = &extraction.slides;
    let started = Instant::now();
    let mut ppt = Presentation::open(bytes)?;
    let font = render_font()?;
    ppt.register_font("Arial", false, false, &font)?;
    ppt.register_font("Arial", true, false, &font)?;
    if ppt.slides().len() != slides.len() {
        bail!("cantidad de vistas no coincide con diapositivas")
    }
    let generated = chrono_now();
    let mut allissues = extraction.issues.clone();
    let mut out = BundleWriter::create(Format::Pptx, attempt)?;
    fs::create_dir_all(attempt.join("assets"))?;
    for s in slides {
        let rendered = ppt
            .render_png(s.idx - 1, &RenderOptions::default())
            .context("falló renderizado PPTX Rust")?;
        if rendered.bytes.is_empty() {
            bail!("vista obligatoria ausente en diapositiva {}", s.idx)
        }
        let view = format!("assets/slide-{:03}.png", s.idx);
        out.file(&view, &rendered.bytes)?;
        if rendered.skipped_images > 0 {
            allissues.push(Issue::loss(format!(
                "Diapositiva {}: renderizador omitió {} imagen(es)",
                s.idx, rendered.skipped_images
            )));
        }
        let (fields, mut md) = markdown_slide(s, filename, &generated);
        md.push_str(&format!(
            "\n## Vista visual\n\n![Vista de la diapositiva {}](../assets/slide-{:03}.png)\n",
            s.idx, s.idx
        ));
        out.concept(&format!("contenido/slide-{:03}.md", s.idx), &fields, &md)?;
    }
    for entry in fs::read_dir(attempt.join("assets"))? {
        let e = entry?;
        if e.file_type()?.is_file() {
            let name = format!("assets/{}", e.file_name().to_string_lossy());
            out.file(&name, &fs::read(e.path())?)?;
        }
    }
    let nav = Navigation {
        contents_label: "Índice de diapositivas",
        heading: "Diapositivas",
        root_entries: slides
            .iter()
            .map(|s| index_entry(s, &format!("contenido/slide-{:03}.md", s.idx)))
            .collect(),
        contents_entries: slides
            .iter()
            .map(|s| index_entry(s, &format!("slide-{:03}.md", s.idx)))
            .collect(),
    };
    let design_md = if extraction.design_texts.is_empty() {
        String::new()
    } else {
        let lines = extraction
            .design_texts
            .iter()
            .map(|(t, n)| {
                format!(
                    "- {} ({n} diapositiva(s))\n",
                    escape_line_start(&escape_inline(t))
                )
            })
            .collect::<String>();
        format!(
            "\n## Texto del diseño\n\nFormas fijas del layout o del patrón, visibles en las diapositivas que los usan y no repetidas en sus conceptos.\n\n{lines}"
        )
    };
    let doc_fields = document_fields(
        Format::Pptx,
        filename,
        "Presentación fuente convertida a conceptos OKF por diapositiva.",
        &generated,
        hash,
    );
    let doc = format!(
        "# {}\n\nPresentación de {} diapositivas.\n{design_md}",
        escape_inline(filename),
        slides.len()
    );
    let count = |f: fn(&Slide) -> bool| slides.iter().filter(|s| f(s)).count();
    let summary = [
        format!("Diapositivas inventariadas: {}", slides.len()),
        format!(
            "Diapositivas representadas con concepto y vista: {}",
            slides.len()
        ),
        format!("Vistas adjuntas: {}", slides.len()),
        format!(
            "Diapositivas con contenido solo visual: {}",
            count(|s| s.visual_only)
        ),
        format!(
            "Títulos inferidos: {}; diapositivas sin título: {}",
            count(|s| s.title_inferred),
            count(|s| s.title.is_none())
        ),
        format!(
            "Imágenes adjuntas: {}",
            slides.iter().map(|s| s.image_paths.len()).sum::<usize>()
        ),
        format!(
            "Imágenes decorativas visibles solo en las vistas (iconos sin texto alternativo útil, menos del 1 % del área, o fondos): {}",
            slides.iter().map(|s| s.decorative_images).sum::<usize>()
        ),
        "Vistas: BetterOffice PPTX Raster (Rust)".to_string(),
        format!(
            "Duración de ensamblado/renderizado: {} ms",
            started.elapsed().as_millis()
        ),
    ];
    let report = report(Format::Pptx, &generated, hash, &allissues, &summary, "");
    out.finish(bytes, (&doc_fields, &doc), &nav, &report, &allissues)
}

/// Identificadores de las diapositivas en el orden de la presentación. Las entradas
/// de secciones (`p14:sldId`) no llevan `r:id` y no cuentan.
pub(crate) fn presentation_slide_ids(pptx: &[u8]) -> Result<Vec<u32>> {
    let mut z = ZipArchive::new(Cursor::new(pptx))?;
    let pres = String::from_utf8(read_entry(&mut z, "ppt/presentation.xml")?)?;
    let mut rd = quick_xml::Reader::from_str(&pres);
    let mut ids = Vec::new();
    loop {
        match rd.read_event()? {
            quick_xml::events::Event::Start(e) | quick_xml::events::Event::Empty(e)
                if e.local_name().as_ref() == b"sldId"
                    && e.attributes()
                        .flatten()
                        .any(|a| a.key.as_ref().ends_with(b":id")) =>
            {
                ids.push(
                    attr(&e, b"id")
                        .and_then(|v| v.parse().ok())
                        .context("sldId sin identificador numérico")?,
                )
            }
            quick_xml::events::Event::Eof => break,
            _ => {}
        }
    }
    Ok(ids)
}

pub(crate) fn bench_render(input: &FsPath, output: &FsPath) -> Result<()> {
    let started = Instant::now();
    let bytes = fs::read(input)?;
    fs::create_dir_all(output)?;
    let mut ppt = Presentation::open(&bytes)?;
    let font = render_font()?;
    ppt.register_font("Arial", false, false, &font)?;
    ppt.register_font("Arial", true, false, &font)?;
    let mut total = 0usize;
    for i in 0..ppt.slides().len() {
        let image = ppt.render_png(i, &RenderOptions::default())?;
        if image.bytes.is_empty() {
            bail!("sin PNG para diapositiva {}", i + 1)
        }
        let path = output.join(format!("slide-{:03}.png", i + 1));
        fs::write(path, &image.bytes)?;
        total += image.bytes.len();
        println!(
            "slide={} png_bytes={} skipped_images={}",
            i + 1,
            image.bytes.len(),
            image.skipped_images
        );
    }
    println!(
        "renderer=betteroffice-pptx-raster slides={} elapsed_ms={} png_total_bytes={}",
        ppt.slides().len(),
        started.elapsed().as_millis(),
        total
    );
    Ok(())
}

/// Extrae y ensambla el bundle de una presentación.
pub(crate) fn convert(
    cfg: &Config,
    bytes: &[u8],
    filename: &str,
    hash: &str,
    dir: &FsPath,
) -> Result<(PathBuf, String)> {
    let extraction = extract(bytes, dir)?;
    let slides = extraction.slides.len();
    if slides == 0 {
        return Err(Rejected("sin diapositivas".into()).into());
    }
    if slides > cfg.max_slides {
        return Err(Rejected(format!(
            "la presentación tiene {slides} diapositivas; el máximo admitido es {}",
            cfg.max_slides
        ))
        .into());
    }
    build_bundle(bytes, filename, hash, &extraction, dir)
}
/// Comprobaciones PPTX del bundle: una vista enlazada por diapositiva, un concepto por
/// diapositiva del original con su `slide_index` y `slide_id` en el orden de la
/// presentación, y los índices en ese mismo orden.
pub(crate) fn validate(
    z: &mut ZipArchive<fs::File>,
    names: &std::collections::HashSet<String>,
    original: &[u8],
) -> Result<()> {
    let ids = presentation_slide_ids(original)?;
    let mut concepts: Vec<&String> = names
        .iter()
        .filter(|n| n.starts_with("contenido/slide-") && n.ends_with(".md"))
        .collect();
    concepts.sort();
    let expected: Vec<String> = (1..=ids.len())
        .map(|n| format!("contenido/slide-{n:03}.md"))
        .collect();
    if concepts.len() != expected.len() || concepts.iter().zip(&expected).any(|(a, b)| *a != b) {
        bail!(
            "el bundle tiene {} conceptos de diapositiva y el original {} diapositivas",
            concepts.len(),
            ids.len()
        )
    }
    for (n, (name, id)) in expected.iter().zip(&ids).enumerate() {
        let md = read_text(z, name)?;
        let view = format!("assets/slide-{:03}.png", n + 1);
        if !names.contains(&view) || !md.contains(&format!("](../{view})")) {
            bail!(
                "diapositiva {:03} carece de su vista visual obligatoria o de su enlace",
                n + 1
            )
        }
        let fm = frontmatter(&md)?;
        let span = fm
            .get("source_spans")
            .and_then(YamlValue::as_sequence)
            .and_then(|s| s.first())
            .with_context(|| format!("{name} carece de source_spans"))?;
        let index = span.get("slide_index").and_then(YamlValue::as_u64);
        let slide_id = span.get("slide_id").and_then(YamlValue::as_u64);
        if index != Some(n as u64 + 1) || slide_id != Some(u64::from(*id)) {
            bail!(
                "{name} no corresponde a la diapositiva {} del original (slide_id {id})",
                n + 1
            )
        }
    }
    for (index, prefix) in [
        (read_text(z, "index.md")?, "](contenido/slide-"),
        (read_text(z, "contenido/index.md")?, "](slide-"),
    ] {
        let listed: Vec<usize> = index
            .match_indices(prefix)
            .filter_map(|(i, _)| index[i + prefix.len()..].get(..3)?.parse().ok())
            .collect();
        if listed != (1..=ids.len()).collect::<Vec<_>>() {
            bail!("un índice no enumera las diapositivas en el orden del original")
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    fn example(name: &str) -> Vec<u8> {
        fs::read(format!("examples/pptx/{name}.pptx")).unwrap()
    }
    /// Presentaciones reales del curso. Por defecto se usan las copias reducidas de
    /// `examples/pptx/corpus` (XML íntegro, imágenes mínimas); `OKF_CORPUS_DIR` permite
    /// apuntar a los originales.
    fn corpus(name: &str) -> (Vec<Slide>, Vec<Issue>, String) {
        let dir = std::env::var("OKF_CORPUS_DIR").unwrap_or_else(|_| "examples/pptx/corpus".into());
        let path = format!("{dir}/{name}.pptx");
        let bytes = fs::read(&path).unwrap_or_else(|e| panic!("no se pudo leer {path}: {e}"));
        let tmp = tempfile::tempdir().unwrap();
        let Extraction { slides, issues, .. } = extract(&bytes, tmp.path()).unwrap();
        let md = slides
            .iter()
            .map(|s| {
                let (fields, body) = markdown_slide(s, name, "2026-01-01T00:00:00Z");
                fields.frontmatter() + &body
            })
            .collect();
        (slides, issues, md)
    }
    fn text_lines(s: &Slide) -> Vec<String> {
        s.items
            .iter()
            .filter_map(|i| match i {
                Item::Text(p) => Some(p.iter().map(|p| p.plain.clone()).collect::<Vec<_>>()),
                _ => None,
            })
            .flatten()
            .collect()
    }
    #[test]
    fn corpus_s1_infers_titles_labels_links_and_drops_icons() {
        let (slides, issues, md) = corpus("s1");
        assert_eq!(slides.len(), 29);
        assert!(slides.iter().all(|s| s.title.is_some() && s.title_inferred));
        assert!(issues.iter().all(|i| !i.text.contains("sin título")));
        // Títulos inferidos e iconos se informan, pero no son pérdidas.
        assert!(issues.iter().all(|i| i.severity == Severity::Note));
        let s18 = &slides[17];
        assert_eq!(s18.title.as_deref(), Some("Localizar el fallo"));
        assert_eq!(s18.kicker.as_deref(), Some("ARQUITECTURA Y DIAGNÓSTICO"));
        // El antetítulo no se repite en el contenido y los títulos inferidos no son borrador.
        assert!(!text_lines(s18).contains(&"ARQUITECTURA Y DIAGNÓSTICO".to_string()));
        assert!(!md.contains("status: draft"));
        // Los números de paso se unen con su etiqueta.
        let s12 = text_lines(&slides[11]);
        assert_eq!(
            s12[..2],
            ["1. Tarea".to_string(), "2. Evidencia".to_string()]
        );
        assert!(md.contains("- 1\\. Tarea\n- 2\\. Evidencia"));
        assert!(
            md.contains(
                "description: \"Diapositiva 12 de s1: El recorrido de la clase. 1. Tarea\""
            )
        );
        // Las observaciones repetidas se agrupan en una línea.
        assert!(issues.iter().any(
            |i| i.text == "Diapositivas 1–29: título inferido por tamaño y posición del texto"
        ));
        assert_eq!(slides[4].visible_label.as_deref(), Some("05"));
        assert!(
            slides
                .iter()
                .all(|s| text_lines(s).iter().all(|t| !t.contains(" / 29")))
        );
        // Fragmentos de un mismo párrafo se unen sin espacios añadidos.
        assert!(!md.contains("Determinismo  hasta"));
        // Las notas conservan sus bloques y no arrastran el número de diapositiva.
        let notes = &slides[1].notes;
        assert_eq!(notes.first().map(String::as_str), Some("GUÍA DOCENTE"));
        assert!(notes.iter().all(|n| n.trim() != "2" && !n.ends_with(" 2")));
        assert!(md.contains("](https://misw4411-aplicaciones-basadas-en-llms.github.io/llm-retrieval-engineering-course/)"));
        assert!(slides.iter().map(|s| s.decorative_images).sum::<usize>() >= 80);
        assert!(slides.iter().map(|s| s.image_paths.len()).sum::<usize>() <= 20);
        assert!(md.contains("- \\+ consulta"));
    }
    #[test]
    fn corpus_s2_s3_keep_entities_and_clean_notes() {
        let (s2, issues2, md2) = corpus("s2");
        let (s3, _, md3) = corpus("s3");
        assert_eq!((s2.len(), s3.len()), (27, 34));
        assert_eq!(s2[6].title.as_deref(), Some("Chunk Size & Chunk Overlap"));
        assert!(s2.iter().chain(&s3).all(|s| !s.title_inferred));
        for md in [&md2, &md3] {
            assert!(!md.contains("‹#›"));
        }
        // Diapositiva solo visual: la imagen se enlaza y las notas se conservan.
        let s3_4 = &s3[3];
        assert!(s3_4.title.is_none());
        assert!(matches!(s3_4.items.as_slice(), [Item::Image { .. }]));
        assert!(s3_4.notes.iter().any(|n| n.contains("vecino más cercano")));
        assert!(s3_4.visual_only && s2[6].visual_only);
        assert_eq!(s2.iter().filter(|s| s.visual_only).count(), 15);
        // Animación con imagen a pantalla completa sobre el texto (s3, diapositiva 21).
        assert!(s3[20].view_hides_text && !s3[20].visual_only);
        // Un cambio de formato separa las partes del título; una línea partida se une.
        assert_eq!(
            s3[20].title.as_deref(),
            Some("Fidelidad (Faithfulness) · Componente: Generador")
        );
        assert_eq!(
            s3[14].title.as_deref(),
            Some("Retrieval Augmented Generation Assessment")
        );
        assert_eq!(
            s3[27].title.as_deref(),
            Some("Exhaustividad del contexto · Componente: Recuperador")
        );
        assert!(issues2.iter().all(|i| !i.text.ends_with("«c»")));
        assert!(issues2.iter().any(|i| i.severity == Severity::Note
            && i.text.contains("Texto del diseño")
            && i.text.contains("MISO – Maestría en Ingeniería de Software")));
    }
    #[test]
    fn equations_comments_and_alt_text_are_kept_without_duplicates() {
        let bytes = example("rich");
        let tmp = tempfile::tempdir().unwrap();
        let extraction = extract(&bytes, tmp.path()).unwrap();
        let s = &extraction.slides[0];
        let (_, md) = markdown_slide(s, "rich.pptx", "2026-01-01T00:00:00Z");
        assert!(md.contains("- `E=mc^2+a/b`"), "{md}");
        // La imagen de mc:Fallback representa la misma ecuación: no se adjunta.
        assert!(s.image_paths.is_empty());
        assert!(md.contains("> Texto alternativo: Ciclo: consulta, recuperación y respuesta"));
        assert!(md.contains("> Texto alternativo: Flecha de la consulta hacia el índice"));
        assert!(md.contains("- **Revisora** · 2026-10-30: Revisar la cifra de la ecuación"));
        let notes: Vec<&str> = extraction
            .issues
            .iter()
            .filter(|i| i.severity == Severity::Note)
            .map(|i| i.text.as_str())
            .collect();
        assert!(notes.iter().any(|n| n.contains("ecuación(es) convertidas")));
        assert!(
            notes
                .iter()
                .any(|n| n.contains("1 comentario(s) de revisión"))
        );
        let (zip, state) = build_bundle(
            &bytes,
            "rich.pptx",
            &hex(&Sha256::digest(&bytes)),
            &extraction,
            tmp.path(),
        )
        .unwrap();
        assert_eq!(state, "complete");
        validate_bundle(&zip).unwrap();
    }
    #[test]
    fn modern_comments_keep_authors_and_replies() {
        let authors = HashMap::from([("{A1}".to_string(), "Ana".to_string())]);
        let xml = r#"<p188:cmLst xmlns:p188="p188" xmlns:a="a"><p188:cm id="{C1}" authorId="{A1}" created="2026-10-29T09:15:00.000"><p188:replyLst><p188:reply id="{R1}" authorId="{B2}" created="2026-10-30T11:00:00.000"><p188:txBody><a:p><a:r><a:t>Hecho</a:t></a:r></a:p></p188:txBody></p188:reply></p188:replyLst><p188:txBody><a:p><a:r><a:t>Falta la fuente &amp; la fecha</a:t></a:r></a:p></p188:txBody></p188:cm></p188:cmLst>"#;
        let comments = parse_comments(xml, &authors).unwrap();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].author.as_deref(), Some("Ana"));
        assert_eq!(comments[0].date.as_deref(), Some("2026-10-29"));
        assert_eq!(comments[0].text.trim(), "Falta la fuente & la fecha");
        assert_eq!(comments[0].replies[0].author.as_deref(), Some("{B2}"));
        assert_eq!(comments[0].replies[0].text.trim(), "Hecho");
    }
    #[test]
    fn chart_cache_values_are_recoverable_from_chart_xml() {
        let xml = r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:v>Revenue</c:v><c:v>North</c:v><c:v>12</c:v></c:chartSpace>"#;
        let text = chart_text(xml);
        assert!(text.contains("Revenue"));
        assert!(text.contains("North"));
        assert!(text.contains("12"));
    }
}
