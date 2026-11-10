//! Listados numerados en capturas con OCR previo. El fondo del panel confirma la
//! región; los píxeles recuperan la sangría que puede faltar en la capa invisible.
//! Se conservan los caracteres y la numeración OCR, sin reparar el programa.
use super::{
    layout::{self, CodePosition, Line, Para, Run},
    read::{Glyph, Rect},
};
use image::RgbImage;

fn tinted(p: [u8; 3]) -> bool {
    let lo = *p.iter().min().unwrap();
    let hi = *p.iter().max().unwrap();
    lo >= 160 && hi - lo >= 8
}

fn median(mut values: Vec<f32>) -> f32 {
    values.sort_by(f32::total_cmp);
    values.get(values.len() / 2).copied().unwrap_or(0.)
}

fn row_text(glyphs: &[Glyph]) -> String {
    let (mut lines, _) = layout::lines(glyphs);
    lines.sort_by(|a, b| a.x0.total_cmp(&b.x0));
    lines
        .iter()
        .map(Line::text)
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn number_anchor(line: &Line) -> Option<(usize, f32)> {
    let text = line.text();
    let text = text.trim();
    let prefix: String = text.chars().take_while(char::is_ascii_digit).collect();
    // Solo identifica el margen: la letra OCR se conserva literalmente al escribir.
    let (number, count) = if prefix.is_empty() && text.starts_with('l') {
        (1, 1)
    } else {
        (prefix.parse::<usize>().ok()?, prefix.len())
    };
    if number == 0 || number > 500 {
        return None;
    }
    let right = if prefix == text {
        line.x1
    } else {
        line.x0 + count as f32 * 0.45 * line.size
    };
    Some((number, right))
}

/// Solo regiones con fondo claro coloreado, numeración alineada y texto pequeño.
/// Los demás diseños siguen el maquetador normal; la vista permanece como evidencia.
pub(super) fn recover(
    lines: &mut Vec<Line>,
    glyphs: &[Glyph],
    image: &RgbImage,
    width: f32,
    height: f32,
) -> Vec<(Para, Rect)> {
    let sx = image.width() as f32 / width;
    let sy = image.height() as f32 / height;
    let body_size = median(
        lines
            .iter()
            .filter(|l| l.text().chars().count() > 40)
            .map(|l| l.size)
            .collect(),
    );
    let mut panels: Vec<Rect> = Vec::new();
    for line in lines.iter() {
        let Some((number, number_right)) = number_anchor(line) else {
            continue;
        };
        if number == 0 || number > 500 || (line.size > 10. && line.size > body_size * 0.85) {
            continue;
        }
        let y = ((line.base + 0.25 * line.size) * sy) as u32;
        if y >= image.height() {
            continue;
        }
        let mut start = None;
        for x in 0..image.width() {
            if tinted(image.get_pixel(x, y).0) {
                start.get_or_insert(x);
            } else if let Some(left) = start.take() {
                if (x - left) as f32 / sx < 0.3 * width {
                    continue;
                }
                let a = left as f32 / sx;
                let b = x as f32 / sx;
                if line.x0 < a
                    || (line.text().trim().parse::<usize>().is_err()
                        && line.x0 > a + 2. * line.size)
                    || number_right > a + 0.15 * (b - a)
                {
                    continue;
                }
                let probe = (left + (3. * sx) as u32).min(x - 1);
                let mut top = y;
                let mut bottom = y;
                while top > 0 && tinted(image.get_pixel(probe, top - 1).0) {
                    top -= 1;
                }
                while bottom + 1 < image.height() && tinted(image.get_pixel(probe, bottom + 1).0) {
                    bottom += 1;
                }
                let rect = Rect {
                    x0: a,
                    x1: b,
                    top: top as f32 / sy,
                    bottom: bottom as f32 / sy,
                };
                if rect.height() >= line.size
                    && !panels
                        .iter()
                        .any(|r| (r.top - rect.top).abs() < 3. && (r.x0 - rect.x0).abs() < 3.)
                {
                    panels.push(rect);
                }
            }
        }
    }
    panels.sort_by(|a, b| a.top.total_cmp(&b.top));
    let mut out = Vec::new();
    let mut taken = vec![false; lines.len()];
    for rect in panels {
        let inside = |l: &Line| {
            l.base > rect.top && l.base < rect.bottom && l.x0 >= rect.x0 && l.x1 <= rect.x1 + 1.
        };
        let anchors: Vec<_> = lines
            .iter()
            .filter(|l| {
                inside(l)
                    && (l.text().trim().parse::<usize>().is_ok() || l.x0 <= rect.x0 + 2. * l.size)
            })
            .filter_map(|l| {
                number_anchor(l)
                    .filter(|(_, r)| *r < rect.x0 + 0.15 * rect.width())
                    .map(|(n, r)| (n, l, r))
            })
            .collect();
        if anchors.is_empty() {
            continue;
        }
        let right = anchors.iter().map(|(_, _, r)| *r).fold(f32::MAX, f32::min);
        let anchors: Vec<_> = anchors
            .into_iter()
            .filter(|(_, _, r)| (*r - right).abs() < 3.)
            .collect();
        if !anchors.iter().any(|(n, _, _)| *n == 1) && anchors.len() < 2 {
            continue;
        }
        let pitch = median(
            anchors
                .windows(2)
                .filter_map(|w| {
                    let n = w[1].0.checked_sub(w[0].0)?;
                    (n > 0).then_some((w[1].1.base - w[0].1.base) / n as f32)
                })
                .filter(|p| *p > 5. && *p < 30.)
                .collect(),
        );
        let pitch = if pitch > 0. {
            pitch
        } else {
            1.4 * anchors[0].1.size
        };
        let cut = right + 4.;
        let mut rows: Vec<Vec<Glyph>> = Vec::new();
        let mut selected: Vec<_> = glyphs
            .iter()
            .filter(|g| {
                !g.rotated
                    && g.base > rect.top
                    && g.base < rect.bottom
                    && g.x0 >= rect.x0
                    && g.x1 <= rect.x1 + 1.
            })
            .cloned()
            .collect();
        selected.sort_by(|a, b| a.base.total_cmp(&b.base).then(a.x0.total_cmp(&b.x0)));
        for glyph in selected {
            if let Some(row) = rows.last_mut()
                && (row[0].base - glyph.base).abs() < 0.45 * pitch
            {
                row.push(glyph);
            } else {
                rows.push(vec![glyph]);
            }
        }
        let mut recovered = Vec::new();
        let mut starts = Vec::new();
        let mut cells = Vec::new();
        for row in rows {
            let number: Vec<_> = row.iter().filter(|g| g.x1 <= cut).cloned().collect();
            let code: Vec<_> = row.iter().filter(|g| g.x1 > cut).cloned().collect();
            let label = row_text(&number);
            let text = row_text(&code);
            if label.is_empty() && text.is_empty() {
                continue;
            }
            let base = median(row.iter().map(|g| g.base).collect());
            let size = median(row.iter().map(|g| g.size).collect());
            let ink = ink_extent(image, sx, sy, rect, cut, base, size);
            if !text.is_empty()
                && let Some((a, b)) = ink
            {
                starts.push(a);
                if text.chars().count() >= 12 {
                    cells.push((b - a) / text.chars().count() as f32);
                }
            }
            recovered.push((label, text, base, size, ink));
        }
        if recovered.is_empty() || starts.is_empty() {
            continue;
        }
        let left = starts.into_iter().fold(f32::MAX, f32::min);
        let cell = median(cells).clamp(1.5, 10.);
        let digits = recovered
            .iter()
            .map(|(n, _, _, _, _)| n.chars().count())
            .max()
            .unwrap_or(1);
        let code_lines: Vec<_> = recovered
            .into_iter()
            .map(|(label, text, base, size, ink)| {
                let indent = ink
                    .map_or(0, |(x, _)| ((x - left) / cell).round().max(0.) as usize)
                    .min(128);
                let text = if text.is_empty() {
                    label
                } else {
                    format!("{label:>digits$}  {}{text}", " ".repeat(indent))
                };
                Line {
                    x0: rect.x0,
                    x1: rect.x1,
                    base,
                    size,
                    bold: false,
                    mono: true,
                    runs: vec![Run {
                        text,
                        bold: false,
                        italic: false,
                        link: None,
                    }],
                    code: Some(CodePosition {
                        indent: 0,
                        blank_lines: 0,
                        wrapped: false,
                    }),
                }
            })
            .collect();
        for (i, line) in lines.iter().enumerate() {
            if inside(line) {
                taken[i] = true;
            }
        }
        out.push((
            Para {
                size: code_lines[0].size,
                x0: rect.x0,
                bold: false,
                mono: true,
                marker: None,
                keep_hyphen: Vec::new(),
                join_hyphen: Vec::new(),
                lines: code_lines,
            },
            rect,
        ));
    }
    let mut i = 0;
    lines.retain(|_| {
        let keep = !taken[i];
        i += 1;
        keep
    });
    out
}

fn ink_extent(
    image: &RgbImage,
    sx: f32,
    sy: f32,
    rect: Rect,
    cut: f32,
    base: f32,
    size: f32,
) -> Option<(f32, f32)> {
    let y0 = ((base - 0.8 * size).max(rect.top + 2.) * sy).max(0.) as u32;
    let y1 = ((base + 0.15 * size).min(rect.bottom - 2.) * sy).min(image.height() as f32) as u32;
    let x0 = (cut * sx).max(0.) as u32;
    let x1 = ((rect.x1 - 4.) * sx).min(image.width() as f32) as u32;
    let mut first = None;
    let mut last = 0;
    for x in x0..x1 {
        let count = (y0..y1)
            .filter(|&y| {
                let p = image.get_pixel(x, y).0;
                *p.iter().min().unwrap() < 210
            })
            .count();
        if count >= 2 {
            first.get_or_insert(x);
            last = x;
        }
    }
    first.map(|a| (a as f32 / sx, (last + 1) as f32 / sx))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn line(text: &str, x: f32, base: f32, size: f32) -> Line {
        Line {
            x0: x,
            x1: x + text.chars().count() as f32 * 3.,
            base,
            size,
            runs: vec![Run {
                text: text.into(),
                bold: false,
                italic: false,
                link: None,
            }],
            bold: false,
            mono: false,
            code: None,
        }
    }
    #[test]
    fn numbered_ocr_uses_the_panel_and_visible_ink_without_rewriting_text() {
        for (colored, with_body, wrong_numbers) in [
            (true, true, false),
            (false, true, false),
            (true, false, false),
            (true, true, true),
        ] {
            let mut im = RgbImage::from_pixel(220, 180, image::Rgb([255, 255, 255]));
            if colored {
                for y in 55..115 {
                    for x in 10..200 {
                        im.put_pixel(x, y, image::Rgb([255, 248, 225]));
                    }
                }
            }
            for (x, base) in [(50, 75), (66, 95)] {
                for y in base - 4..base {
                    for a in x..x + 60 {
                        im.put_pixel(a, y, image::Rgb([90, 90, 90]));
                    }
                }
            }
            let mut lines = vec![
                line(
                    "A sufficiently long paragraph outside the numbered panel.",
                    10.,
                    30.,
                    10.,
                ),
                line("1", 30., 75., 5.),
                line("print(variable)", 50., 75., 5.),
                line("2", 30., 95., 5.),
                line("prtnt(variable)", 50., 95., 5.),
            ];
            if wrong_numbers {
                lines[1].runs[0].text = "S".into();
                lines[3].runs[0].text = "1".into();
            }
            if !with_body {
                lines.remove(0);
            }
            let mut glyphs = Vec::new();
            for l in lines.iter().filter(|l| l.size < 10.) {
                for (i, c) in l.text().chars().enumerate() {
                    glyphs.push(Glyph {
                        c,
                        x0: l.x0 + i as f32 * 3.,
                        x1: l.x0 + (i + 1) as f32 * 3.,
                        base: l.base,
                        size: l.size,
                        bold: false,
                        italic: false,
                        mono: false,
                        rotated: false,
                        span: glyphs.len(),
                        link: None,
                    });
                }
            }
            let result = recover(&mut lines, &glyphs, &im, 220., 180.);
            if colored {
                assert_eq!(result.len(), 1);
                assert_eq!(lines.len(), usize::from(with_body));
                let text: Vec<_> = result[0].0.lines.iter().map(Line::text).collect();
                assert!(
                    text[0].starts_with(if wrong_numbers {
                        "S  print(variable)"
                    } else {
                        "1  print(variable)"
                    }),
                    "{text:?}"
                );
                assert!(
                    text[1].starts_with(if wrong_numbers {
                        "1      prtnt(variable)"
                    } else {
                        "2      prtnt(variable)"
                    }),
                    "{text:?}"
                );
            } else {
                assert!(result.is_empty());
                assert_eq!(lines.len(), 5);
            }
        }
    }
}
