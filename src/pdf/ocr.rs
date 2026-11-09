//! OCR local de las páginas escaneadas: PaddleOCR (detección DBNet y reconocimiento
//! SVTR) ejecutado en Rust con tract a través de pdf_oxide. La página se renderiza, se
//! endereza, se detectan sus líneas y cada línea se reconoce por separado; el resultado
//! se entrega como caracteres posicionados para que la maquetación de `layout` lo trate
//! igual que el texto digital.
use super::read::{self, Glyph};
use anyhow::{Context, Result, anyhow};
use image::{DynamicImage, GrayImage, Luma, imageops::FilterType};
use pdf_oxide::{
    PdfDocument,
    ocr::{DetectedBox, OcrConfig, TextDetector, TextRecognizer, crop_text_region},
    rendering::{RenderOptions, render_page},
};
use std::path::Path;

/// Archivos que debe contener `OKF_OCR_MODELS_DIR`.
pub(crate) const MODEL_FILES: [&str; 3] = ["det.onnx", "rec.onnx", "dict.txt"];
/// Resolución de la página que se reconoce.
const DPI: f32 = 200.0;
/// Lado mayor de la imagen reconocida: acota la memoria en páginas grandes
/// (diapositivas, planos).
const MAX_SIDE_PX: f32 = 2400.0;
/// Altura a la que el reconocedor lleva cada línea (la de PaddleOCR).
const LINE_HEIGHT: u32 = 48;
/// Las líneas se rellenan hasta un múltiplo de este ancho: tract prepara un plan por
/// cada forma de entrada, y con anchos libres la memoria y el tiempo crecen con cada
/// línea nueva. Un relleno mayor hace que el reconocedor pierda espacios entre
/// palabras: con 320 px, las líneas cortas salían con todas sus palabras unidas.
const WIDTH_STEP: u32 = 64;
/// Hilos que reconocen las líneas de una página.
const THREADS: usize = 2;
/// Inclinación máxima que se corrige, en grados, y paso de la búsqueda.
const MAX_SKEW: f32 = 5.0;
const SKEW_STEP: f32 = 0.1;
/// Margen que se añade a la derecha de cada línea antes de reconocerla, en alturas de
/// línea: la caja detectada suele dejar fuera el punto final. Un margen mayor, o también
/// a la izquierda, hace leer el grano del papel como guiones.
const RIGHT_MARGIN: f32 = 0.15;
/// Altura de las ascendentes sobre la línea base, en cuerpos de letra: convierte la
/// altura de la tinta medida en tamaño de letra.
const ASCENT: f32 = 0.73;
/// Los tamaños hasta esta fracción por encima de la mediana de la página, o hasta
/// `SMALLER` por debajo, toman la mediana, para que la medición no parta los párrafos.
/// La medición falla sobre todo por defecto (líneas cortas, letra monoespaciada), y un
/// subtítulo apenas mayor que el cuerpo debe seguir siendo mayor.
const LARGER: f32 = 0.06;
const SMALLER: f32 = 0.25;
/// Confianza media de una página por debajo de la cual su texto es una incidencia.
pub(crate) const LOW_CONFIDENCE: f32 = 0.8;

/// Modelos cargados; se cargan una vez por documento, al llegar a la primera página
/// escaneada.
pub(crate) struct Engine {
    detector: TextDetector,
    recognizer: TextRecognizer,
    min_confidence: f32,
}

impl Engine {
    pub(crate) fn load(dir: &Path) -> Result<Self> {
        let config = OcrConfig::default();
        let detector = TextDetector::new(dir.join("det.onnx"), config.clone())
            .map_err(|e| anyhow!("no se pudo cargar el modelo de detección del OCR: {e}"))?;
        let recognizer =
            TextRecognizer::new(dir.join("rec.onnx"), dir.join("dict.txt"), config.clone())
                .map_err(|e| {
                    anyhow!("no se pudo cargar el modelo de reconocimiento del OCR: {e}")
                })?;
        Ok(Engine {
            detector,
            recognizer,
            min_confidence: config.rec_threshold,
        })
    }
}

/// Comprueba al arrancar el worker que los modelos existen y se pueden cargar.
pub fn check_models(dir: &Path) -> Result<()> {
    for file in MODEL_FILES {
        let path = dir.join(file);
        anyhow::ensure!(
            path.is_file(),
            "OKF_OCR_ENABLED=true, pero falta {} (OKF_OCR_MODELS_DIR); se obtiene con tools/ocr_models.sh",
            path.display()
        );
    }
    Engine::load(dir)
        .map(drop)
        .with_context(|| format!("modelos de OCR inválidos en {}", dir.display()))
}

