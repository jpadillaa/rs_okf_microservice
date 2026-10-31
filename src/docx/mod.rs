//! Adaptador DOCX: lee el paquete con el lector OOXML propio, numera los bloques del
//! cuerpo (párrafos y tablas de primer nivel, desde 1), reconstruye la jerarquía de
//! encabezados y divide el documento en conceptos por secciones.
mod read;
mod styles;
mod xml;

use crate::bundle::*;
use crate::ooxml::*;
use crate::worker::{Config, Rejected};
use anyhow::{Context, Result, bail};
use read::{Aux, Para, Reader, Table};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Cursor,
    path::{Path as FsPath, PathBuf},
    time::Instant,
};
use styles::{Numbering, Styles};
use yaml_serde::Value as YamlValue;
use zip::{ZipArchive, ZipWriter};

/// Por debajo de este número de caracteres el documento es breve y forma un solo
/// concepto aunque tenga encabezados.
pub(crate) const BRIEF_MAX_CHARS: usize = 1500;
/// Longitud máxima de un párrafo sin estilo de encabezado que puede inferirse como tal.
const INFERRED_HEADING_MAX_CHARS: usize = 150;

pub(crate) enum Block {
    Para(Box<Para>),
    Table(Table),
}

/// Encabezado o pie de página: texto e imágenes repetidos en las páginas.
pub(crate) struct Furniture {
    kind: &'static str,
    paras: Vec<Para>,
}

pub(crate) struct Extraction {
    /// Bloques del cuerpo; el bloque `i` tiene el ordinal `i + 1`.
    pub(crate) blocks: Vec<Block>,
    footnotes: HashMap<String, Vec<Para>>,
    endnotes: HashMap<String, Vec<Para>>,
    footnote_order: Vec<String>,
    endnote_order: Vec<String>,
    comments: Vec<Comment>,
    /// Hilo (comentario de primer nivel) al que pertenece cada identificador.
    comment_thread: HashMap<String, usize>,
    furniture: Vec<Furniture>,
    /// Texto en caché de cada gráfico, por parte.
    chart_texts: HashMap<String, String>,
    /// Texto de los nodos de cada SmartArt, por parte de datos.
    smartart_texts: HashMap<String, String>,
    core_title: Option<String>,
    /// Fragmentos importados (`altChunk`): número de bloques que los preceden.
    alt_chunks: Vec<usize>,
}

/// Número de bloques del cuerpo de un DOCX: la base de los localizadores.
pub(crate) fn block_count(docx: &[u8]) -> Result<usize> {
    let mut z = ZipArchive::new(Cursor::new(docx))?;
    let main = main_part(&mut z).unwrap_or_else(|| "word/document.xml".into());
    let doc = String::from_utf8(read_entry(&mut z, &main)?)?;
    let mut n = 0;
    xml::body_elements(&doc, |node, _| {
        n += usize::from(xml::is_block(&node));
        Ok(())
    })?;
    Ok(n)
}

pub(crate) fn extract(bytes: &[u8], max_blocks: usize) -> Result<Extraction> {
    let mut z = ZipArchive::new(Cursor::new(bytes))?;
    enforce_package_limits(&mut z)?;
    let main = main_part(&mut z).unwrap_or_else(|| "word/document.xml".into());
    let doc_rels = relationships(&mut z, &main);
    let first = |z: &mut ZipArchive<Cursor<&[u8]>>, suffix: &str| {
        related(&doc_rels, &main, suffix)
            .first()
            .and_then(|p| parse_entry(z, p))
    };
    let styles = first(&mut z, "/styles")
        .map(|n| Styles::parse(&n))
        .unwrap_or_default();
    let numbering = first(&mut z, "/numbering")
        .map(|n| Numbering::parse(&n))
        .unwrap_or_default();
    let mut reader = Reader::new(&styles, &numbering);
    reader.set_part(&main, rel_map(&doc_rels));
    let doc = String::from_utf8(read_entry(&mut z, &main)?)
        .context("document.xml no está codificado en UTF-8")?;
    let mut blocks = Vec::new();
    let mut alt_chunks = Vec::new();
    xml::body_elements(&doc, |n, gallery| {
        if n.is(b"p") {
            let mut p = reader.paragraph(&n);
            p.toc |= gallery == Some("Table of Contents");
            blocks.push(Block::Para(Box::new(p)));
        } else if n.is(b"tbl") {
            blocks.push(Block::Table(reader.table(&n)));
        } else if n.is(b"altChunk") {
            alt_chunks.push(blocks.len());
        }
        if blocks.len() > max_blocks {
            return Err(Rejected(format!(
                "el documento supera el máximo admitido de {max_blocks} bloques"
            ))
            .into());
        }
        Ok(())
    })?;
    drop(doc);
    let notes = |z: &mut ZipArchive<Cursor<&[u8]>>, reader: &mut Reader, suffix: &str| {
        for path in related(&doc_rels, &main, suffix) {
            let rels = relationships(z, &path);
            reader.set_part(&path, rel_map(&rels));
            if let Some(root) = parse_entry(z, &path) {
                return reader.notes(&root);
            }
        }
        HashMap::new()
    };
    let footnotes = notes(&mut z, &mut reader, "/footnotes");
    let endnotes = notes(&mut z, &mut reader, "/endnotes");
    let (comments, comment_thread) = comments(&mut z, &mut reader, &doc_rels, &main);
    let mut furniture = Vec::new();
    for (suffix, kind) in [("/header", "Encabezado"), ("/footer", "Pie de página")] {
        for path in related(&doc_rels, &main, suffix) {
            let rels = relationships(&mut z, &path);
            reader.set_part(&path, rel_map(&rels));
            if let Some(root) = parse_entry(&mut z, &path) {
                let paras: Vec<Para> = reader
                    .flow(&root)
                    .into_iter()
                    .filter(|p| !p.is_empty())
                    .collect();
                if !paras.is_empty() {
                    furniture.push(Furniture { kind, paras });
                }
            }
        }
    }
    let all_aux: Vec<&Aux> = blocks
        .iter()
        .flat_map(|b| match b {
            Block::Para(p) => vec![&p.aux],
            Block::Table(t) => t.paras().into_iter().map(|p| &p.aux).collect(),
        })
        .collect();
    let mut chart_texts = HashMap::new();
    for c in all_aux.iter().flat_map(|a| a.charts.iter()) {
        if let Ok(b) = read_entry(&mut z, c) {
            chart_texts.insert(c.clone(), chart_text(&String::from_utf8_lossy(&b)));
        }
    }
    let mut smartart_texts = HashMap::new();
    for d in all_aux.iter().flat_map(|a| a.smartart.iter()) {
        if let Some(root) = parse_entry(&mut z, d) {
            smartart_texts.insert(d.clone(), smartart_text(&root));
        }
    }
    let core_title = parse_entry(&mut z, "docProps/core.xml")
        .and_then(|root| root.find(b"title").map(read::node_text))
        .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|t| !t.is_empty());
    Ok(Extraction {
        blocks,
        footnotes,
        endnotes,
        footnote_order: reader.footnote_order.clone(),
        endnote_order: reader.endnote_order.clone(),
        comments,
        comment_thread,
        furniture,
        chart_texts,
        smartart_texts,
        core_title,
        alt_chunks,
    })
}

