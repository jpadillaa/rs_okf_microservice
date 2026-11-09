//! Lectura de PDF con pdf_oxide: caracteres con posición y fuente, imágenes con su
//! rectángulo, esquema, metadatos, etiquetas de página internas y vistas renderizadas.
//! Las coordenadas pasan a puntos con origen en la esquina superior izquierda de la
//! página.
use crate::worker::Rejected;
use anyhow::{Context, Result};
use pdf_oxide::{
    Destination, OutlineItem, PdfDocument,
    extractors::images::{ColorSpace, PdfFilter},
    layout::TextChar,
    object::Object,
    rendering::{RenderOptions, render_page},
};
use sha2::{Digest, Sha256};

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

/// Destino de una anotación de enlace: URL, página (desde 1) o referencia sin destino.
/// Los demás (otro archivo, acciones con nombre) no se convierten en enlace.
fn link_target(doc: &PdfDocument, a: &pdf_oxide::Annotation, pages: usize) -> Option<String> {
    use pdf_oxide::{LinkAction, LinkDestination};
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
        (Some(LinkAction::Uri(u)), _) => Some(u.trim().to_string()).filter(|u| !u.is_empty()),
        (Some(LinkAction::GoTo(d)), _) | (None, Some(d)) => Some(page(d)),
        _ => None,
    }
}

/// Marca los caracteres que cubre cada anotación de enlace de la página.
fn apply_links(doc: &PdfDocument, index: usize, (mx0, my1): (f32, f32), glyphs: &mut [Glyph]) {
    let Ok(annotations) = doc.get_annotations(index) else {
        return;
    };
    let pages = doc.page_count().unwrap_or(0);
    for a in annotations {
        if a.subtype.as_deref() != Some("Link") {
            continue;
        }
        let (Some([ax0, ay0, ax1, ay1]), Some(target)) = (a.rect, link_target(doc, &a, pages))
        else {
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

pub(crate) struct Page {
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) glyphs: Vec<Glyph>,
    pub(crate) pictures: Vec<Picture>,
    /// Texto escrito con una fuente sin glifos, típica de la capa de OCR que acompaña a
    /// un escaneo.
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

pub(crate) fn page_count(doc: &PdfDocument) -> Result<usize> {
    doc.page_count()
        .map_err(|e| Rejected(format!("el PDF no tiene un árbol de páginas legible: {e}")).into())
}

/// Caja de la página: origen y tamaño en el espacio de usuario.
pub(crate) fn media_box(doc: &PdfDocument, index: usize) -> (f32, f32, f32, f32) {
    match doc.get_page_media_box(index) {
        Ok((x0, y0, x1, y1)) if x1 > x0 && y1 > y0 => (x0, y0, x1, y1),
        _ => (0.0, 0.0, 612.0, 792.0),
    }
}

/// Caracteres e imágenes de una página.
pub(crate) fn page(doc: &PdfDocument, index: usize) -> Result<Page> {
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
        let direct = doc.extract_chars(index).unwrap_or_default();
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
    let mut ocr_layer = false;
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
                italic: ch.is_italic,
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
            italic: ch.is_italic,
            mono: ch.is_monospace,
            rotated: ch.rotation_degrees.abs() > 5.0
                && (ch.rotation_degrees.abs() - 360.0).abs() > 5.0,
            span: span_of[k - 1],
            link: None,
        });
    }
    apply_links(doc, index, (mx0, my1), &mut glyphs);
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
    wanted
        .iter()
        .map(|&k| {
            let Some(h) = handles.get(k) else {
                return Encoding::Failed;
            };
            let jpeg = matches!(h.filter_chain.as_slice(), [PdfFilter::DCTDecode])
                && !matches!(h.color_space, ColorSpace::DeviceCMYK);
            if !jpeg && u64::from(h.width).saturating_mul(u64::from(h.height)) > max_pixels {
                return Encoding::TooLarge;
            }
            let encoded = if jpeg {
                h.raw_compressed_bytes().ok().map(|b| (b, "jpg"))
            } else {
                h.decode()
                    .ok()
                    .and_then(|d| d.to_png_bytes().ok())
                    .map(|b| (b, "png"))
            };
            match encoded {
                Some((bytes, ext)) if !bytes.is_empty() => Encoding::Ready(Encoded { bytes, ext }),
                _ => Encoding::Failed,
            }
        })
        .collect()
}

/// Fragmento armado de forma inconsistente: su rectángulo es mucho más ancho que la
/// página o sus caracteres visibles retroceden.
fn malformed(s: &pdf_oxide::layout::TextSpan, chars: &[TextChar], page_width: f32) -> bool {
    let xs: Vec<f32> = chars
        .iter()
        .filter(|c| !c.char.is_whitespace())
        .map(|c| c.bbox.x)
        .collect();
    s.bbox.width > 1.5 * page_width || xs.windows(2).any(|w| w[1] < w[0] - 1.0)
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

/// Vista PNG de la página, con la mayor dimensión limitada a `max_px` píxeles.
pub(crate) fn render_png(doc: &PdfDocument, index: usize, max_px: f32) -> Result<Vec<u8>> {
    let (x0, y0, x1, y1) = media_box(doc, index);
    let dpi = (max_px * 72.0 / (x1 - x0).max(y1 - y0)).clamp(36.0, 150.0) as u32;
    let img = render_page(doc, index, &RenderOptions::with_dpi(dpi))
        .map_err(|e| anyhow::anyhow!("no se pudo renderizar la página {}: {e}", index + 1))?;
    Ok(img.data)
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
mod tests {
    use super::*;

    #[test]
    fn text_strings_decode_utf16_and_latin1() {
        assert_eq!(text_string(&[0xFE, 0xFF, 0x00, 0x41, 0x00, 0xF3]), "Aó");
        assert_eq!(text_string(b"Taller formaci\xF3n"), "Taller formación");
    }
}