/// Texto reconocido en una página.
pub(crate) struct PageText {
    pub(crate) glyphs: Vec<Glyph>,
    /// Confianza media, ponderada por caracteres.
    pub(crate) confidence: f32,
    /// Líneas reconocidas y, de ellas, las de confianza baja.
    pub(crate) lines: usize,
    pub(crate) low_lines: usize,
}

/// Reconoce el texto de la página `index` (desde 0).
pub(crate) fn page(engine: &Engine, doc: &PdfDocument, index: usize) -> Result<PageText> {
    let (x0, y0, x1, y1) = read::media_box(doc, index);
    let (width_pt, height_pt) = ((x1 - x0).abs().max(1.0), (y1 - y0).abs().max(1.0));
    let dpi = DPI
        .min(MAX_SIDE_PX * 72.0 / width_pt.max(height_pt))
        .max(36.0);
    let rendered = render_page(doc, index, &RenderOptions::with_dpi(dpi as u32))
        .map_err(|e| anyhow!("no se pudo renderizar la página para el OCR: {e}"))?;
    let gray = image::load_from_memory(&rendered.data)
        .context("no se pudo leer la página renderizada para el OCR")?
        .to_luma8();
    drop(rendered);
    let gray = deskew(gray);
    // Puntos por píxel en cada eje, a partir del tamaño real de la imagen.
    let (sx, sy) = (
        width_pt / gray.width().max(1) as f32,
        height_pt / gray.height().max(1) as f32,
    );
    let image = DynamicImage::ImageLuma8(gray);
    let boxes = engine
        .detector
        .detect(&image)
        .map_err(|e| anyhow!("falló la detección de texto del OCR: {e}"))?;
    let chunk = boxes.len().div_ceil(THREADS).max(1);
    let recognized: Vec<(DetectedBox, String, f32, Option<f32>)> = std::thread::scope(|scope| {
        let workers: Vec<_> = boxes
            .chunks(chunk)
            .map(|part| {
                let image = &image;
                scope.spawn(move || {
                    part.iter()
                        .map(|b| {
                            let crop = crop_text_region(image, &widened(b, image.width()))
                                .map_err(|e| anyhow!("no se pudo recortar una línea: {e}"))?;
                            let ink = ascent_px(&crop);
                            let r = engine
                                .recognizer
                                .recognize(&padded(&crop))
                                .map_err(|e| anyhow!("falló el reconocimiento del OCR: {e}"))?;
                            Ok((b.clone(), r.text, r.confidence, ink))
                        })
                        .collect::<Result<Vec<_>>>()
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|w| w.join().expect("hilo del OCR"))
            .collect::<Result<Vec<_>>>()
            .map(|v| v.into_iter().flatten().collect())
    })?;
    // Tamaño de letra de cada línea: por la tinta medida a resolución completa o, si no
    // se puede medir, por la altura de la caja.
    let kept: Vec<(Bounds, String, f32, f32)> = recognized
        .into_iter()
        .filter(|(_, text, confidence, _)| {
            *confidence >= engine.min_confidence && !text.trim().is_empty()
        })
        .map(|(b, text, confidence, ink)| {
            let bounds = Bounds::of(&b, sx, sy);
            let size = match ink {
                Some(px) => px * sy / ASCENT,
                None => (bounds.bottom - bounds.top) * 0.75,
            };
            (bounds, text.trim().to_string(), confidence, size.max(1.0))
        })
        .collect();
    let mut sizes: Vec<f32> = kept.iter().map(|x| x.3).collect();
    sizes.sort_by(f32::total_cmp);
    let median = sizes.get(sizes.len() / 2).copied().unwrap_or(0.0);
    let mut glyphs = Vec::new();
    let (mut lines, mut low_lines, mut weighted, mut chars) = (0, 0, 0.0f32, 0usize);
    for (span, (bounds, text, confidence, size)) in kept.into_iter().enumerate() {
        let n = text.chars().count();
        lines += 1;
        if confidence < LOW_CONFIDENCE {
            low_lines += 1;
        }
        weighted += confidence * n as f32;
        chars += n;
        let size = if size >= median * (1.0 - SMALLER) && size <= median * (1.0 + LARGER) {
            median
        } else {
            size
        };
        glyphs.extend(line_glyphs(&text, &bounds, size, span));
    }
    Ok(PageText {
        glyphs,
        confidence: if chars == 0 {
            0.0
        } else {
            weighted / chars as f32
        },
        lines,
        low_lines,
    })
}

/// Caja de una línea detectada, en puntos de la página.
struct Bounds {
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
}

impl Bounds {
    fn of(b: &DetectedBox, sx: f32, sy: f32) -> Self {
        let xs = b.polygon.iter().map(|p| p[0]);
        let ys = b.polygon.iter().map(|p| p[1]);
        Bounds {
            left: xs.clone().fold(f32::MAX, f32::min) * sx,
            right: xs.fold(f32::MIN, f32::max) * sx,
            top: ys.clone().fold(f32::MAX, f32::min) * sy,
            bottom: ys.fold(f32::MIN, f32::max) * sy,
        }
    }
}

/// La caja detectada, ensanchada por la derecha `RIGHT_MARGIN` alturas de línea.
fn widened(b: &DetectedBox, width: u32) -> [[f32; 2]; 4] {
    let [tl, tr, br, bl] = b.polygon;
    let height = (bl[1] - tl[1]).abs().max((br[1] - tr[1]).abs());
    let right = |v: f32| (v + RIGHT_MARGIN * height).min(width.saturating_sub(1) as f32);
    [tl, [right(tr[0]), tr[1]], [right(br[0]), br[1]], bl]
}

/// Caracteres de una línea reconocida, repartidos a partes iguales sobre su caja, en
/// puntos de la página, con el tamaño de letra `size`.
fn line_glyphs(text: &str, b: &Bounds, size: f32, span: usize) -> Vec<Glyph> {
    let base = (b.top + b.bottom) / 2.0 + 0.25 * size;
    let n = text.chars().count().max(1);
    let advance = (b.right - b.left).max(0.0) / n as f32;
    text.chars()
        .enumerate()
        .map(|(k, c)| Glyph {
            c,
            x0: b.left + k as f32 * advance,
            x1: b.left + (k + 1) as f32 * advance,
            base,
            size,
            bold: false,
            italic: false,
            mono: false,
            rotated: false,
            span,
            link: None,
        })
        .collect()
}

/// Altura, en píxeles, de la tinta de una línea desde lo alto de las ascendentes hasta
/// la línea base. La franja central (la altura de la x) es la de más tinta; la línea
/// base es su borde inferior, y las ascendentes llegan hasta la primera fila vacía por
/// encima, de modo que los descendentes de la línea anterior no cuentan. `None` si el
/// recorte no tiene contraste.
fn ascent_px(crop: &DynamicImage) -> Option<f32> {
    let g = crop.to_luma8();
    let (w, h) = g.dimensions();
    if w < 4 || h < 4 {
        return None;
    }
    let mut hist = [0u32; 256];
    for p in g.pixels() {
        hist[p.0[0] as usize] += 1;
    }
    let total = (w * h) as f32;
    let percentile = |q: f32| {
        let mut acc = 0u32;
        for (v, n) in hist.iter().enumerate() {
            acc += n;
            if acc as f32 >= q * total {
                return v as u8;
            }
        }
        255
    };
    let (dark, light) = (percentile(0.02), percentile(0.6));
    if light.saturating_sub(dark) < 40 {
        return None;
    }
    let threshold = ((dark as u16 + light as u16) / 2) as u8;
    let rows: Vec<u32> = (0..h)
        .map(|y| {
            (0..w)
                .filter(|&x| g.get_pixel(x, y).0[0] < threshold)
                .count() as u32
        })
        .collect();
    let (peak, &max) = rows.iter().enumerate().max_by_key(|(_, c)| **c)?;
    let band = |k: usize| rows[k] as f32 >= 0.35 * max as f32;
    let mut base = peak;
    while base + 1 < rows.len() && band(base + 1) {
        base += 1;
    }
    let mut top = peak;
    while top > 0 && rows[top - 1] as f32 >= 0.03 * max as f32 {
        top -= 1;
    }
    (base > top).then(|| (base - top + 1) as f32)
}

/// Lleva la línea a la altura del reconocedor y rellena con blanco hasta un múltiplo
/// de `WIDTH_STEP`, para que las formas de entrada sean pocas.
fn padded(crop: &DynamicImage) -> DynamicImage {
    let (w, h) = (crop.width().max(1), crop.height().max(1));
    let width = ((w as f32 * LINE_HEIGHT as f32 / h as f32) as u32).max(1);
    let target = width.div_ceil(WIDTH_STEP) * WIDTH_STEP;
    let resized = crop
        .resize_exact(width, LINE_HEIGHT, FilterType::Lanczos3)
        .to_luma8();
    let mut canvas = GrayImage::from_pixel(target, LINE_HEIGHT, Luma([255]));
    image::imageops::replace(&mut canvas, &resized, 0, 0);
    DynamicImage::ImageLuma8(canvas)
}

/// Endereza la página. Una inclinación de menos de un grado basta para que la detección
/// junte líneas vecinas. El ángulo es el que maximiza la varianza del perfil de filas de
/// los píxeles oscuros, sobre una copia reducida.
fn deskew(gray: GrayImage) -> GrayImage {
    let angle = skew(&gray);
    if angle.abs() < SKEW_STEP {
        return gray;
    }
    rotate(&gray, angle)
}

/// Inclinación estimada, en grados.
fn skew(gray: &GrayImage) -> f32 {
    let small = image::imageops::resize(
        gray,
        (gray.width() / 2).clamp(1, 1000),
        (gray.height() / 2).clamp(1, 1000),
        FilterType::Triangle,
    );
    let (w, h) = small.dimensions();
    let dark: Vec<(f32, f32)> = small
        .enumerate_pixels()
        .filter(|(_, _, p)| p.0[0] < 128)
        .map(|(x, y, _)| (x as f32 - w as f32 / 2.0, y as f32))
        .collect();
    if dark.len() < 100 {
        return 0.0;
    }
    let score = |degrees: f32| {
        let t = degrees.to_radians().tan();
        let offset = h as f32;
        let mut rows = vec![0u32; h as usize * 3];
        for &(x, y) in &dark {
            let r = y - x * t + offset;
            if r >= 0.0
                && let Some(row) = rows.get_mut(r as usize)
            {
                *row += 1;
            }
        }
        rows.iter().map(|&v| (v as u64) * (v as u64)).sum::<u64>()
    };
    let steps = (MAX_SKEW / SKEW_STEP).round() as i32;
    let mut best = (0.0, score(0.0));
    for i in -steps..=steps {
        let degrees = i as f32 * SKEW_STEP;
        let s = score(degrees);
        if s > best.1 {
            best = (degrees, s);
        }
    }
    best.0
}

/// Gira la imagen `degrees` en sentido contrario a la inclinación medida, con
/// interpolación bilineal y fondo blanco.
fn rotate(gray: &GrayImage, degrees: f32) -> GrayImage {
    let (w, h) = gray.dimensions();
    let (sin, cos) = degrees.to_radians().sin_cos();
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let at = |x: u32, y: u32| gray.get_pixel(x, y).0[0] as f32;
    GrayImage::from_fn(w, h, |x, y| {
        let (dx, dy) = (x as f32 - cx, y as f32 - cy);
        let (sx, sy) = (cx + dx * cos - dy * sin, cy + dx * sin + dy * cos);
        if sx < 0.0 || sy < 0.0 || sx >= (w - 1) as f32 || sy >= (h - 1) as f32 {
            return Luma([255]);
        }
        let (x0, y0) = (sx as u32, sy as u32);
        let (fx, fy) = (sx - x0 as f32, sy - y0 as f32);
        let v = at(x0, y0) * (1.0 - fx) * (1.0 - fy)
            + at(x0 + 1, y0) * fx * (1.0 - fy)
            + at(x0, y0 + 1) * (1.0 - fx) * fy
            + at(x0 + 1, y0 + 1) * fx * fy;
        Luma([v.round().clamp(0.0, 255.0) as u8])
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Página sintética con renglones negros horizontales.
    fn ruled(w: u32, h: u32) -> GrayImage {
        GrayImage::from_fn(w, h, |x, y| {
            let line = y % 40 < 8 && (100..w - 100).contains(&x) && (100..h - 100).contains(&y);
            Luma([if line { 0 } else { 255 }])
        })
    }

    #[test]
    fn skew_is_measured_and_corrected() {
        let page = ruled(1200, 1600);
        assert_eq!(skew(&page), 0.0);
        let tilted = rotate(&page, -1.5);
        let measured = skew(&tilted);
        assert!(
            (measured - 1.5).abs() <= 0.2,
            "inclinación medida {measured}"
        );
        assert!(skew(&deskew(tilted)).abs() <= 0.2);
    }

    #[test]
    fn lines_are_padded_to_few_widths() {
        let widths: std::collections::HashSet<u32> = (40..1500)
            .step_by(7)
            .map(|w| padded(&DynamicImage::ImageLuma8(GrayImage::new(w, 30))).width())
            .collect();
        assert!(widths.iter().all(|w| w % WIDTH_STEP == 0));
        assert!(widths.len() <= 40, "{widths:?}");
    }

    #[test]
    fn glyphs_cover_the_box_in_page_points() {
        let b = DetectedBox {
            polygon: [[100.0, 50.0], [300.0, 50.0], [300.0, 90.0], [100.0, 90.0]],
            confidence: 0.9,
        };
        let g = line_glyphs("año 2", &Bounds::of(&b, 0.5, 0.5), 15.0, 7);
        assert_eq!(g.len(), 5);
        assert_eq!(g[0].x0, 50.0);
        assert_eq!(g[4].x1, 150.0);
        assert!(
            g.iter()
                .all(|x| x.span == 7 && x.base > 25.0 && x.base < 45.0)
        );
        assert_eq!(g.iter().map(|x| x.c).collect::<String>(), "año 2");
    }
}