/// Texto de los nodos de un SmartArt (`dgm:pt` sin tipo o de tipo `node`), en orden.
fn smartart_text(root: &xml::Node) -> String {
    let mut pts = Vec::new();
    root.find_all(b"pt", b"", &mut pts);
    pts.iter()
        .filter(|pt| pt.attr(b"type").is_none_or(|t| t == "node"))
        .map(|pt| pt.child(b"t").map(read::node_text).unwrap_or_default())
        .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

/// Comentarios con sus respuestas (`commentsExtended`) y el texto que anclan.
fn comments(
    z: &mut ZipArchive<Cursor<&[u8]>>,
    reader: &mut Reader,
    doc_rels: &[Rel],
    main: &str,
) -> (Vec<Comment>, HashMap<String, usize>) {
    let mut top: Vec<Comment> = Vec::new();
    let mut thread = HashMap::new();
    let Some(path) = related(doc_rels, main, "/comments").into_iter().next() else {
        return (top, thread);
    };
    let anchors = reader.anchors.clone();
    let rels = relationships(z, &path);
    reader.set_part(&path, rel_map(&rels));
    let Some(root) = parse_entry(z, &path) else {
        return (top, thread);
    };
    // paraId del último párrafo de cada comentario → paraId del comentario al que responde.
    let parents: HashMap<String, String> = related(doc_rels, main, "/commentsExtended")
        .first()
        .and_then(|p| parse_entry(z, p))
        .map(|r| {
            r.nodes()
                .filter(|n| n.is(b"commentEx"))
                .filter_map(|n| {
                    Some((
                        n.attr(b"paraId")?.to_string(),
                        n.attr(b"paraIdParent")?.to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    let mut by_para: HashMap<String, usize> = HashMap::new();
    for c in root.nodes().filter(|n| n.is(b"comment")) {
        let Some(id) = c.attr(b"id").map(str::to_string) else {
            continue;
        };
        let last_para = c
            .nodes()
            .filter(|p| p.is(b"p"))
            .last()
            .and_then(|p| p.attr(b"paraId"))
            .map(str::to_string);
        let mut text = reader
            .flow(c)
            .iter()
            .map(Para::flat)
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        if let Some(a) = anchors.get(&id) {
            let a = a.split_whitespace().collect::<Vec<_>>().join(" ");
            if !a.is_empty() {
                text = format!("{text} (sobre «{}»)", truncate_chars(&a, 120));
            }
        }
        let comment = Comment {
            author: c
                .attr(b"author")
                .map(str::to_string)
                .filter(|a| !a.is_empty()),
            date: c.attr(b"date").map(|d| d.chars().take(10).collect()),
            text,
            replies: Vec::new(),
        };
        let parent = last_para
            .as_ref()
            .and_then(|p| parents.get(p))
            .and_then(|p| by_para.get(p))
            .copied();
        let index = match parent {
            Some(i) => {
                top[i].replies.push(comment);
                i
            }
            None => {
                top.push(comment);
                top.len() - 1
            }
        };
        if let Some(p) = last_para {
            by_para.insert(p, index);
        }
        thread.insert(id, index);
    }
    (top, thread)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Heading {
    /// Nivel desde 1.
    pub(crate) level: u8,
    /// Sin estilo ni nivel de esquema: inferido por tener el formato de un encabezado.
    pub(crate) inferred: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct Section {
    pub(crate) title: String,
    /// Bloque del encabezado que abre la sección.
    pub(crate) heading: Option<usize>,
    /// Primer y último bloque (índices desde 0, inclusivos).
    pub(crate) from: usize,
    pub(crate) to: usize,
    /// Contenido anterior al primer encabezado de división.
    pub(crate) preamble: bool,
    pub(crate) inferred: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum TitleSource {
    Style,
    Heading,
    Metadata,
    Filename,
}

pub(crate) struct Outline {
    pub(crate) headings: Vec<Option<Heading>>,
    pub(crate) title: String,
    pub(crate) title_block: Option<usize>,
    pub(crate) title_source: TitleSource,
    /// Nivel de encabezado que divide el documento en conceptos; `None` si es breve.
    pub(crate) split: Option<u8>,
    /// Desplazamiento para escribir los subencabezados con `##` en adelante.
    base: i32,
    pub(crate) sections: Vec<Section>,
}

impl Outline {
    fn hashes(&self, level: u8) -> String {
        "#".repeat((level as i32 - self.base).clamp(2, 6) as usize)
    }
    pub(crate) fn file(&self, k: usize) -> String {
        match self.split {
            None => "texto.md".into(),
            Some(_) => format!("seccion-{:03}.md", k + 1),
        }
    }
}

/// Texto de un encabezado, con su número de lista si lo tiene («2. Métodos»).
fn heading_text(p: &Para) -> String {
    let t = p.flat();
    match &p.list {
        Some(l) => match (l.ordinal, &l.label) {
            (Some(n), _) => format!("{n}. {t}"),
            (None, Some(label)) => format!("{label} {t}"),
            _ => t,
        },
        None => t,
    }
}

fn block_chars(b: &Block) -> usize {
    match b {
        Block::Para(p) => p.flat().chars().count(),
        Block::Table(t) => t.paras().iter().map(|p| p.flat().chars().count()).sum(),
    }
}

pub(crate) fn outline(ex: &Extraction, filename: &str) -> Outline {
    let n = ex.blocks.len();
    let para = |i: usize| match &ex.blocks[i] {
        Block::Para(p) => Some(&**p),
        Block::Table(_) => None,
    };
    let mut headings: Vec<Option<Heading>> = (0..n)
        .map(|i| {
            para(i)
                .filter(|p| !p.flat().is_empty() && !p.toc)
                .and_then(|p| p.outline)
                .map(|l| Heading {
                    level: l + 1,
                    inferred: false,
                })
        })
        .collect();
    // Tamaño del texto corriente: el más frecuente por caracteres fuera de encabezados.
    let mut weights: HashMap<u32, usize> = HashMap::new();
    let mut heading_sizes: HashMap<u32, u8> = HashMap::new();
    for (i, heading) in headings.iter().enumerate() {
        let Some(p) = para(i) else { continue };
        let Some(size) = p.uniform_size() else {
            continue;
        };
        match heading {
            Some(h) => {
                let l = heading_sizes.entry(size).or_insert(h.level);
                *l = (*l).min(h.level);
            }
            None => *weights.entry(size).or_default() += p.flat().chars().count(),
        }
    }
    let body = weights
        .iter()
        .max_by_key(|(s, w)| (**w, std::cmp::Reverse(**s)))
        .map(|(s, _)| *s);
    // Un párrafo breve sin estilo de encabezado, con el mismo tamaño que los encabezados
    // de un nivel y mayor que el texto corriente, es un encabezado con formato directo.
    for (i, heading) in headings.iter_mut().enumerate() {
        let Some(p) = para(i) else { continue };
        if heading.is_some() || p.list.is_some() || p.title_style || p.body_level || p.toc {
            continue;
        }
        let t = p.flat();
        let candidate = !t.is_empty()
            && t.chars().count() <= INFERRED_HEADING_MAX_CHARS
            && !t.ends_with(['.', ':', ';', ',']);
        if let (true, Some(size), Some(body)) = (candidate, p.uniform_size(), body)
            && size > body
            && let Some(&level) = heading_sizes.get(&size)
        {
            *heading = Some(Heading {
                level,
                inferred: true,
            });
        }
    }
    let content = |i: usize| match &ex.blocks[i] {
        Block::Para(p) => !p.is_empty(),
        Block::Table(_) => true,
    };
    let has_text = |i: usize| match &ex.blocks[i] {
        Block::Para(p) => !p.flat().is_empty(),
        Block::Table(_) => true,
    };
    // Título: el estilo «Title» al comienzo, o un primer encabezado seguido directamente
    // (sin texto entre ambos) por otro de igual o mayor rango.
    let mut title_block = None;
    let mut title_source = TitleSource::Filename;
    if let Some(f) = (0..n).find(|&i| content(i))
        && let Some(p) = para(f)
        && !p.flat().is_empty()
    {
        if p.title_style {
            title_block = Some(f);
            title_source = TitleSource::Style;
        } else if let Some(h) = headings[f]
            && let Some(next) = (f + 1..n).find(|&i| has_text(i))
            && headings[next].is_some_and(|x| x.level <= h.level)
        {
            title_block = Some(f);
            title_source = TitleSource::Heading;
        }
    }
    let title = match title_block {
        Some(t) => {
            headings[t] = None;
            heading_text(para(t).expect("el título es un párrafo"))
        }
        None => match &ex.core_title {
            Some(t) => {
                title_source = TitleSource::Metadata;
                t.clone()
            }
            None => FsPath::new(filename)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| filename.to_string()),
        },
    };
    let mut counts = [0usize; 10];
    for h in headings.iter().flatten() {
        counts[h.level as usize] += 1;
    }
    let total: usize = ex.blocks.iter().map(block_chars).sum();
    // Se divide por el nivel más alto que tenga al menos dos encabezados.
    let split = (1..=9u8)
        .find(|&l| counts[l as usize] >= 2)
        .filter(|_| total >= BRIEF_MAX_CHARS);
    let min_level = headings.iter().flatten().map(|h| h.level).min();
    let sections = match split {
        None => vec![Section {
            title: title.clone(),
            heading: None,
            from: 0,
            to: n.saturating_sub(1),
            preamble: false,
            inferred: false,
        }],
        Some(level) => {
            let starts: Vec<usize> = (0..n)
                .filter(|&i| headings[i].is_some_and(|h| h.level == level))
                .collect();
            let mut out = Vec::new();
            let opening = (0..starts[0]).find(|&i| Some(i) != title_block && content(i));
            if let Some(o) = opening {
                // Un encabezado de rango superior al comienzo da nombre al inicio.
                let named = headings[o].map(|h| (o, h));
                out.push(Section {
                    title: named
                        .and_then(|(o, _)| para(o).map(heading_text))
                        .unwrap_or_else(|| title.clone()),
                    heading: named.map(|(o, _)| o),
                    from: 0,
                    to: starts[0] - 1,
                    preamble: true,
                    inferred: named.is_some_and(|(_, h)| h.inferred),
                });
            }
            for (k, &s) in starts.iter().enumerate() {
                out.push(Section {
                    title: para(s).map(heading_text).unwrap_or_default(),
                    heading: Some(s),
                    from: if k == 0 && opening.is_none() { 0 } else { s },
                    to: starts.get(k + 1).map_or(n - 1, |next| next - 1),
                    preamble: false,
                    inferred: headings[s].is_some_and(|h| h.inferred),
                });
            }
            out
        }
    };
    let base = match split {
        Some(l) => l as i32 - 1,
        None => min_level.map_or(0, |m| m as i32 - 2),
    };
    Outline {
        headings,
        title,
        title_block,
        title_source,
        split,
        base,
        sections,
    }
}

/// Recursos adjuntos: una ruta estable por parte del paquete, numerada por prefijo en
/// orden de aparición.
struct Assets {
    names: HashSet<String>,
    paths: HashMap<String, (String, usize)>,
    counts: HashMap<&'static str, usize>,
    files: Vec<(String, String)>,
}

impl Assets {
    fn path(&mut self, target: &str, prefix: &'static str) -> Option<(String, usize)> {
        if !self.names.contains(target) {
            return None;
        }
        if let Some(p) = self.paths.get(target) {
            return Some(p.clone());
        }
        let n = self.counts.entry(prefix).or_default();
        *n += 1;
        let ext = target
            .rsplit_once('.')
            .map(|(_, e)| e.to_ascii_lowercase())
            .filter(|e| e.len() <= 5 && e.chars().all(|c| c.is_ascii_alphanumeric()))
            .unwrap_or_else(|| "bin".into());
        let path = format!("assets/{prefix}-{:03}.{ext}", *n);
        self.paths.insert(target.to_string(), (path.clone(), *n));
        self.files.push((target.to_string(), path.clone()));
        Some((path, *n))
    }
}

/// Ordinales de los bloques afectados por cada tipo de incidencia u observación.
#[derive(Default)]
struct Tally {
    images: usize,
    decorative: usize,
    metafiles: Vec<usize>,
    missing: Vec<usize>,
    missing_furniture: usize,
    charts: Vec<usize>,
    empty_charts: Vec<usize>,
    smartart: Vec<usize>,
    smartart_text: Vec<usize>,
    objects: Vec<usize>,
    toc: Vec<usize>,
    linked: Vec<usize>,
    textboxes: Vec<usize>,
    hidden: Vec<usize>,
    revisions: Vec<usize>,
    comments: Vec<usize>,
    equations: Vec<usize>,
    equation_count: usize,
    merged: Vec<usize>,
    nested: Vec<usize>,
    inferred: Vec<usize>,
    tables: usize,
    lists: usize,
    links: usize,
    empty: usize,
    furniture_texts: usize,
    furniture_images: usize,
}

fn mark(v: &mut Vec<usize>, ordinal: usize) {
    if v.last() != Some(&ordinal) {
        v.push(ordinal);
    }
}

/// Contenido auxiliar que se reúne al final de un concepto.
#[derive(Default)]
struct Refs {
    footnotes: Vec<String>,
    endnotes: Vec<String>,
    comments: Vec<usize>,
    hidden: Vec<(usize, String)>,
    revisions: Vec<(usize, &'static str, String)>,
}

fn snippet(s: &str) -> String {
    escape_inline(&truncate_chars(
        &s.split_whitespace().collect::<Vec<_>>().join(" "),
        300,
    ))
}

struct Render<'a> {
    ex: &'a Extraction,
    outline: &'a Outline,
    assets: Assets,
    tally: Tally,
}

impl Render<'_> {
    fn is_toc(&self, i: usize) -> bool {
        matches!(&self.ex.blocks[i], Block::Para(p) if p.toc)
    }

    fn note_aux(&mut self, ordinal: usize, aux: &Aux, refs: &mut Refs) {
        let t = &mut self.tally;
        for d in &aux.smartart {
            if self.ex.smartart_texts.get(d).is_some_and(|t| !t.is_empty()) {
                mark(&mut t.smartart_text, ordinal);
            } else {
                mark(&mut t.smartart, ordinal);
            }
        }
        if !aux.objects.is_empty() {
            mark(&mut t.objects, ordinal);
        }
        if aux.linked_images > 0 {
            mark(&mut t.linked, ordinal);
        }
        if !aux.textboxes.is_empty() {
            mark(&mut t.textboxes, ordinal);
        }
        if aux.equations > 0 {
            mark(&mut t.equations, ordinal);
            t.equation_count += aux.equations;
        }
        t.links += aux.links;
        if !aux.hidden.is_empty() {
            mark(&mut t.hidden, ordinal);
            refs.hidden
                .extend(aux.hidden.iter().map(|h| (ordinal, h.clone())));
        }
        if !aux.revisions.is_empty() {
            mark(&mut t.revisions, ordinal);
            refs.revisions
                .extend(aux.revisions.iter().map(|(k, h)| (ordinal, *k, h.clone())));
        }
        for id in &aux.comments {
            if let Some(&i) = self.ex.comment_thread.get(id) {
                mark(&mut t.comments, ordinal);
                if !refs.comments.contains(&i) {
                    refs.comments.push(i);
                }
            }
        }
        refs.footnotes.extend(aux.footnotes.iter().cloned());
        refs.endnotes.extend(aux.endnotes.iter().cloned());
    }

    /// Imagen enlazada desde un concepto (`../assets/...`); `None` si es decorativa.
    fn image_md(
        &mut self,
        ordinal: usize,
        img: &read::Image,
        prefix: &'static str,
        up: &str,
    ) -> Option<String> {
        if img.decorative {
            self.tally.decorative += 1;
            return None;
        }
        match self.assets.path(&img.target, prefix) {
            Some((path, n)) => {
                self.tally.images += 1;
                if ordinal > 0 && (path.ends_with(".emf") || path.ends_with(".wmf")) {
                    mark(&mut self.tally.metafiles, ordinal);
                }
                let alt = img
                    .alt
                    .as_deref()
                    .map(escape_inline)
                    .unwrap_or_else(|| match prefix {
                        "encabezado" => format!("Imagen {n} de encabezado o pie de página"),
                        _ => format!("Imagen {n}"),
                    });
                Some(format!("![{alt}]({up}{path})"))
            }
            None => {
                // El ordinal 0 corresponde a encabezados y pies de página.
                if ordinal == 0 {
                    self.tally.missing_furniture += 1;
                } else {
                    mark(&mut self.tally.missing, ordinal);
                }
                Some("_Imagen ausente del paquete original._".into())
            }
        }
    }

    /// Imágenes, gráficos y cuadros de texto de un bloque, debajo de su texto.
    fn aux_md(&mut self, ordinal: usize, aux: &Aux, indent: &str) -> String {
        let mut out = String::new();
        // Lo que no puede representarse se señala en su posición, además del informe.
        for d in &aux.smartart {
            match self.ex.smartart_texts.get(d).filter(|t| !t.is_empty()) {
                Some(t) => out.push_str(&format!(
                    "{indent}> Diagrama SmartArt: {}\n\n",
                    escape_inline(t)
                )),
                None => out.push_str(&format!("{indent}_SmartArt sin texto extraído._\n\n")),
            }
        }
        for prog in &aux.objects {
            out.push_str(&format!(
                "{indent}_Objeto incrustado ({}) sin representación fiel; se muestra su vista previa si existe._\n\n",
                escape_inline(prog)
            ));
        }
        for _ in 0..aux.linked_images {
            out.push_str(&format!(
                "{indent}_Imagen vinculada a un archivo externo; no está incluida en el paquete._\n\n"
            ));
        }
        for img in &aux.images {
            if let Some(md) = self.image_md(ordinal, img, "imagen", "../") {
                out.push_str(&format!("{indent}{md}\n\n"));
            }
        }
        for c in &aux.charts {
            match self.ex.chart_texts.get(c).filter(|t| !t.trim().is_empty()) {
                Some(t) => {
                    mark(&mut self.tally.charts, ordinal);
                    out.push_str(&format!(
                        "{indent}> Datos del gráfico: {}\n\n",
                        escape_inline(t)
                    ));
                }
                None => {
                    mark(&mut self.tally.empty_charts, ordinal);
                    out.push_str(&format!("{indent}_Gráfico sin datos extraíbles._\n\n"));
                }
            }
        }
        for tb in &aux.textboxes {
            out.push_str(&format!("{indent}> Cuadro de texto:\n"));
            for para in tb {
                out.push_str(&format!("{indent}>\n"));
                for line in para.split('\n') {
                    out.push_str(&format!("{indent}> {line}\n"));
                }
            }
            out.push('\n');
        }
        out
    }

    fn marker(out: &mut String, spans: &mut Vec<(String, usize, usize)>, from: usize, to: usize) {
        let id = if from == to {
            format!("b-{from}")
        } else {
            format!("b-{from}-{to}")
        };
        out.push_str(&format!("<!-- okf-source-span: {id} -->\n"));
        spans.push((id, from, to));
    }

    fn cell_md(&mut self, ordinal: usize, cell: &read::Cell) -> String {
        let mut parts = Vec::new();
        for p in &cell.paras {
            let mut md = p.md().replace("\\\n", "<br>");
            if let Some(l) = &p.list {
                let label = match (l.ordinal, &l.label) {
                    (Some(n), _) => format!("{n}."),
                    (None, Some(label)) => escape_inline(label),
                    _ => "•".into(),
                };
                md = format!("{label} {md}");
            }
            if !p.flat().is_empty() {
                parts.push(md);
            }
            for img in &p.aux.images {
                if let Some(md) = self.image_md(ordinal, img, "imagen", "../") {
                    parts.push(md);
                }
            }
            for tb in &p.aux.textboxes {
                parts.push(format!("Cuadro de texto: {}", tb.join(" ")));
            }
        }
        for t in &cell.nested {
            for row in &t.rows {
                let cells: Vec<String> = row
                    .iter()
                    .map(|c| c.paras.iter().map(Para::md).collect::<Vec<_>>().join(" "))
                    .filter(|s| !s.trim().is_empty())
                    .collect();
                if !cells.is_empty() {
                    parts.push(cells.join(" · ").replace("\\\n", " "));
                }
            }
        }
        parts.join("<br>").replace('|', "\\|")
    }

    fn table_md(&mut self, ordinal: usize, t: &Table, refs: &mut Refs) -> String {
        self.tally.tables += 1;
        if t.has_merged_cells() {
            mark(&mut self.tally.merged, ordinal);
        }
        if t.nested_tables() > 0 {
            mark(&mut self.tally.nested, ordinal);
        }
        for p in t.paras() {
            self.note_aux(ordinal, &p.aux, refs);
        }
        // Una tabla de una sola celda es un recuadro: se escribe como cita.
        if let [row] = t.rows.as_slice()
            && let [cell] = row.as_slice()
            && cell.nested.is_empty()
        {
            let mut lines = Vec::new();
            for p in cell.paras.iter().filter(|p| !p.is_empty()) {
                let md = p.md();
                if !md.is_empty() {
                    lines.push(
                        md.split('\n')
                            .map(|l| format!("> {l}"))
                            .collect::<Vec<_>>()
                            .join("\n"),
                    );
                }
                let extra = self.aux_md(ordinal, &p.aux, "> ");
                if !extra.is_empty() {
                    lines.push(extra.trim_end().to_string());
                }
            }
            return if lines.is_empty() {
                String::new()
            } else {
                format!("{}\n\n", lines.join("\n>\n"))
            };
        }
        let rows: Vec<Vec<String>> = t
            .rows
            .iter()
            .map(|row| {
                let mut cells = Vec::new();
                for cell in row {
                    cells.push(if cell.merged {
                        String::new()
                    } else {
                        self.cell_md(ordinal, cell)
                    });
                    cells.extend(std::iter::repeat_n(String::new(), cell.span - 1));
                }
                cells
            })
            .collect();
        if rows.iter().flatten().all(|c| c.trim().is_empty()) {
            return String::new();
        }
        format!("{}\n", table_markdown(&rows))
    }

    /// Markdown del concepto `k` y la lista de sus localizadores de bloque.
    fn concept(&mut self, k: usize, filename: &str, generated: &str) -> String {
        let outline = self.outline;
        let ex = self.ex;
        let sec = &outline.sections[k];
        let mut body = String::new();
        let mut spans = Vec::new();
        let mut refs = Refs::default();
        if sec.inferred
            && let Some(h) = sec.heading
        {
            mark(&mut self.tally.inferred, h + 1);
        }
        if let Some(h) = sec.heading
            && let Block::Para(p) = &ex.blocks[h]
        {
            self.note_aux(h + 1, &p.aux, &mut refs);
            body.push_str(&self.aux_md(h + 1, &p.aux, ""));
        }
        let mut i = sec.from;
        while i <= sec.to && !ex.blocks.is_empty() {
            let ordinal = i + 1;
            if Some(i) == outline.title_block || Some(i) == sec.heading {
                // El título se escribe en documento.md; sus notas o comentarios, aquí.
                if Some(i) == outline.title_block
                    && let Block::Para(p) = &ex.blocks[i]
                {
                    self.note_aux(ordinal, &p.aux, &mut refs);
                }
                i += 1;
                continue;
            }
            match &ex.blocks[i] {
                Block::Para(p) if outline.headings[i].is_some() => {
                    let h = outline.headings[i].expect("encabezado");
                    if h.inferred {
                        mark(&mut self.tally.inferred, ordinal);
                    }
                    self.note_aux(ordinal, &p.aux, &mut refs);
                    body.push_str(&format!(
                        "{} {}\n\n",
                        outline.hashes(h.level),
                        escape_inline(&heading_text(p))
                    ));
                    body.push_str(&self.aux_md(ordinal, &p.aux, ""));
                    i += 1;
                }
                Block::Para(_) if self.is_toc(i) => {
                    let mut j = i;
                    while j < sec.to && self.is_toc(j + 1) {
                        j += 1;
                    }
                    Self::marker(&mut body, &mut spans, i + 1, j + 1);
                    let mut entries = 0;
                    for (k, b) in ex.blocks[i..=j].iter().enumerate() {
                        if let Block::Para(q) = b {
                            entries += usize::from(!q.flat().is_empty());
                            self.note_aux(i + k + 1, &q.aux, &mut refs);
                        }
                        mark(&mut self.tally.toc, i + k + 1);
                    }
                    body.push_str(&format!(
                        "_Tabla de contenido generada por el procesador de texto ({entries} párrafos); no se repite aquí: la estructura del documento está en [documento.md](../documento.md)._\n\n"
                    ));
                    i = j + 1;
                }
                Block::Para(p) if p.code && !p.is_empty() => {
                    let mut j = i;
                    while j < sec.to
                        && outline.headings[j + 1].is_none()
                        && matches!(&ex.blocks[j + 1], Block::Para(q) if q.code)
                    {
                        j += 1;
                    }
                    Self::marker(&mut body, &mut spans, i + 1, j + 1);
                    let mut lines = Vec::new();
                    for (k, b) in ex.blocks[i..=j].iter().enumerate() {
                        if let Block::Para(q) = b {
                            self.note_aux(i + k + 1, &q.aux, &mut refs);
                            lines.push(q.code_text());
                        }
                    }
                    let code = lines.join("\n");
                    let fence = if code.contains("```") { "~~~~" } else { "```" };
                    body.push_str(&format!("{fence}\n{code}\n{fence}\n\n"));
                    i = j + 1;
                }
                Block::Para(p) if p.list.is_some() => {
                    let mut j = i;
                    while j < sec.to
                        && outline.headings[j + 1].is_none()
                        && matches!(&ex.blocks[j + 1], Block::Para(q) if q.list.is_some())
                    {
                        j += 1;
                    }
                    Self::marker(&mut body, &mut spans, i + 1, j + 1);
                    self.tally.lists += 1;
                    // Columna de contenido de cada nivel abierto, para anidar sublistas.
                    let mut stack: Vec<(u8, usize)> = Vec::new();
                    for (k, b) in ex.blocks[i..=j].iter().enumerate() {
                        let item = i + k + 1;
                        let Block::Para(q) = b else { continue };
                        let Some(label) = &q.list else { continue };
                        self.note_aux(item, &q.aux, &mut refs);
                        while stack.last().is_some_and(|(l, _)| *l >= label.level) {
                            stack.pop();
                        }
                        let indent = stack.last().map_or(0, |(_, c)| *c);
                        let (marker, width) = match (label.ordinal, &label.label) {
                            (Some(n), _) => {
                                let m = format!("{n}. ");
                                let w = m.len();
                                (m, w)
                            }
                            (None, Some(l)) => (format!("- {} ", escape_inline(l)), 2),
                            _ => ("- ".to_string(), 2),
                        };
                        let pad = " ".repeat(indent);
                        let inner = " ".repeat(indent + width);
                        let md = q.md();
                        let mut lines = md.split('\n');
                        body.push_str(&format!("{pad}{marker}{}\n", lines.next().unwrap_or("")));
                        for line in lines {
                            body.push_str(&format!("{inner}{line}\n"));
                        }
                        let extra = self.aux_md(item, &q.aux, &inner);
                        if !extra.is_empty() {
                            body.push('\n');
                            body.push_str(&extra);
                        }
                        stack.push((label.level, indent + width));
                    }
                    body.push('\n');
                    i = j + 1;
                }
                Block::Para(p) if p.is_empty() => {
                    // Sin contenido visible, pero puede tener texto oculto, eliminado o
                    // comentarios que se declaran en el concepto.
                    self.tally.empty += 1;
                    self.tally.decorative += p.aux.images.iter().filter(|i| i.decorative).count();
                    self.note_aux(ordinal, &p.aux, &mut refs);
                    i += 1;
                }
                Block::Para(p) => {
                    self.note_aux(ordinal, &p.aux, &mut refs);
                    Self::marker(&mut body, &mut spans, ordinal, ordinal);
                    let md = p.md();
                    if !md.is_empty() {
                        body.push_str(&format!("{md}\n\n"));
                    }
                    body.push_str(&self.aux_md(ordinal, &p.aux, ""));
                    i += 1;
                }
                Block::Table(t) => {
                    let md = self.table_md(ordinal, t, &mut refs);
                    if md.is_empty() {
                        self.tally.empty += 1;
                    } else {
                        Self::marker(&mut body, &mut spans, ordinal, ordinal);
                        body.push_str(&md);
                    }
                    i += 1;
                }
            }
        }
        while body.ends_with("\n\n") {
            body.pop();
        }
        let mut aux = String::new();
        for (title, ids, order, notes, label) in [
            (
                "Notas al pie",
                &refs.footnotes,
                &ex.footnote_order,
                &ex.footnotes,
                "nota",
            ),
            (
                "Notas finales",
                &refs.endnotes,
                &ex.endnote_order,
                &ex.endnotes,
                "nota final",
            ),
        ] {
            let mut seen = HashSet::new();
            let ids: Vec<&String> = ids.iter().filter(|id| seen.insert(*id)).collect();
            if ids.is_empty() {
                continue;
            }
            aux.push_str(&format!("\n## {title}\n\n"));
            for id in ids {
                let n = order.iter().position(|x| x == id).map_or(0, |p| p + 1);
                let paras = notes.get(id).map(Vec::as_slice).unwrap_or_default();
                let text = paras
                    .iter()
                    .map(|p| p.md().replace("\\\n", " "))
                    .filter(|t| !t.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");
                aux.push_str(&format!("- \\[{label} {n}\\] {text}\n"));
                for p in paras {
                    for img in &p.aux.images {
                        if let Some(md) = self.image_md(sec.from + 1, img, "imagen", "../") {
                            aux.push_str(&format!("  {md}\n"));
                        }
                    }
                }
            }
        }
        if !refs.comments.is_empty() {
            let selected: Vec<Comment> = refs
                .comments
                .iter()
                .map(|&i| ex.comments[i].clone())
                .collect();
            aux.push('\n');
            aux.push_str(&comments_markdown(&selected));
        }
        if !refs.revisions.is_empty() {
            aux.push_str("\n## Revisiones pendientes\n\n> Cambios registrados sin aceptar en el original. El texto principal muestra la vista final: incluye las inserciones y omite las eliminaciones.\n\n");
            for (ordinal, kind, text) in &refs.revisions {
                aux.push_str(&format!(
                    "- Bloque {ordinal} · {kind}: «{}»\n",
                    snippet(text)
                ));
            }
        }
        if !refs.hidden.is_empty() {
            aux.push_str("\n## Texto oculto\n\n> Texto con formato oculto en el original; no aparece en la vista normal del documento.\n\n");
            for (ordinal, text) in &refs.hidden {
                aux.push_str(&format!("- Bloque {ordinal}: «{}»\n", snippet(text)));
            }
        }
        // Descripción: posición en el documento y primera línea con palabras.
        let first = (sec.from..=sec.to.min(ex.blocks.len().saturating_sub(1)))
            .filter(|&i| Some(i) != outline.title_block && outline.headings[i].is_none())
            .find_map(|i| match &ex.blocks[i] {
                Block::Para(p) => {
                    let t = p.flat();
                    (t.chars().filter(|c| c.is_alphabetic()).count() >= 3).then_some(t)
                }
                Block::Table(_) => None,
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
        let mut out = format!(
            "---\ntype: {}\ntitle: {}\ndescription: {}\nsources:\n  - id: original\n    resource: /references/original.docx\ngenerated:\n  by: process:document-conversion\n  at: {}\nsource_spans:\n  - {{ id: seccion, source_id: original, kind: docx_blocks, from: {}, to: {} }}\n",
            if outline.split.is_some() {
                "Source Section"
            } else {
                "Source Text"
            },
            yaml(&sec.title),
            yaml(&description),
            yaml(generated),
            sec.from + 1,
            sec.to + 1
        );
        for (id, a, b) in &spans {
            out.push_str(&format!(
                "  - {{ id: {id}, source_id: original, kind: docx_blocks, from: {a}, to: {b} }}\n"
            ));
        }
        if sec.inferred {
            out.push_str("title_inferred: true\n");
        }
        out.push_str("---\n\n");
        out.push_str(&format!("# {}\n\n", escape_inline(&sec.title)));
        if sec.inferred {
            out.push_str("> Encabezado inferido por su formato: el párrafo no tiene estilo de encabezado.\n\n");
        }
        if body.is_empty() {
            out.push_str("Sin texto extraído.\n");
        } else {
            out.push_str(&body);
        }
        out.push_str(&aux);
        out
    }
}

pub(crate) fn build_bundle(
    bytes: &[u8],
    filename: &str,
    hash: &str,
    ex: &Extraction,
    attempt: &FsPath,
) -> Result<(PathBuf, String)> {
    let started = Instant::now();
    let generated = chrono_now();
    let mut z_in = ZipArchive::new(Cursor::new(bytes))?;
    let names: HashSet<String> = z_in.file_names().map(str::to_string).collect();
    let outline = outline(ex, filename);
    let mut r = Render {
        ex,
        outline: &outline,
        assets: Assets {
            names,
            paths: HashMap::new(),
            counts: HashMap::new(),
            files: Vec::new(),
        },
        tally: Tally::default(),
    };
    let zip_path = attempt.join("bundle.zip");
    let mut z = ZipWriter::new(fs::File::create(&zip_path)?);
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
        if sec.preamble && sec.heading.is_none() {
            structure.push_str(&format!("- [Inicio](contenido/{})\n", outline.file(k)));
        }
        for i in sec.from..=sec.to {
            let Some(h) = outline.headings.get(i).copied().flatten() else {
                continue;
            };
            let Block::Para(p) = &ex.blocks[i] else {
                continue;
            };
            let text = escape_line_start(&escape_inline(&heading_text(p)));
            let suffix = if h.inferred {
                " (inferido por formato)"
            } else {
                ""
            };
            match outline.split {
                Some(l) if Some(i) == sec.heading => structure.push_str(&format!(
                    "{}- [{text}](contenido/{}){suffix}\n",
                    "  ".repeat(h.level.saturating_sub(l) as usize),
                    outline.file(k)
                )),
                Some(l) => structure.push_str(&format!(
                    "{}- {text}{suffix}\n",
                    "  ".repeat(h.level.saturating_sub(l) as usize)
                )),
                None => structure.push_str(&format!(
                    "{}- {text}{suffix}\n",
                    "  ".repeat((h.level - min_level + 1) as usize)
                )),
            }
        }
    }
    // Encabezados y pies de página, sin repetir textos ni imágenes.
    let mut furniture = String::new();
    let mut seen = HashSet::new();
    let mut furniture_images = Vec::new();
    for f in &ex.furniture {
        for p in &f.paras {
            let md = p.md().replace("\\\n", " ");
            if !md.is_empty() && seen.insert((f.kind, md.clone())) {
                r.tally.furniture_texts += 1;
                furniture.push_str(&format!("- {}: {md}\n", f.kind));
            }
            for img in &p.aux.images {
                if let Some(md) = r.image_md(0, img, "encabezado", "")
                    && !furniture_images.contains(&md)
                {
                    furniture_images.push(md);
                }
            }
        }
    }
    r.tally.furniture_images = furniture_images.len();
    let furniture_md = if furniture.is_empty() && furniture_images.is_empty() {
        String::new()
    } else {
        let images: String = furniture_images
            .iter()
            .map(|m| format!("\n{m}\n"))
            .collect();
        format!(
            "\n## Encabezados y pies de página\n\nTexto e imágenes que se repiten en las páginas; no forman parte del flujo del documento.\n\n{furniture}{images}"
        )
    };
    for (target, path) in &r.assets.files {
        let data = read_entry(&mut z_in, target)?;
        write_zip_file(&mut z, path, &data)?;
    }
    write_zip_file(&mut z, "references/original.docx", bytes)?;
    let n = ex.blocks.len();
    let tables = ex
        .blocks
        .iter()
        .filter(|b| matches!(b, Block::Table(_)))
        .count();
    let t = &r.tally;
    let mut issues = Vec::new();
    let mut grouped = |ns: &[usize], severity: Severity, text: &str| {
        if !ns.is_empty() {
            issues.push(Issue {
                severity,
                text: format!("{}{text}", units_phrase("Bloque", "Bloques", ns)),
            });
        }
    };
    grouped(&t.smartart, Severity::Loss, ": SmartArt sin texto extraído");
    grouped(
        &t.smartart_text,
        Severity::Note,
        ": SmartArt representado por el texto de sus nodos; la disposición gráfica no se conserva",
    );
    grouped(
        &t.toc,
        Severity::Note,
        ": tabla de contenido generada, resumida en una línea; repite los encabezados y sus números de página no son canónicos",
    );
    grouped(
        &t.objects,
        Severity::Loss,
        ": objeto incrustado (OLE) sin representación fiel; se adjunta su vista previa si existe",
    );
    grouped(
        &t.linked,
        Severity::Loss,
        ": imagen vinculada a un archivo externo, no incluida en el paquete",
    );
    grouped(
        &t.missing,
        Severity::Loss,
        ": imagen ausente del paquete original",
    );
    grouped(
        &t.empty_charts,
        Severity::Loss,
        ": gráfico sin datos extraíbles",
    );
    grouped(
        &t.inferred,
        Severity::Note,
        ": encabezado inferido por su formato (mismo tamaño que los encabezados con estilo); el párrafo no tiene estilo de encabezado",
    );
    grouped(
        &t.textboxes,
        Severity::Note,
        ": cuadros de texto conservados como cita junto al párrafo que los ancla",
    );
    grouped(
        &t.revisions,
        Severity::Note,
        ": revisiones pendientes (inserciones, eliminaciones, traslados o cambios de formato); el texto principal muestra la vista final y los cambios se listan en «Revisiones pendientes»",
    );
    grouped(
        &t.hidden,
        Severity::Note,
        ": texto oculto conservado en la sección «Texto oculto»",
    );
    grouped(
        &t.comments,
        Severity::Note,
        ": comentarios de revisión conservados como contenido auxiliar",
    );
    if t.equation_count > 0 {
        grouped(
            &t.equations,
            Severity::Note,
            &format!(
                ": {} ecuación(es) convertidas a notación lineal y escritas como código",
                t.equation_count
            ),
        );
    }
    grouped(
        &t.charts,
        Severity::Note,
        ": gráfico representado por sus datos en caché",
    );
    grouped(
        &t.merged,
        Severity::Note,
        ": tabla con celdas combinadas; las celdas cubiertas quedan vacías",
    );
    grouped(
        &t.nested,
        Severity::Note,
        ": tabla anidada aplanada dentro de la celda que la contiene",
    );
    grouped(
        &t.metafiles,
        Severity::Note,
        ": imagen EMF/WMF adjunta en su formato original; muchos visores no la muestran",
    );
    if t.missing_furniture > 0 {
        issues.push(Issue::loss(format!(
            "{} imagen(es) de encabezados o pies de página ausentes del paquete original",
            t.missing_furniture
        )));
    }
    for after in &ex.alt_chunks {
        issues.push(Issue::loss(match after {
            0 => "Fragmento importado (altChunk) al comienzo del documento, no representado"
                .to_string(),
            n => format!("Fragmento importado (altChunk) tras el bloque {n}, no representado"),
        }));
    }
    if t.decorative > 0 {
        issues.push(Issue::note(format!(
            "{} imagen(es) marcadas como decorativas en el original; no se adjuntan",
            t.decorative
        )));
    }
    if t.furniture_texts + t.furniture_images > 0 {
        issues.push(Issue::note(format!(
            "Encabezados y pies de página registrados en documento.md: {} texto(s) y {} imagen(es)",
            t.furniture_texts, t.furniture_images
        )));
    }
    issues.push(Issue::note(format!(
        "Título del documento tomado {}: «{}»",
        match outline.title_source {
            TitleSource::Style => "del párrafo con estilo Título",
            TitleSource::Heading =>
                "del primer encabezado, seguido directamente por otro de igual o mayor rango",
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
    let division = match outline.split {
        Some(l) => format!(
            "{} concepto(s), divididos por los encabezados de nivel {l}",
            outline.sections.len()
        ),
        None => "1 concepto (documento breve o sin secciones)".into(),
    };
    let headings = outline.headings.iter().flatten().count();
    let report = format!(
        "---\ntype: Conversion Report\ntitle: Informe de conversión\ngenerated:\n  by: process:document-conversion\n  at: {}\nstatus: {state}\nformat: docx\nsha256: {hash}\n---\n\n# Informe de conversión DOCX\n\n- Estado: `{state}`\n- Bloques inventariados: {n} ({} párrafos y {tables} tablas)\n- Bloques representados: {} (el título en documento.md, cada encabezado de división como título de su concepto); párrafos vacíos o tablas sin contenido: {}\n- Conceptos: {division}\n- Encabezados: {headings} (inferidos por formato: {})\n- Listas: {}; tablas: {}\n- Imágenes adjuntas: {}; decorativas no adjuntas: {}\n- Notas al pie: {}; notas finales: {}\n- Comentarios de revisión: {}\n- Enlaces: {}\n- Duración de ensamblado: {} ms\n\n## Incidencias\n\nPérdidas o incertidumbres sustantivas; determinan el estado `partial`.\n\n{}\n## Observaciones\n\nInformación sobre la conversión que no afecta el estado.\n\n{}",
        yaml(&generated),
        n - tables,
        n - t.empty,
        t.empty,
        t.inferred.len(),
        t.lists,
        t.tables,
        t.images,
        t.decorative,
        ex.footnote_order.len(),
        ex.endnote_order.len(),
        ex.comments
            .iter()
            .map(|c| 1 + c.replies.len())
            .sum::<usize>(),
        t.links,
        started.elapsed().as_millis(),
        list(&losses, "Sin pérdidas ni incertidumbres sustantivas."),
        list(&notes, "Sin observaciones.")
    );
    let title_span = outline
        .title_block
        .map(|b| {
            format!(
                "  - {{ id: titulo, source_id: original, kind: docx_blocks, from: {0}, to: {0} }}\n",
                b + 1
            )
        })
        .unwrap_or_default();
    let title_marker = if outline.title_block.is_some() {
        "<!-- okf-source-span: titulo -->\n"
    } else {
        ""
    };
    let summary = match outline.split {
        Some(l) => format!(
            "Documento de {n} bloques ({} párrafos y {tables} tablas), dividido en {} conceptos por sus encabezados de nivel {l}.",
            n - tables,
            outline.sections.len()
        ),
        None => format!(
            "Documento de {n} bloques ({} párrafos y {tables} tablas), representado en un solo concepto.",
            n - tables
        ),
    };
    let doc = format!(
        "---\ntype: Source Document\ntitle: {}\ndescription: {}\nsources:\n  - id: original\n    resource: /references/original.docx\ngenerated:\n  by: process:document-conversion\n  at: {}\nsource_file:\n  path: references/original.docx\n  sha256: {hash}\nsource_spans:\n  - {{ id: documento, source_id: original, kind: docx_blocks, from: 1, to: {n} }}\n{title_span}---\n\n{title_marker}# {}\n\n{summary}\n\n## Estructura\n\n{structure}{furniture_md}",
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

/// Extrae y ensambla el bundle de un documento.
pub(crate) fn convert(
    cfg: &Config,
    bytes: &[u8],
    filename: &str,
    hash: &str,
    dir: &FsPath,
) -> Result<(PathBuf, String)> {
    let ex = extract(bytes, cfg.max_blocks)?;
    if ex.blocks.is_empty() {
        return Err(Rejected("el documento no tiene bloques de contenido".into()).into());
    }
    build_bundle(bytes, filename, hash, &ex, dir)
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

/// Comprobaciones DOCX del bundle: los conceptos de contenido reparten los bloques del
/// original en tramos contiguos desde 1, cada localizador de bloque cae dentro de su
/// concepto y los índices enumeran los conceptos en orden.
pub(crate) fn validate(
    z: &mut ZipArchive<fs::File>,
    names: &HashSet<String>,
    original: &[u8],
) -> Result<()> {
    let total = block_count(original)?;
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
        (s.get("kind").and_then(YamlValue::as_str) == Some("docx_blocks")).then_some(())?;
        let a = s.get("from").and_then(YamlValue::as_u64)?;
        let b = s.get("to").and_then(YamlValue::as_u64)?;
        (a >= 1 && a <= b).then_some((a, b))
    };
    let mut next = 1u64;
    for name in &expected {
        let fm = frontmatter(&read_text(z, name)?)?;
        let spans = fm
            .get("source_spans")
            .and_then(YamlValue::as_sequence)
            .with_context(|| format!("{name} carece de source_spans"))?;
        let section = spans
            .iter()
            .find(|s| s.get("id").and_then(YamlValue::as_str) == Some("seccion"))
            .and_then(span_range)
            .with_context(|| {
                format!("{name} carece de un localizador docx_blocks «seccion» válido")
            })?;
        if section.0 != next {
            bail!(
                "{name} empieza en el bloque {} y debía empezar en el {next}",
                section.0
            )
        }
        for s in spans {
            match span_range(s) {
                Some((a, b)) if a >= section.0 && b <= section.1 => {}
                _ => bail!("{name} tiene un localizador de bloque fuera de su sección"),
            }
        }
        next = section.1 + 1;
    }
    if next != total as u64 + 1 {
        bail!(
            "los conceptos cubren los bloques 1–{} y el original tiene {total}",
            next - 1
        )
    }
    let doc = frontmatter(&read_text(z, "documento.md")?)?;
    let whole = doc
        .get("source_spans")
        .and_then(YamlValue::as_sequence)
        .and_then(|s| {
            s.iter()
                .find(|s| s.get("id").and_then(YamlValue::as_str) == Some("documento"))
        })
        .and_then(span_range);
    if whole != Some((1, total as u64)) {
        bail!("documento.md no localiza los {total} bloques del original")
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

    /// Convierte un DOCX y devuelve el contenido de cada entrada Markdown del bundle.
    fn convert_example(path: &str) -> (Extraction, String, HashMap<String, String>) {
        let bytes = fs::read(path).unwrap_or_else(|e| panic!("no se pudo leer {path}: {e}"));
        let ex = extract(&bytes, 50_000).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let name = FsPath::new(path)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let (zip, state) = build_bundle(
            &bytes,
            &name,
            &hex(&Sha256::digest(&bytes)),
            &ex,
            tmp.path(),
        )
        .unwrap();
        let mut z = ZipArchive::new(fs::File::open(&zip).unwrap()).unwrap();
        let mut md = HashMap::new();
        for i in 0..z.len() {
            let name = z.by_index(i).unwrap().name().to_string();
            if name.ends_with(".md") {
                md.insert(name.clone(), read_text(&mut z, &name).unwrap());
            }
        }
        (ex, state, md)
    }

    fn corpus(name: &str) -> (Extraction, String, HashMap<String, String>) {
        let dir =
            std::env::var("OKF_DOCX_CORPUS_DIR").unwrap_or_else(|_| "examples/docx/corpus".into());
        convert_example(&format!("{dir}/{name}.docx"))
    }

    fn titles(md: &HashMap<String, String>) -> Vec<String> {
        let mut names: Vec<&String> = md
            .keys()
            .filter(|k| k.starts_with("contenido/seccion-"))
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

    #[test]
    fn corpus_d1_splits_weeks_and_keeps_bold_labels_and_nested_lists() {
        let (ex, state, md) = corpus("d1");
        assert_eq!(state, "complete");
        assert_eq!(ex.blocks.len(), 106);
        let o = outline(&ex, "d1.docx");
        assert_eq!(o.title, "MISW4411 - Temario 2026");
        assert_eq!(o.title_source, TitleSource::Heading);
        let t = titles(&md);
        assert_eq!(t.len(), 8);
        assert!(t[0].starts_with("Semana 1: Fundamentos"));
        assert_eq!(t[7], "Semana 8: Varios");
        let s5 = &md["contenido/seccion-005.md"];
        // Las etiquetas en negrita se conservan como negrita, no como encabezados.
        assert!(s5.contains("**Técnicas de Re-ranking**"), "{s5}");
        assert!(s5.contains("- Parent-Child Document Retrieval\n  - Indexar chunks pequeños"));
        assert!(s5.contains("<!-- okf-source-span: b-52-58 -->"));
        // El título del documento queda en documento.md, localizado en el bloque 1.
        let doc = &md["documento.md"];
        assert!(doc.contains("<!-- okf-source-span: titulo -->\n# MISW4411 - Temario 2026"));
        assert!(
            doc.contains("{ id: titulo, source_id: original, kind: docx_blocks, from: 1, to: 1 }")
        );
    }

    #[test]
    fn corpus_d2_infers_direct_formatted_heading_and_records_page_furniture() {
        let (ex, state, md) = corpus("d2");
        assert_eq!(state, "complete");
        let o = outline(&ex, "d2.docx");
        assert_eq!(o.title_source, TitleSource::Style);
        assert!(o.title.starts_with("Tarea No. 2 - Evaluación estratégica"));
        let t = titles(&md);
        assert_eq!(t.len(), 7, "{t:?}");
        assert!(t[3].starts_with("Paso No. 2 – Evaluación comparativa"));
        let paso2 = &md["contenido/seccion-004.md"];
        assert!(paso2.contains("title_inferred: true"));
        let report = &md["conversion-report.md"];
        assert!(
            report.contains("Bloque 24: encabezado inferido por su formato"),
            "{report}"
        );
        let doc = &md["documento.md"];
        assert!(
            doc.contains("- Pie de página: *Universidad de los Andes*"),
            "{doc}"
        );
        assert!(doc.contains("](assets/encabezado-001.png)"));
        let entregables = &md["contenido/seccion-007.md"];
        assert!(entregables.contains("## Video de presentación"));
        assert!(
            entregables
                .contains("<!-- okf-source-span: b-45-52 -->\n- Primero, una presentación breve")
        );
        assert!(entregables.contains("- Octavo, una mención breve"));
    }

    #[test]
    fn corpus_d3_detects_title_cover_figures_links_and_field_results() {
        let (ex, state, md) = corpus("d3");
        assert_eq!(state, "complete");
        let o = outline(&ex, "d3.docx");
        assert_eq!(o.title, "Harness en agentes de inteligencia artificial");
        let t = titles(&md);
        assert_eq!(t[0], "Harness en agentes de inteligencia artificial");
        assert_eq!(
            t[1],
            "Introducción - ¿Por qué necesitamos el concepto de harness?"
        );
        assert_eq!(t.last().map(String::as_str), Some("Referencias"));
        let all: String = md.values().cloned().collect();
        // La instrucción INCLUDEPICTURE no es texto; su resultado (la imagen) sí.
        assert!(!all.contains("INCLUDEPICTURE"));
        assert_eq!(all.matches("](../assets/imagen-").count(), 5);
        assert!(md["contenido/seccion-001.md"].contains("![Imagen 1](../assets/imagen-001.png)"));
        let arch = md
            .values()
            .find(|m| m.contains("# Arquitectura interna del harness"))
            .unwrap();
        assert!(arch.contains("## 1. Orquestador y bucle de ejecución"));
        assert!(md["documento.md"].contains("  - 1\\. Orquestador y bucle de ejecución"));
        let refs = md.values().find(|m| m.contains("# Referencias")).unwrap();
        assert!(
            refs.contains("](https://arxiv.org/abs/2602.22480)"),
            "{refs}"
        );
        assert!(all.contains("***"));
    }

    #[test]
    fn synthetic_docx_keeps_auxiliary_content_and_structure() {
        let (ex, state, md) = convert_example("examples/docx/estructura.docx");
        assert_eq!(state, "complete", "{}", md["conversion-report.md"]);
        let o = outline(&ex, "estructura.docx");
        assert_eq!(o.title, "Informe de prueba");
        let all: String = md.values().cloned().collect();
        let s1 = &md["contenido/seccion-001.md"];
        assert!(s1.contains("**negrita** y *cursiva*"), "{s1}");
        assert!(s1.contains("[enlace](https://example.com/okf)"));
        assert!(s1.contains("\\[nota 1\\]"));
        assert!(s1.contains("## Notas al pie\n\n- \\[nota 1\\] Fuente de la cifra."));
        assert!(s1.contains("`E=mc^2`"));
        assert!(
            s1.contains(
                "1. Primer paso\n2. Segundo paso\n   - Detalle del segundo paso\n3. Tercer paso"
            ),
            "{s1}"
        );
        let s2 = &md["contenido/seccion-002.md"];
        assert!(s2.contains("| Región | Ventas |  |"), "{s2}");
        assert!(s2.contains("![Diagrama del flujo](../assets/imagen-001.png)"));
        assert!(s2.contains("> Cuadro de texto:\n>\n> Texto del recuadro"));
        assert_eq!(all.matches("Texto del recuadro").count(), 1);
        assert!(s2.contains("## Comentarios de revisión"));
        assert!(s2.contains("**Revisora** · 2026-10-30: Confirmar la cifra (sobre «420»)"));
        // Vista final: la inserción está en el texto y la eliminación solo en las revisiones.
        assert!(s2.contains("Cifra final de 420 unidades.\n"), "{s2}");
        assert!(s2.contains("- Bloque 12 · inserción: «420»\n- Bloque 12 · eliminación: «380»"));
        assert!(s2.contains("- Bloque 12: «Nota interna para el equipo.»"));
        assert!(s2.contains("## Texto oculto\n\n> Texto con formato oculto"));
        assert!(!all.contains("PAGE"));
        // Un campo de paginación deja un marcador, no un número de página concreto.
        assert!(s2.contains("<!-- okf-source-span: b-13 -->\nPágina \\[página\\]\n"));
        let report = &md["conversion-report.md"];
        assert!(report.contains("Bloque 3: 1 ecuación(es)"), "{report}");
        assert!(report.contains("Bloque 10: tabla con celdas combinadas"));
        assert!(report.contains("Bloque 11: cuadros de texto"));
        assert!(md["documento.md"].contains("- Encabezado: Informe interno"));
    }

    #[test]
    fn brief_docx_is_a_single_text_concept() {
        let (_, state, md) = convert_example("examples/docx/breve.docx");
        assert_eq!(state, "complete");
        assert!(md.contains_key("contenido/texto.md"));
        assert!(!md.keys().any(|k| k.starts_with("contenido/seccion-")));
        let t = &md["contenido/texto.md"];
        assert!(t.contains("type: Source Text"));
        assert!(t.contains("- Primera idea\n- Segunda idea"), "{t}");
    }

    #[test]
    fn reviewed_report_keeps_revisions_comments_notes_and_collapses_the_toc() {
        let (ex, state, md) = convert_example("examples/docx/muestras/informe_revisado.docx");
        assert_eq!(state, "complete");
        assert_eq!(
            outline(&ex, "informe_revisado.docx").title,
            "Informe de evaluación del asistente RAG"
        );
        assert_eq!(
            titles(&md),
            [
                "Informe de evaluación del asistente RAG",
                "1. Introducción",
                "2. Resultados",
                "3. Conclusiones"
            ]
        );
        // La tabla de contenido se resume y sus entradas no se repiten.
        let inicio = &md["contenido/seccion-001.md"];
        assert!(inicio.contains("<!-- okf-source-span: b-3-7 -->\n_Tabla de contenido generada"));
        assert!(!inicio.contains("Alcance"));
        let intro = &md["contenido/seccion-002.md"];
        assert!(intro.contains("## 1.1. Alcance de la evaluación"));
        assert!(intro.contains("con cuarenta y cinco documentos"));
        assert!(intro.contains("- Bloque 12 · eliminación: «cuarenta y dos»"));
        assert!(intro.contains(
            "- \\[nota 1\\] Las preguntas se anonimizaron antes del análisis. El protocolo"
        ));
        assert!(intro.contains("- \\[nota final 1\\] Foro del curso MISW4411"));
        assert!(intro.contains("- Bloque 14: «Nota interna: repetir el muestreo"));
        assert!(intro.contains("(sobre «La muestra excluye preguntas en inglés»)"));
        let res = &md["contenido/seccion-003.md"];
        assert!(res.contains("|  |  | **Normativo** | **Técnico** |"));
        assert!(res.contains("Exhaustividad del contexto\\[nota 2\\]"));
        assert!(res.contains("Verificación por fragmentos:<br>**Paso** · **Responsable**"));
        assert!(res.contains("  - **Laura Gómez** · 2026-10-28: Añadida en la nota del anexo"));
        assert!(res.contains("- Bloque 21 · traslado (origen): «La latencia media"));
        assert!(res.contains("- Bloque 22 · traslado (destino): «La latencia media"));
        assert!(res.contains("- Bloque 23 · cambio de formato: «Hallazgo principal:»"));
        // El texto trasladado aparece una vez en el cuerpo y dos en las revisiones.
        assert_eq!(
            res.matches("La latencia media fue de 2,4 segundos.")
                .count(),
            3
        );
        let doc = &md["documento.md"];
        assert!(doc.contains("- Pie de página: Página \\[página\\] de \\[total de páginas\\]"));
        assert!(doc.contains("![Logotipo del programa de maestría](assets/encabezado-001.png)"));
        let report = &md["conversion-report.md"];
        assert!(report.contains("Bloques 3–7: tabla de contenido generada"));
        assert!(report.contains("Notas al pie: 2; notas finales: 2"));
        assert!(report.contains("Comentarios de revisión: 3"));
        assert!(report.contains("(inferidos por formato: 0)"));
    }

    #[test]
    fn technical_guide_keeps_equations_figures_chart_lists_code_and_links() {
        let (_, state, md) = convert_example("examples/docx/muestras/guia_tecnica.docx");
        assert_eq!(state, "complete");
        let f = &md["contenido/seccion-002.md"];
        assert!(
            f.contains("`cos(q,d)=(∑_(i=1)^n q_i d_i)/(√(∑_(i=1)^n (q_i)^2)√(∑_(i=1)^n (d_i)^2))`")
        );
        assert!(f.contains("`F_1=(2PR)/(P+R)`"));
        assert!(f.contains("> - Use el mismo modelo para la consulta y los documentos."));
        assert!(f.contains("![Flujo de indexación: documentos, fragmentación, embeddings e índice vectorial](../assets/imagen-001.png)"));
        assert!(f.contains("*Figura 1. Flujo de indexación.*"));
        // La imagen decorativa no se adjunta ni deja un marcador vacío.
        assert!(!md.values().any(|m| m.contains("imagen-002")));
        assert!(!f.contains("b-11 -->"));
        let c = &md["contenido/seccion-003.md"];
        assert!(c.contains("1. Prepare el corpus.\n   - a) Elimine duplicados.\n   - b) Normalice la codificación a UTF-8.\n     - i. Verifique los metadatos obligatorios.\n2. Fragmente los documentos."), "{c}");
        assert!(c.contains("3. Calcule los embeddings.\n4. Cree el índice"));
        assert!(c.contains("1. Ejecute las consultas de prueba.\n2. Compare"));
        assert!(c.contains("> Datos del gráfico: Precisión por tamaño de fragmento. Precisión: 256 tokens = 0.71; 512 tokens = 0.82; 1024 tokens = 0.76"));
        assert!(c.contains(
            "```\ncoleccion = cliente.create_collection(\n    name=\"laboratorio3\", metric=\"cosine\")\n```"
        ));
        assert!(c.contains("> **Importante:** los índices"));
        let r = &md["contenido/seccion-004.md"];
        assert!(
            r.contains("[el sitio del proveedor](https://example.org/docs/indices-vectoriales)")
        );
        assert!(r.contains("[enunciado del laboratorio](https://example.org/okf/laboratorio3)"));
        // Un enlace a un marcador interno queda como texto.
        assert!(r.contains("Véase también la Figura 1 y"));
        assert!(r.contains("Responsable del laboratorio: Carolina Pérez"));
        assert!(
            !md.values()
                .any(|m| m.contains("SEQ") || m.contains("_Ref1"))
        );
        let report = &md["conversion-report.md"];
        assert!(report.contains("1 imagen(es) marcadas como decorativas"));
        assert!(report.contains("Bloques 6 y 7: 2 ecuación(es)"));
    }

    #[test]
    fn unrepresentable_content_is_declared_in_place_and_in_the_report() {
        let (_, state, md) = convert_example("examples/docx/muestras/anexos_con_perdidas.docx");
        assert_eq!(state, "partial");
        let a = &md["contenido/seccion-001.md"];
        assert!(a.contains("> Diagrama SmartArt: Preguntas · Recuperación · Generación · Juicio"));
        assert!(a.contains("_Objeto incrustado (Excel.Sheet.12) sin representación fiel"));
        let b = &md["contenido/seccion-002.md"];
        assert!(b.contains("_Imagen vinculada a un archivo externo"));
        assert!(b.contains("_Gráfico sin datos extraíbles._"));
        assert!(b.contains("_Imagen ausente del paquete original._"));
        let report = &md["conversion-report.md"];
        let (losses, notes) = report.split_once("## Observaciones").unwrap();
        for loss in [
            "Bloque 7: objeto incrustado (OLE)",
            "Bloque 11: imagen vinculada",
            "Bloque 13: gráfico sin datos",
            "Bloque 15: imagen ausente",
            "Fragmento importado (altChunk) tras el bloque 11",
        ] {
            assert!(losses.contains(loss), "{loss}: {losses}");
        }
        assert!(notes.contains("Bloque 5: SmartArt representado por el texto de sus nodos"));
    }
}
