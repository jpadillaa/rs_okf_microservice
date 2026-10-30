//! Lectura de una parte PresentationML (diapositiva o notas) como bloques ordenados.
//!
//! El orden de los bloques es el del árbol de formas (orden de autoría). La geometría se
//! conserva en EMU absolutas para que el ensamblado infiera títulos y filtre recursos.
use quick_xml::events::{BytesStart, Event};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: i64,
    pub y: i64,
    pub cx: i64,
    pub cy: i64,
}

#[derive(Clone, Debug, Default)]
pub struct Paragraph {
    pub level: u32,
    /// Texto literal; los saltos de línea internos se representan con `\n`.
    pub plain: String,
    /// Markdown escapado con enlaces resueltos; mismas líneas que `plain`.
    pub md: String,
}

#[derive(Clone, Debug, Default)]
pub struct TextShape {
    pub rect: Option<Rect>,
    pub placeholder: Option<String>,
    pub max_size: Option<u32>,
    pub paragraphs: Vec<Paragraph>,
}

impl TextShape {
    /// Texto del bloque en una sola línea, con espacios normalizados.
    pub fn flat_text(&self) -> String {
        self.paragraphs
            .iter()
            .flat_map(|p| p.plain.split_whitespace())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[derive(Clone, Debug, Default)]
pub struct Picture {
    pub rect: Option<Rect>,
    /// Relación preferida: SVG cuando existe, si no el mapa de bits.
    pub rid: String,
    pub descr: Option<String>,
}

#[derive(Clone, Debug)]
pub enum Block {
    Text(TextShape),
    Picture(Picture),
    /// Filas de celdas ya escapadas para una tabla Markdown.
    Table(Vec<Vec<String>>),
    Chart(String),
}

#[derive(Debug, Default)]
pub struct Part {
    pub blocks: Vec<Block>,
    pub background_images: usize,
    pub smartart_frames: usize,
}

#[derive(Clone, Copy, Default)]
struct GroupXf {
    off: (i64, i64),
    ext: (i64, i64),
    ch_off: (i64, i64),
    ch_ext: (i64, i64),
}

impl GroupXf {
    fn map(&self, r: Rect) -> Rect {
        let sx = |v: i64| {
            if self.ch_ext.0 == 0 {
                v
            } else {
                (v as i128 * self.ext.0 as i128 / self.ch_ext.0 as i128) as i64
            }
        };
        let sy = |v: i64| {
            if self.ch_ext.1 == 0 {
                v
            } else {
                (v as i128 * self.ext.1 as i128 / self.ch_ext.1 as i128) as i64
            }
        };
        Rect {
            x: self.off.0 + sx(r.x - self.ch_off.0),
            y: self.off.1 + sy(r.y - self.ch_off.1),
            cx: sx(r.cx),
            cy: sy(r.cy),
        }
    }
}

#[derive(Default)]
struct ParaBuilder {
    level: u32,
    plain: String,
    md: String,
    run_text: String,
    run_link: Option<String>,
}

impl ParaBuilder {
    fn flush_run(&mut self) {
        if self.run_text.is_empty() {
            return;
        }
        let text = normalize_breaks(&std::mem::take(&mut self.run_text));
        self.plain.push_str(&text);
        let escaped = escape_inline(&text);
        match self.run_link.take() {
            Some(dest) if !text.trim().is_empty() => {
                // Un salto dentro del texto enlazado rompería el enlace Markdown.
                let label = escaped.replace('\n', " ");
                let dest = if dest.contains([' ', '(', ')']) {
                    format!("<{dest}>")
                } else {
                    dest
                };
                self.md.push_str(&format!("[{label}]({dest})"));
            }
            _ => self.md.push_str(&escaped),
        }
    }
    fn finish(mut self) -> Option<Paragraph> {
        self.flush_run();
        if self.plain.trim().is_empty() {
            return None;
        }
        let md = self
            .md
            .split('\n')
            .map(|l| escape_line_start(l.trim_end()))
            .collect::<Vec<_>>()
            .join("\n");
        Some(Paragraph {
            level: self.level,
            plain: self.plain.trim_end().to_string(),
            md: md.trim().to_string(),
        })
    }
}

#[derive(Default)]
enum Owner {
    #[default]
    None,
    Shape,
    Picture,
    Frame,
}

/// Lee una diapositiva o página de notas. `link` traduce el `r:id` de un hipervínculo
/// a un destino Markdown; devuelve `None` si el destino no puede representarse.
pub fn parse_part(xml: &str, link: &dyn Fn(&str) -> Option<String>) -> anyhow::Result<Part> {
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut part = Part::default();
    let mut stack: Vec<Vec<u8>> = Vec::new();
    let mut groups: Vec<GroupXf> = Vec::new();
    let mut owner = Owner::None;
    let mut rect = Rect::default();
    let mut has_rect = false;
    let mut shape = TextShape::default();
    let mut shape_link: Option<String> = None;
    let mut picture = Picture::default();
    let mut blip_rid: Option<String> = None;
    let mut svg_rid: Option<String> = None;
    let mut frame_chart: Option<String> = None;
    let mut frame_smartart = false;
    let mut table: Option<Vec<Vec<String>>> = None;
    let mut cell: Option<Vec<String>> = None;
    let mut para: Option<ParaBuilder> = None;
    let mut in_t = false;
    let mut in_bg = false;
    loop {
        let event = reader.read_event()?;
        let (start, is_empty) = match &event {
            Event::Start(e) => (Some(e.clone()), false),
            Event::Empty(e) => (Some(e.clone()), true),
            _ => (None, false),
        };
        if let Some(e) = start {
            let name = e.local_name().as_ref().to_vec();
            let parent = stack.last().map(Vec::as_slice);
            let grandparent = stack.len().checked_sub(2).map(|i| stack[i].as_slice());
            match name.as_slice() {
                b"bg" => in_bg = true,
                b"grpSp" => groups.push(GroupXf::default()),
                b"sp" | b"cxnSp" if !in_bg => {
                    owner = Owner::Shape;
                    shape = TextShape::default();
                    shape_link = None;
                    has_rect = false;
                    blip_rid = None;
                    svg_rid = None;
                }
                b"pic" if !in_bg => {
                    owner = Owner::Picture;
                    picture = Picture::default();
                    has_rect = false;
                    blip_rid = None;
                    svg_rid = None;
                }
                b"graphicFrame" => {
                    owner = Owner::Frame;
                    has_rect = false;
                    frame_chart = None;
                    frame_smartart = false;
                }
                b"cNvPr" => {
                    if matches!(owner, Owner::Picture) {
                        picture.descr = attr(&e, b"descr").filter(|d| !d.trim().is_empty());
                    }
                }
                b"hlinkClick" => {
                    let target = rel_id(&e).and_then(|rid| link(&rid));
                    match parent {
                        Some(b"cNvPr") => shape_link = target,
                        Some(b"rPr") => {
                            if let Some(p) = para.as_mut() {
                                p.run_link = target
                            }
                        }
                        _ => {}
                    }
                }
                b"ph" if matches!(owner, Owner::Shape) => {
                    shape.placeholder = Some(attr(&e, b"type").unwrap_or_else(|| "body".into()));
                }
                b"off" | b"ext" | b"chOff" | b"chExt" if matches!(parent, Some(b"xfrm")) => {
                    let (a, b) = match name.as_slice() {
                        b"off" | b"chOff" => (b"x".as_slice(), b"y".as_slice()),
                        _ => (b"cx".as_slice(), b"cy".as_slice()),
                    };
                    let value = (num(&e, a), num(&e, b));
                    if matches!(grandparent, Some(b"grpSpPr")) {
                        if let Some(g) = groups.last_mut() {
                            match name.as_slice() {
                                b"off" => g.off = value,
                                b"ext" => g.ext = value,
                                b"chOff" => g.ch_off = value,
                                _ => g.ch_ext = value,
                            }
                        }
                    } else if matches!(grandparent, Some(b"spPr") | Some(b"graphicFrame")) {
                        has_rect = true;
                        match name.as_slice() {
                            b"off" => (rect.x, rect.y) = value,
                            b"ext" => (rect.cx, rect.cy) = value,
                            _ => {}
                        }
                    }
                }
                b"blip" => {
                    if in_bg {
                        part.background_images += 1;
                    } else {
                        blip_rid = embed(&e);
                    }
                }
                b"svgBlip" if !in_bg => svg_rid = embed(&e),
                b"graphicData" => {
                    if attr(&e, b"uri").is_some_and(|u| u.contains("/diagram")) {
                        frame_smartart = true;
                    }
                }
                b"chart" if matches!(owner, Owner::Frame) => frame_chart = rel_id(&e),
                b"tbl" => table = Some(Vec::new()),
                b"tr" => {
                    if let Some(t) = table.as_mut() {
                        t.push(Vec::new())
                    }
                }
                b"tc" => cell = Some(Vec::new()),
                b"p" if matches!(parent, Some(b"txBody")) => {
                    para = Some(ParaBuilder::default());
                }
                b"pPr" => {
                    if let Some(p) = para.as_mut() {
                        p.level = attr(&e, b"lvl").and_then(|v| v.parse().ok()).unwrap_or(0);
                    }
                }
                b"r" | b"fld" => {
                    if let Some(p) = para.as_mut() {
                        p.flush_run();
                        p.run_link = shape_link.clone();
                    }
                }
                b"rPr" => {
                    if let Some(size) = attr(&e, b"sz").and_then(|v| v.parse::<u32>().ok())
                        && cell.is_none()
                        && matches!(owner, Owner::Shape)
                    {
                        shape.max_size = Some(shape.max_size.map_or(size, |m| m.max(size)));
                    }
                }
                b"br" => {
                    if let Some(p) = para.as_mut() {
                        p.flush_run();
                        p.plain.push('\n');
                        p.md.push('\n');
                    }
                }
                b"t" if !is_empty => in_t = true,
                _ => {}
            }
            if !is_empty {
                stack.push(name);
            }
            continue;
        }
        match event {
            Event::Text(t) if in_t => {
                if let Some(p) = para.as_mut() {
                    p.run_text.push_str(&t.decode()?);
                }
            }
            Event::GeneralRef(r) if in_t => {
                if let Some(p) = para.as_mut() {
                    let resolved = match r.resolve_char_ref()? {
                        Some(c) => c.to_string(),
                        None => {
                            let name = r.decode()?;
                            quick_xml::escape::resolve_predefined_entity(&name)
                                .map(str::to_string)
                                .unwrap_or_else(|| format!("&{name};"))
                        }
                    };
                    p.run_text.push_str(&resolved);
                }
            }
            Event::CData(t) if in_t => {
                if let Some(p) = para.as_mut() {
                    p.run_text.push_str(&t.decode()?);
                }
            }
            Event::End(e) => {
                let name = e.local_name().as_ref().to_vec();
                stack.pop();
                let absolute =
                    |groups: &[GroupXf], r: Rect| groups.iter().rev().fold(r, |acc, g| g.map(acc));
                match name.as_slice() {
                    b"t" => in_t = false,
                    b"bg" => in_bg = false,
                    b"grpSp" => {
                        groups.pop();
                    }
                    b"p" => {
                        if let Some(p) = para.take().and_then(ParaBuilder::finish) {
                            if let Some(c) = cell.as_mut() {
                                c.push(p.md.replace('\n', "<br>"));
                            } else if matches!(owner, Owner::Shape) {
                                shape.paragraphs.push(p);
                            }
                        }
                    }
                    b"tc" => {
                        if let (Some(c), Some(t)) = (cell.take(), table.as_mut())
                            && let Some(row) = t.last_mut()
                        {
                            row.push(c.join("<br>").replace('|', "\\|"));
                        }
                    }
                    b"tbl" => {
                        if let Some(t) = table.take().filter(|t| !t.is_empty()) {
                            part.blocks.push(Block::Table(t));
                        }
                    }
                    b"sp" | b"cxnSp" if matches!(owner, Owner::Shape) => {
                        owner = Owner::None;
                        shape.rect = has_rect.then(|| absolute(&groups, rect));
                        let fill = svg_rid.take().or(blip_rid.take());
                        let text = std::mem::take(&mut shape);
                        if let Some(rid) = fill {
                            // Imagen usada como relleno de una forma.
                            part.blocks.push(Block::Picture(Picture {
                                rect: text.rect,
                                rid,
                                descr: None,
                            }));
                        }
                        if !text.paragraphs.is_empty() {
                            part.blocks.push(Block::Text(text));
                        }
                    }
                    b"pic" if matches!(owner, Owner::Picture) => {
                        owner = Owner::None;
                        if let Some(rid) = svg_rid.take().or(blip_rid.take()) {
                            let mut p = std::mem::take(&mut picture);
                            p.rect = has_rect.then(|| absolute(&groups, rect));
                            p.rid = rid;
                            part.blocks.push(Block::Picture(p));
                        }
                    }
                    b"graphicFrame" => {
                        owner = Owner::None;
                        if let Some(rid) = frame_chart.take() {
                            part.blocks.push(Block::Chart(rid));
                        }
                        if frame_smartart {
                            part.smartart_frames += 1;
                        }
                    }
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(part)
}

fn attr(e: &BytesStart<'_>, name: &[u8]) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.local_name().as_ref() == name)
        .and_then(|a| a.unescape_value().ok().map(|v| v.into_owned()))
}
fn num(e: &BytesStart<'_>, name: &[u8]) -> i64 {
    attr(e, name).and_then(|v| v.parse().ok()).unwrap_or(0)
}
/// Atributo `r:id` (relación del paquete), distinto del `id` sin prefijo.
fn rel_id(e: &BytesStart<'_>) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.as_ref().ends_with(b":id"))
        .and_then(|a| a.unescape_value().ok().map(|v| v.into_owned()))
}
fn embed(e: &BytesStart<'_>) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.as_ref().ends_with(b":embed"))
        .and_then(|a| a.unescape_value().ok().map(|v| v.into_owned()))
        .filter(|v| !v.is_empty())
}
/// PowerPoint guarda saltos suaves como `\r\n`, `\r` o tabulación vertical.
fn normalize_breaks(s: &str) -> String {
    s.replace("\r\n", "\n").replace(['\r', '\u{b}'], "\n")
}

/// Escapa los caracteres con significado en línea para Markdown/CommonMark.
pub fn escape_inline(s: &str) -> String {
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
pub fn escape_line_start(line: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    const NS: &str = r#"xmlns:p="p" xmlns:a="a" xmlns:r="r""#;

    fn texts(part: &Part) -> Vec<String> {
        part.blocks
            .iter()
            .filter_map(|b| match b {
                Block::Text(t) => Some(
                    t.paragraphs
                        .iter()
                        .map(|p| p.md.clone())
                        .collect::<Vec<_>>()
                        .join(" / "),
                ),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn runs_join_without_spaces_and_entities_are_kept() {
        let xml = format!(
            r#"<p:sld {NS}><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>Chunk Size &amp; Over</a:t></a:r><a:r><a:t>lap &lt;x&gt; &#8364;</a:t></a:r></a:p><a:p><a:pPr lvl="1"/><a:r><a:t>uno</a:t></a:r><a:br/><a:r><a:t>dos</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#
        );
        let part = parse_part(&xml, &|_| None).unwrap();
        let Block::Text(shape) = &part.blocks[0] else {
            panic!()
        };
        assert_eq!(shape.paragraphs[0].plain, "Chunk Size & Overlap <x> €");
        assert_eq!(shape.paragraphs[0].md, "Chunk Size & Overlap \\<x> €");
        assert_eq!(shape.paragraphs[1].level, 1);
        assert_eq!(shape.paragraphs[1].plain, "uno\ndos");
    }

    #[test]
    fn hyperlinks_group_transforms_and_svg_pictures() {
        let xml = format!(
            r#"<p:sld {NS}><p:cSld><p:spTree><p:grpSp><p:grpSpPr><a:xfrm><a:off x="1000" y="2000"/><a:ext cx="200" cy="200"/><a:chOff x="0" y="0"/><a:chExt cx="100" cy="100"/></a:xfrm></p:grpSpPr>
            <p:sp><p:nvSpPr><p:cNvPr id="2" name="t"/><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="10" y="20"/><a:ext cx="30" cy="40"/></a:xfrm></p:spPr>
            <p:txBody><a:p><a:r><a:rPr sz="4000"><a:hlinkClick r:id="rId9"/></a:rPr><a:t>sitio</a:t></a:r><a:r><a:t> [nota]</a:t></a:r></a:p></p:txBody></p:sp></p:grpSp>
            <p:pic><p:nvPicPr><p:cNvPr id="3" name="i" descr="preencoded.png"/></p:nvPicPr><p:blipFill><a:blip r:embed="rIdPng"><a:extLst><a:ext uri="x"><asvg:svgBlip xmlns:asvg="s" r:embed="rIdSvg"/></a:ext></a:extLst></a:blip></p:blipFill><p:spPr><a:xfrm><a:off x="5" y="6"/><a:ext cx="7" cy="8"/></a:xfrm></p:spPr></p:pic>
            </p:spTree></p:cSld></p:sld>"#
        );
        let part = parse_part(&xml, &|rid| {
            (rid == "rId9").then(|| "https://example.org/a".into())
        })
        .unwrap();
        let Block::Text(shape) = &part.blocks[0] else {
            panic!()
        };
        assert_eq!(shape.placeholder.as_deref(), Some("title"));
        assert_eq!(shape.max_size, Some(4000));
        assert_eq!(
            shape.rect,
            Some(Rect {
                x: 1020,
                y: 2040,
                cx: 60,
                cy: 80
            })
        );
        assert_eq!(
            shape.paragraphs[0].md,
            "[sitio](https://example.org/a) \\[nota\\]"
        );
        let Block::Picture(pic) = &part.blocks[1] else {
            panic!()
        };
        assert_eq!(pic.rid, "rIdSvg");
        assert_eq!(
            pic.rect,
            Some(Rect {
                x: 5,
                y: 6,
                cx: 7,
                cy: 8
            })
        );
        assert_eq!(texts(&part).len(), 1);
    }

    #[test]
    fn line_starts_that_look_like_markdown_are_escaped() {
        assert_eq!(escape_line_start("+ consulta"), "\\+ consulta");
        assert_eq!(escape_line_start("1. La ventana"), "1\\. La ventana");
        assert_eq!(escape_line_start("# 3"), "\\# 3");
        assert_eq!(escape_line_start("Normal"), "Normal");
    }
}
