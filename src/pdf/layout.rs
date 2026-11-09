//! Maquetación de una página PDF: de caracteres a líneas, orden de lectura por cortes
//! de espacio en blanco (columnas y bandas) y bloques (párrafos, elementos de lista,
//! tablas sencillas e imágenes).
use super::read::{Glyph, Rect};

/// Tramo de una línea con el mismo énfasis y el mismo enlace.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Run {
    pub(crate) text: String,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    /// Destino del enlace que lo cubre (ver `Glyph::link`).
    pub(crate) link: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct Line {
    pub(crate) x0: f32,
    pub(crate) x1: f32,
    pub(crate) base: f32,
    /// Tamaño dominante (por caracteres).
    pub(crate) size: f32,
    pub(crate) runs: Vec<Run>,
    pub(crate) bold: bool,
    pub(crate) mono: bool,
}

impl Line {
    pub(crate) fn text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }
    pub(crate) fn top(&self) -> f32 {
        self.base - 0.75 * self.size
    }
    pub(crate) fn bottom(&self) -> f32 {
        self.base + 0.25 * self.size
    }
    pub(crate) fn rect(&self) -> Rect {
        Rect {
            x0: self.x0,
            top: self.top(),
            x1: self.x1,
            bottom: self.bottom(),
        }
    }
    fn chars(&self) -> usize {
        self.runs.iter().map(|r| r.text.chars().count()).sum()
    }
}

/// Distancia horizontal, en tamaños de letra, a partir de la cual dos caracteres de la
/// misma altura pertenecen a fragmentos distintos (columnas, celdas o textos
/// separados).
const FRAGMENT_GAP: f32 = 0.8;
/// Dentro de un mismo fragmento de pdf_oxide (un texto que el PDF escribe seguido),
/// solo un hueco mayor separa partes distintas; los espacios de una línea justificada
/// pueden superar con holgura los de `FRAGMENT_GAP`.
const SPAN_GAP: f32 = 3.0;
/// Hueco máximo entre una viñeta o un número de lista y su texto.
const MARKER_GAP: f32 = 3.0;
/// Distancia a partir de la cual se supone un espacio que el PDF no escribe.
const IMPLICIT_SPACE: f32 = 0.3;

fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

/// Tramo de caracteres seguidos de un fragmento de pdf_oxide.
struct Piece<'a> {
    glyphs: Vec<&'a Glyph>,
    x0: f32,
    x1: f32,
    base: f32,
    size: f32,
}

impl Piece<'_> {
    fn span(&self) -> (f32, f32) {
        (self.base - 0.75 * self.size, self.base + 0.25 * self.size)
    }
}

/// Líneas de la página: las del texto horizontal y, aparte, las del texto girado.
///
/// Cada fragmento de pdf_oxide (que nunca cruza líneas) se parte donde hay un hueco
/// grande; los tramos con la misma línea base y contiguos forman una línea, y un
/// superíndice o subíndice se une al texto que toca. Así dos columnas con líneas base
/// distintas no se mezclan.
pub(crate) fn lines(glyphs: &[Glyph]) -> (Vec<Line>, Vec<Line>) {
    let mut rotated = Vec::new();
    let mut pieces: Vec<Piece> = Vec::new();
    // Espacios que llegan como fragmento propio: se devuelven a la línea en la que caen.
    let mut loose: Vec<&Glyph> = Vec::new();
    let mut k = 0;
    while k < glyphs.len() {
        let mut end = k;
        while end + 1 < glyphs.len() && glyphs[end + 1].span == glyphs[k].span {
            end += 1;
        }
        let group: Vec<&Glyph> = glyphs[k..=end].iter().collect();
        k = end + 1;
        if group.iter().all(|g| g.c.is_whitespace()) {
            loose.extend(group);
            continue;
        }
        if group[0].rotated {
            rotated.extend(line_from(&group));
            continue;
        }
        let mut sorted = group;
        sorted.sort_by(|a, b| a.x0.total_cmp(&b.x0));
        let mut current: Vec<&Glyph> = Vec::new();
        let mut last: Option<&Glyph> = None;
        for g in sorted {
            if !g.c.is_whitespace() {
                if let Some(p) = last
                    && g.x0 - p.x1 > SPAN_GAP * p.size.max(g.size)
                {
                    pieces.extend(piece(std::mem::take(&mut current)));
                }
                last = Some(g);
            }
            current.push(g);
        }
        pieces.extend(piece(current));
    }
    // Tramos de la misma línea: misma línea base y contiguos, o un tramo menor que se
    // superpone en vertical y toca a otro (superíndice o subíndice).
    let mut order: Vec<usize> = (0..pieces.len()).collect();
    order.sort_by(|&a, &b| pieces[a].base.total_cmp(&pieces[b].base));
    let max_size = pieces.iter().map(|p| p.size).fold(0.0f32, f32::max);
    let mut parent: Vec<usize> = (0..pieces.len()).collect();
    for (oi, &i) in order.iter().enumerate() {
        for &j in &order[oi + 1..] {
            let (a, b) = (&pieces[i], &pieces[j]);
            if b.base - a.base > max_size {
                break;
            }
            let (small, large) = (a.size.min(b.size), a.size.max(b.size));
            let (l, r) = if a.x0 <= b.x0 { (a, b) } else { (b, a) };
            let gap = r.x0 - l.x1;
            // Una viñeta o un número de lista suelto se une a su texto aunque los separe
            // una tabulación.
            let marker = l.glyphs.iter().filter(|g| !g.c.is_whitespace()).count() <= 4 && {
                let t: String = l.glyphs.iter().map(|g| g.c).collect();
                list_marker(&format!("{} x", t.trim())).is_some()
            };
            let reach = if marker { MARKER_GAP } else { FRAGMENT_GAP };
            let same_line = (a.base - b.base).abs() <= 0.25 * small
                && gap <= reach * large
                && gap >= -0.5 * large;
            let script = small <= 0.85 * large && {
                let (at, ab) = a.span();
                let (bt, bb) = b.span();
                let overlap = ab.min(bb) - at.max(bt);
                overlap >= 0.3 * small * 1.0 && (-0.3 * large..=0.3 * large).contains(&gap)
            };
            if same_line || script {
                let (x, y) = (find(&mut parent, i), find(&mut parent, j));
                if x != y {
                    parent[x] = y;
                }
            }
        }
    }
    let mut groups: Vec<(usize, Vec<&Glyph>)> = Vec::new();
    let mut slot: Vec<Option<usize>> = vec![None; pieces.len()];
    for (i, piece) in pieces.iter().enumerate() {
        let root = find(&mut parent, i);
        let at = match slot[root] {
            Some(at) => at,
            None => {
                groups.push((root, Vec::new()));
                slot[root] = Some(groups.len() - 1);
                groups.len() - 1
            }
        };
        groups[at].1.extend(piece.glyphs.iter().copied());
    }
    let mut out = Vec::new();
    for (root, mut gs) in groups {
        let (base, size) = (pieces[root].base, pieces[root].size);
        let (x0, x1) = gs
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), g| (a.min(g.x0), b.max(g.x1)));
        gs.extend(
            loose
                .iter()
                .filter(|g| (g.base - base).abs() <= 0.25 * size && g.x0 >= x0 && g.x0 <= x1),
        );
        gs.sort_by(|a, b| a.x0.total_cmp(&b.x0));
        let mut kept: Vec<&Glyph> = Vec::with_capacity(gs.len());
        let mut last: Option<&Glyph> = None;
        for g in gs {
            if !g.c.is_whitespace() {
                if let Some(p) = last {
                    // Texto dibujado dos veces para simular negrita o sombra.
                    if p.span != g.span && p.c == g.c && (g.x0 - p.x0).abs() < 0.15 * g.size {
                        continue;
                    }
                    // pdf_oxide puede sintetizar un guion encima de otro carácter.
                    if p.span != g.span
                        && (g.x0 - p.x0).abs() < 0.15 * g.size
                        && (g.c == '-' || p.c == '-')
                    {
                        if p.c == '-'
                            && let Some(at) = kept.iter().rposition(|x| std::ptr::eq(*x, p))
                        {
                            kept[at] = g;
                            last = Some(g);
                        }
                        continue;
                    }
                }
                last = Some(g);
            }
            kept.push(g);
        }
        out.extend(line_from(&kept));
    }
    out.sort_by(|a, b| a.base.total_cmp(&b.base).then(a.x0.total_cmp(&b.x0)));
    (out, rotated)
}

fn piece(glyphs: Vec<&Glyph>) -> Option<Piece<'_>> {
    let visible: Vec<&&Glyph> = glyphs.iter().filter(|g| !g.c.is_whitespace()).collect();
    let first = visible.first()?;
    let size = visible.iter().map(|g| g.size).fold(0.0f32, f32::max);
    Some(Piece {
        x0: visible.iter().map(|g| g.x0).fold(f32::MAX, f32::min),
        x1: visible.iter().map(|g| g.x1).fold(f32::MIN, f32::max),
        base: first.base,
        size,
        glyphs,
    })
}

/// Une los caracteres de un fragmento, con espacios donde el PDF los escribe o donde
/// la distancia lo indica.
fn line_from(glyphs: &[&Glyph]) -> Option<Line> {
    let visible: Vec<&&Glyph> = glyphs.iter().filter(|g| !g.c.is_whitespace()).collect();
    let first = visible.first()?;
    let mut runs: Vec<Run> = Vec::new();
    let mut prev: Option<&Glyph> = None;
    let mut pending_space = false;
    let mut sizes: Vec<(f32, usize)> = Vec::new();
    for g in glyphs {
        if g.c.is_whitespace() {
            pending_space = prev.is_some();
            continue;
        }
        if let Some(p) = prev
            && (pending_space || g.x0 - p.x1 > IMPLICIT_SPACE * p.size.min(g.size))
            && let Some(r) = runs.last_mut()
            && !r.text.ends_with(' ')
        {
            r.text.push(' ');
        }
        pending_space = false;
        match runs.last_mut() {
            Some(r) if r.bold == g.bold && r.italic == g.italic && r.link == g.link => {
                r.text.push(g.c)
            }
            _ => runs.push(Run {
                text: g.c.to_string(),
                bold: g.bold,
                italic: g.italic,
                link: g.link.clone(),
            }),
        }
        match sizes.iter_mut().find(|(s, _)| (*s - g.size).abs() < 0.25) {
            Some((_, n)) => *n += 1,
            None => sizes.push((g.size, 1)),
        }
        prev = Some(g);
    }
    let size = sizes
        .iter()
        .max_by(|a, b| a.1.cmp(&b.1).then(a.0.total_cmp(&b.0)))
        .map_or(first.size, |s| s.0);
    let base = visible
        .iter()
        .filter(|g| (g.size - size).abs() < 0.25)
        .map(|g| g.base)
        .next()
        .unwrap_or(first.base);
    if let Some(r) = runs.last_mut() {
        let t = r.text.trim_end().len();
        r.text.truncate(t);
    }
    Some(Line {
        x0: visible.iter().map(|g| g.x0).fold(f32::MAX, f32::min),
        x1: visible.iter().map(|g| g.x1).fold(f32::MIN, f32::max),
        base,
        size,
        bold: visible.iter().all(|g| g.bold),
        mono: visible.iter().all(|g| g.mono),
        runs,
    })
}

/// Elemento que se ordena en la página: una línea o una imagen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Item {
    Line(usize),
    Picture(usize),
}

/// Grupo de elementos que se leen seguidos: una columna o banda (de arriba abajo), o
/// una fila de celdas una junto a otra (de izquierda a derecha), cada una con sus
/// líneas de arriba abajo.
#[derive(Debug)]
pub(crate) struct Leaf {
    pub(crate) items: Vec<Item>,
    pub(crate) row: bool,
    /// Celdas de una fila, con los elementos de cada una.
    pub(crate) cells: Vec<Vec<Item>>,
}

impl Leaf {
    fn flow(items: Vec<Item>) -> Self {
        Leaf {
            items,
            row: false,
            cells: Vec::new(),
        }
    }
}

/// Datos de una página que necesita el orden de lectura.
pub(crate) struct Region<'a> {
    pub(crate) lines: &'a [Line],
    pub(crate) pictures: &'a [Rect],
}

impl Region<'_> {
    fn rect(&self, it: Item) -> Rect {
        match it {
            Item::Line(i) => self.lines[i].rect(),
            Item::Picture(i) => self.pictures[i],
        }
    }
    fn size(&self, it: Item) -> f32 {
        match it {
            Item::Line(i) => self.lines[i].size,
            Item::Picture(_) => 0.0,
        }
    }
}

fn median(mut v: Vec<f32>) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(f32::total_cmp);
    v[v.len() / 2]
}

/// Orden de lectura por cortes recursivos de espacio en blanco. En cada región se
/// prefiere un canal vertical que la atraviese de arriba abajo (columnas), salvo que
/// las dos partes tengan líneas breves alineadas fila a fila, que son celdas de una
/// tabla; si no lo hay, se corta por el mayor espacio horizontal (bandas).
pub(crate) fn reading_order(region: &Region, items: Vec<Item>) -> Vec<Leaf> {
    let mut out = Vec::new();
    let sizes: Vec<f32> = items
        .iter()
        .map(|&i| region.size(i))
        .filter(|s| *s > 0.0)
        .collect();
    let gutter = 0.7 * median(sizes).max(4.0);
    cut(region, items, gutter, &mut out);
    out
}

fn cut(region: &Region, items: Vec<Item>, gutter: f32, out: &mut Vec<Leaf>) {
    if items.len() <= 1 {
        if !items.is_empty() {
            out.push(Leaf::flow(items));
        }
        return;
    }
    // Mayor espacio horizontal; sin ninguno, los elementos forman una sola banda.
    let mut by_y: Vec<(f32, f32)> = items
        .iter()
        .map(|&i| {
            let r = region.rect(i);
            (r.top, r.bottom)
        })
        .collect();
    by_y.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut band_gap: Option<(f32, f32)> = None;
    let mut reach = by_y[0].1;
    for &(t, b) in &by_y[1..] {
        if t > reach && band_gap.is_none_or(|(w, _)| t - reach > w) {
            band_gap = Some((t - reach, (t + reach) / 2.0));
        }
        reach = reach.max(b);
    }
    let Some((_, band_at)) = band_gap else {
        // Una sola banda: celdas una junto a otra (una fila), cada una con los
        // elementos que se solapan en horizontal, o un solo bloque.
        let mut items = items;
        items.sort_by(|a, b| region.rect(*a).x0.total_cmp(&region.rect(*b).x0));
        let mut cells: Vec<(f32, Vec<Item>)> = Vec::new();
        for it in items {
            let r = region.rect(it);
            match cells.last_mut() {
                Some((reach, cell)) if r.x0 < *reach - 1.0 => {
                    *reach = reach.max(r.x1);
                    cell.push(it);
                }
                _ => cells.push((r.x1, vec![it])),
            }
        }
        let by_top = |v: &mut Vec<Item>| {
            v.sort_by(|a, b| {
                let (ra, rb) = (region.rect(*a), region.rect(*b));
                ra.top.total_cmp(&rb.top).then(ra.x0.total_cmp(&rb.x0))
            })
        };
        if cells.len() < 2 {
            let mut items: Vec<Item> = cells.into_iter().flat_map(|(_, c)| c).collect();
            by_top(&mut items);
            out.push(Leaf::flow(items));
        } else {
            let mut cells: Vec<Vec<Item>> = cells.into_iter().map(|(_, c)| c).collect();
            cells.iter_mut().for_each(by_top);
            out.push(Leaf {
                items: cells.iter().flatten().copied().collect(),
                row: true,
                cells,
            });
        }
        return;
    };
    // Canal vertical más ancho que atraviesa la región.
    let mut by_x: Vec<(f32, f32)> = items
        .iter()
        .map(|&i| {
            let r = region.rect(i);
            (r.x0, r.x1)
        })
        .collect();
    by_x.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut best: Option<(f32, f32)> = None;
    let mut reach = by_x[0].1;
    for &(x0, x1) in &by_x[1..] {
        if x0 - reach >= gutter && best.is_none_or(|(w, _)| x0 - reach > w) {
            best = Some((x0 - reach, (x0 + reach) / 2.0));
        }
        reach = reach.max(x1);
    }
    if let Some((_, at)) = best {
        let (left, right): (Vec<Item>, Vec<Item>) =
            items.iter().partition(|&&i| region.rect(i).x1 <= at);
        if !left.is_empty() && !right.is_empty() && !table_like(region, &left, &right) {
            cut(region, left, gutter, out);
            cut(region, right, gutter, out);
            return;
        }
    }
    let (top, bottom): (Vec<Item>, Vec<Item>) = items
        .iter()
        .partition(|&&i| region.rect(i).bottom <= band_at);
    cut(region, top, gutter, out);
    let mid = out.len();
    cut(region, bottom, gutter, out);
    // Las dos hojas que se tocan en el corte siguen el mismo flujo de texto si se
    // solapan en horizontal: se unen para que sus párrafos puedan continuar.
    if mid > 0 && mid < out.len() && !out[mid - 1].row && !out[mid].row {
        let extent = |leaf: &Leaf| {
            leaf.items.iter().fold((f32::MAX, f32::MIN), |(a, b), &i| {
                let r = region.rect(i);
                (a.min(r.x0), b.max(r.x1))
            })
        };
        let (a, b) = (extent(&out[mid - 1]), extent(&out[mid]));
        let overlap = a.1.min(b.1) - a.0.max(b.0);
        if overlap >= 0.5 * (a.1 - a.0).min(b.1 - b.0) {
            let next = out.remove(mid);
            out[mid - 1].items.extend(next.items);
        }
    }
}

/// Dos partes con líneas alineadas fila a fila y al menos una de ellas con líneas
/// breves forman una tabla, no dos columnas de texto.
fn table_like(region: &Region, left: &[Item], right: &[Item]) -> bool {
    let lines = |items: &[Item]| -> Vec<usize> {
        items
            .iter()
            .filter_map(|i| match i {
                Item::Line(k) => Some(*k),
                Item::Picture(_) => None,
            })
            .collect()
    };
    let (l, r) = (lines(left), lines(right));
    if l.is_empty() || r.is_empty() || l.len() + r.len() < 3 {
        return false;
    }
    // La alineación se mide en la franja vertical que comparten las dos partes.
    let extent = |a: &[usize]| {
        a.iter().fold((f32::MAX, f32::MIN), |(t, b), &i| {
            (
                t.min(region.lines[i].top()),
                b.max(region.lines[i].bottom()),
            )
        })
    };
    let (lt, lb) = extent(&l);
    let (rt, rb) = extent(&r);
    let (top, bottom) = (lt.max(rt), lb.min(rb));
    let aligned = |a: &[usize], b: &[usize]| {
        let shared: Vec<usize> = a
            .iter()
            .copied()
            .filter(|&i| region.lines[i].base >= top && region.lines[i].base <= bottom)
            .collect();
        if shared.is_empty() {
            return 0.0;
        }
        let n = shared
            .iter()
            .filter(|&&i| {
                let li = &region.lines[i];
                b.iter()
                    .any(|&j| (region.lines[j].base - li.base).abs() <= 0.3 * li.size)
            })
            .count();
        n as f32 / shared.len() as f32
    };
    let short =
        |a: &[usize]| median(a.iter().map(|&i| region.lines[i].chars() as f32).collect()) < 25.0;
    aligned(&l, &r) >= 0.5 && aligned(&r, &l) >= 0.5 && (short(&l) || short(&r))
}

/// Viñeta o número al comienzo de una línea seguido de espacio: devuelve la etiqueta y
/// el resto del texto.
pub(crate) fn list_marker(text: &str) -> Option<(String, String)> {
    let t = text.trim_start();
    let (label, rest) = t.split_once(' ')?;
    let rest = rest.trim_start();
    if rest.is_empty() {
        return None;
    }
    let bullet = label.chars().count() == 1
        && label.chars().next().is_some_and(|c| {
            matches!(
                c,
                '•' | '◦'
                    | '▪'
                    | '▫'
                    | '■'
                    | '□'
                    | '●'
                    | '○'
                    | '‣'
                    | '⁃'
                    | '–'
                    | '—'
                    | '-'
                    | '*'
                    | '➢'
                    | '➤'
                    | '►'
                    | '▶'
                    | '✓'
                    | '✔'
                    | '·'
                    | '∙'
                    | '❖'
                    | '◆'
                    | '◇'
            ) || ('\u{f000}'..='\u{f0ff}').contains(&c)
        });
    let enumerated = {
        let body = label
            .strip_prefix('(')
            .unwrap_or(label)
            .strip_suffix(['.', ')'])
            .filter(|b| !b.is_empty());
        body.is_some_and(|b| {
            (b.len() <= 3 && b.chars().all(|c| c.is_ascii_digit()))
                || (b.chars().count() == 1 && b.chars().all(|c| c.is_alphabetic()))
                || (b.len() <= 5
                    && b.chars()
                        .all(|c| matches!(c, 'i' | 'v' | 'x' | 'I' | 'V' | 'X')))
        })
    };
    (bullet || enumerated).then(|| (label.to_string(), rest.to_string()))
}

/// Bloque de una página antes de reconciliar continuaciones y encabezados.
#[derive(Clone, Debug)]
pub(crate) enum Block {
    Para(Para),
    Table(Vec<Vec<String>>),
    Picture(usize),
}

#[derive(Clone, Debug)]
pub(crate) struct Para {
    pub(crate) lines: Vec<Line>,
    /// Líneas cuyo guion final es parte de una palabra compuesta y se conserva.
    pub(crate) keep_hyphen: Vec<usize>,
    /// Líneas cuyo guion final parte una palabra que el documento escribe entera.
    pub(crate) join_hyphen: Vec<usize>,
    /// Viñeta o número de un elemento de lista.
    pub(crate) marker: Option<String>,
    pub(crate) size: f32,
    pub(crate) bold: bool,
    pub(crate) mono: bool,
    pub(crate) x0: f32,
}

impl Para {
    fn new(line: Line) -> Self {
        let (marker, line) = match list_marker(&line.text()) {
            Some((label, _)) => (Some(label.clone()), strip_marker(line, &label)),
            None => (None, line),
        };
        Para {
            size: line.size,
            bold: line.bold,
            mono: line.mono,
            x0: line.x0,
            marker,
            lines: vec![line],
            keep_hyphen: Vec::new(),
            join_hyphen: Vec::new(),
        }
    }
    pub(crate) fn last(&self) -> &Line {
        self.lines.last().expect("párrafo con líneas")
    }
    /// Texto con las líneas unidas por espacios y las palabras partidas reunidas.
    pub(crate) fn text(&self) -> String {
        join_runs(&self.runs())
            .0
            .iter()
            .map(|r| r.text.as_str())
            .collect()
    }
    /// Tramos de énfasis de todo el párrafo, con las líneas ya unidas.
    pub(crate) fn runs(&self) -> Vec<Run> {
        let mut out: Vec<Run> = Vec::new();
        for (k, line) in self.lines.iter().enumerate() {
            for (j, r) in line.runs.iter().enumerate() {
                let mut text = r.text.clone();
                if k > 0 && j == 0 {
                    text.insert(
                        0,
                        if self.keep_hyphen.contains(&(k - 1)) {
                            KEPT
                        } else if self.join_hyphen.contains(&(k - 1)) {
                            JOINED
                        } else {
                            '\n'
                        },
                    );
                }
                match out.last_mut() {
                    Some(o) if o.bold == r.bold && o.italic == r.italic && o.link == r.link => {
                        o.text.push_str(&text)
                    }
                    _ => out.push(Run {
                        text,
                        bold: r.bold,
                        italic: r.italic,
                        link: r.link.clone(),
                    }),
                }
            }
        }
        out
    }
}

/// Salto de línea tras un guion que se conserva (palabra compuesta).
const KEPT: char = '\u{1}';
/// Salto de línea tras un guion que parte una palabra conocida: se suprime el guion.
const JOINED: char = '\u{2}';

/// Une los saltos de línea marcados con `\n`: una palabra partida con guion entre
/// minúsculas se reúne; en otro caso el salto es un espacio.
pub(crate) fn join_runs(runs: &[Run]) -> (Vec<Run>, usize) {
    let mut joined = 0;
    let all: String = runs.iter().map(|r| r.text.as_str()).collect();
    let chars: Vec<char> = all.chars().collect();
    let mut drop = vec![false; chars.len()];
    let mut space = vec![false; chars.len()];
    for i in 0..chars.len() {
        if chars[i] == KEPT {
            drop[i] = true;
            continue;
        }
        if chars[i] == JOINED {
            drop[i] = true;
            if i > 0 && matches!(chars[i - 1], '-' | '\u{00AD}' | '‐') {
                drop[i - 1] = true;
                joined += 1;
            }
            continue;
        }
        if chars[i] != '\n' {
            continue;
        }
        let before = i.checked_sub(1).map(|j| chars[j]);
        let before2 = i.checked_sub(2).map(|j| chars[j]);
        let after = chars.get(i + 1).copied();
        if matches!(before, Some('-' | '\u{00AD}' | '‐'))
            && before2.is_some_and(char::is_lowercase)
            && after.is_some_and(char::is_lowercase)
        {
            drop[i - 1] = true;
            drop[i] = true;
            joined += 1;
        } else if before.is_some_and(|c| c == ' ')
            || after.is_some_and(|c| c == ' ')
            || (matches!(before, Some('-' | '‐')) && before2.is_some_and(char::is_alphanumeric))
        {
            // Un guion que no parte una palabra (`AI-assisted`) se conserva sin espacio.
            drop[i] = true;
        } else {
            space[i] = true;
        }
    }
    let mut out: Vec<Run> = Vec::new();
    let mut pos = 0;
    for r in runs {
        let n = r.text.chars().count();
        let mut text = String::new();
        for k in pos..pos + n {
            if drop[k] {
                continue;
            }
            text.push(if space[k] { ' ' } else { chars[k] });
        }
        pos += n;
        if !text.is_empty() {
            out.push(Run {
                text,
                bold: r.bold,
                italic: r.italic,
                link: r.link.clone(),
            });
        }
    }
    (out, joined)
}

/// Quita la etiqueta de lista del comienzo de la línea.
fn strip_marker(mut line: Line, label: &str) -> Line {
    let mut remaining = label.chars().count();
    for r in line.runs.iter_mut() {
        if remaining == 0 {
            break;
        }
        let lead = r.text.len() - r.text.trim_start().len();
        let body: String = r.text[lead..].chars().skip(remaining).collect();
        let taken = r.text[lead..].chars().count().min(remaining);
        remaining -= taken;
        r.text = body.trim_start().to_string();
    }
    line.runs.retain(|r| !r.text.is_empty());
    line
}

fn terminal(text: &str) -> bool {
    text.trim_end()
        .ends_with(['.', '!', '?', ':', ';', '…', '»', '"', '”'])
}

/// Bloques de una hoja en orden: párrafos, elementos de lista, tablas e imágenes.
/// Cada bloque lleva su rectángulo y la hoja de lectura de la que procede.
pub(crate) fn blocks(region: &Region, leaves: &[Leaf]) -> Vec<(Block, Rect, usize)> {
    let mut out: Vec<(Block, Rect, usize)> = Vec::new();
    let mut k = 0;
    while k < leaves.len() {
        let leaf = &leaves[k];
        // Filas consecutivas de varias celdas de texto: tabla.
        let text_row = |l: &Leaf| l.row && l.items.iter().all(|i| matches!(i, Item::Line(_)));
        if text_row(leaf) {
            let mut end = k;
            while end + 1 < leaves.len() && text_row(&leaves[end + 1]) {
                end += 1;
            }
            if end > k {
                let (rows, rect, used) = table(region, leaves, k, end);
                out.push((Block::Table(rows), rect, k));
                k = used + 1;
                continue;
            }
        }
        if leaf.row {
            // Celdas sueltas una junto a otra: cada una es un bloque.
            for cell in &leaf.cells {
                let mut current: Option<(Para, Rect)> = None;
                for &it in cell {
                    match it {
                        Item::Picture(i) => {
                            out.extend(current.take().map(|(p, r)| (Block::Para(p), r, k)));
                            out.push((Block::Picture(i), region.rect(it), k));
                        }
                        Item::Line(i) => {
                            let line = &region.lines[i];
                            match current.as_mut() {
                                Some((p, r)) if list_marker(&line.text()).is_none() => {
                                    p.lines.push(line.clone());
                                    r.bottom = line.bottom();
                                }
                                _ => {
                                    out.extend(current.take().map(|(p, r)| (Block::Para(p), r, k)));
                                    current = Some((Para::new(line.clone()), line.rect()));
                                }
                            }
                        }
                    }
                }
                out.extend(current.take().map(|(p, r)| (Block::Para(p), r, k)));
            }
            k += 1;
            continue;
        }
        let lines: Vec<usize> = leaf
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Line(l) => Some(*l),
                Item::Picture(_) => None,
            })
            .collect();
        let right = lines
            .iter()
            .map(|&i| region.lines[i].x1)
            .fold(f32::MIN, f32::max);
        let gaps: Vec<f32> = lines
            .windows(2)
            .map(|w| region.lines[w[1]].top() - region.lines[w[0]].bottom())
            .filter(|g| *g >= 0.0)
            .collect();
        let usual_gap = median(gaps);
        let mut current: Option<(Para, Rect)> = None;
        for &it in &leaf.items {
            let line = match it {
                Item::Picture(i) => {
                    out.extend(current.take().map(|(p, r)| (Block::Para(p), r, k)));
                    out.push((Block::Picture(i), region.rect(it), k));
                    continue;
                }
                Item::Line(l) => &region.lines[l],
            };
            if let Some((p, r)) = current.as_mut()
                && continues(p, line, right, usual_gap)
            {
                p.lines.push(line.clone());
                *r = Rect {
                    x0: r.x0.min(line.x0),
                    top: r.top,
                    x1: r.x1.max(line.x1),
                    bottom: line.bottom(),
                };
                continue;
            }
            out.extend(current.take().map(|(p, r)| (Block::Para(p), r, k)));
            current = Some((Para::new(line.clone()), line.rect()));
        }
        out.extend(current.take().map(|(p, r)| (Block::Para(p), r, k)));
        k += 1;
    }
    out
}

/// ¿La línea sigue el párrafo en curso?
fn continues(p: &Para, line: &Line, right: f32, usual_gap: f32) -> bool {
    let prev = p.last();
    let text = line.text();
    if list_marker(&text).is_some() || line.mono != p.mono {
        return false;
    }
    if (line.size - prev.size).abs() > 0.15 * prev.size || line.bold != prev.bold {
        return false;
    }
    let gap = line.top() - prev.bottom();
    if gap > (usual_gap + 0.3 * prev.size).max(0.45 * prev.size) || gap < -0.5 * prev.size {
        return false;
    }
    // El código sigue mientras no haya un espacio vertical: su sangría y su puntuación
    // no marcan párrafos.
    if p.mono {
        return true;
    }
    // Un elemento de lista continúa con líneas sangradas hasta su texto.
    if p.marker.is_some() && line.x0 < p.x0 - 0.5 * prev.size {
        return false;
    }
    // Fin de párrafo: la línea anterior termina la frase y queda corta, o la nueva
    // empieza sangrada.
    let prev_text = prev.text();
    if terminal(&prev_text)
        && (prev.x1 < right - 3.0 * prev.size || line.x0 > prev.x0 + 0.8 * prev.size)
        && p.marker.is_none()
    {
        return false;
    }
    true
}

/// Tabla de las filas `from..=to`; añade a la celda de su columna las líneas sueltas
/// que la continúan justo debajo. Devuelve las filas, el rectángulo y la última hoja
/// usada.
fn table(
    region: &Region,
    leaves: &[Leaf],
    from: usize,
    to: usize,
) -> (Vec<Vec<String>>, Rect, usize) {
    let cell_rect = |cell: &[Item]| {
        cell.iter()
            .map(|&i| region.rect(i))
            .fold(None, |acc: Option<Rect>, r| {
                Some(match acc {
                    None => r,
                    Some(o) => Rect {
                        x0: o.x0.min(r.x0),
                        top: o.top.min(r.top),
                        x1: o.x1.max(r.x1),
                        bottom: o.bottom.max(r.bottom),
                    },
                })
            })
    };
    let mut spans: Vec<(f32, f32)> = leaves[from..=to]
        .iter()
        .flat_map(|l| l.cells.iter().filter_map(|c| cell_rect(c)))
        .map(|r| (r.x0, r.x1))
        .collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut cols: Vec<(f32, f32)> = Vec::new();
    for (a, b) in spans {
        match cols.last_mut() {
            Some(c) if a <= c.1 => c.1 = c.1.max(b),
            _ => cols.push((a, b)),
        }
    }
    // Columna de una celda: la que más se solapa con ella (los números alineados a la
    // derecha no empiezan todos en el mismo punto).
    let overlaps = |r: &Rect| -> Vec<(usize, f32)> {
        cols.iter()
            .enumerate()
            .map(|(i, c)| (i, r.x1.min(c.1) - r.x0.max(c.0)))
            .filter(|(_, o)| *o > 0.5)
            .collect()
    };
    let col_of = |r: &Rect| {
        overlaps(r)
            .into_iter()
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map_or(0, |(i, _)| i)
    };
    let text_of = |it: Item| match it {
        Item::Line(i) => region.lines[i].text(),
        Item::Picture(_) => String::new(),
    };
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut rect: Option<Rect> = None;
    let grow = |rect: &mut Option<Rect>, r: Rect| {
        *rect = Some(match *rect {
            None => r,
            Some(o) => Rect {
                x0: o.x0.min(r.x0),
                top: o.top.min(r.top),
                x1: o.x1.max(r.x1),
                bottom: o.bottom.max(r.bottom),
            },
        });
    };
    let mut last = to;
    let mut k = from;
    while k < leaves.len() {
        let leaf = &leaves[k];
        let is_row = leaf.row && leaf.items.iter().all(|i| matches!(i, Item::Line(_)));
        if leaf.row && !is_row {
            break;
        }
        // Tras una línea de continuación, la tabla sigue con las filas cuyas celdas
        // caen en sus columnas.
        if k > to
            && is_row
            && !leaf
                .cells
                .iter()
                .filter_map(|c| cell_rect(c))
                .all(|r| overlaps(&r).len() == 1)
        {
            break;
        }
        if is_row {
            let mut row = vec![String::new(); cols.len()];
            for c in &leaf.cells {
                let Some(r) = cell_rect(c) else { continue };
                grow(&mut rect, r);
                let cell = &mut row[col_of(&r)];
                for &it in c {
                    if !cell.is_empty() {
                        cell.push(' ');
                    }
                    cell.push_str(&text_of(it));
                }
            }
            rows.push(row);
        } else {
            // Línea suelta dentro de una columna, pegada a la fila anterior.
            let bottom = rect.map_or(0.0, |r| r.bottom);
            let fits = leaf.items.len() == 1 && matches!(leaf.items[0], Item::Line(_)) && {
                let r = region.rect(leaf.items[0]);
                let c = col_of(&r);
                r.top - bottom <= 0.6 * region.size(leaf.items[0])
                    && r.x1 <= cols[c].1 + 1.0
                    && (c + 1 == cols.len() || r.x1 < cols[c + 1].0)
            };
            if !fits {
                break;
            }
            let r = region.rect(leaf.items[0]);
            grow(&mut rect, r);
            if let Some(row) = rows.last_mut() {
                let cell = &mut row[col_of(&r)];
                if !cell.is_empty() && !cell.ends_with('-') {
                    cell.push(' ');
                }
                cell.push_str(&text_of(leaf.items[0]));
            }
        }
        last = k;
        k += 1;
    }
    let rect = rect.unwrap_or(Rect {
        x0: 0.0,
        top: 0.0,
        x1: 0.0,
        bottom: 0.0,
    });
    (rows, rect, last.max(to))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Caracteres de un texto escrito en una línea a partir de `x`, con letras de ancho
    /// fijo de medio tamaño.
    fn text_at(s: &str, x: f32, base: f32, size: f32, bold: bool) -> Vec<Glyph> {
        let w = size * 0.5;
        s.chars()
            .enumerate()
            .map(|(k, c)| Glyph {
                c,
                x0: x + k as f32 * w,
                x1: x + (k + 1) as f32 * w,
                base,
                size,
                bold,
                italic: false,
                mono: false,
                rotated: false,
                span: (base * 10.0) as usize + (x as usize) * 100_000,
                link: None,
            })
            .collect()
    }

    fn order(glyphs: &[Glyph]) -> Vec<String> {
        let (lines, _) = lines(glyphs);
        let region = Region {
            lines: &lines,
            pictures: &[],
        };
        let items = (0..lines.len()).map(Item::Line).collect();
        let leaves = reading_order(&region, items);
        blocks(&region, &leaves)
            .into_iter()
            .map(|(b, _, _)| match b {
                Block::Para(p) => match &p.marker {
                    Some(m) => format!("{m} {}", p.text()),
                    None => p.text(),
                },
                Block::Table(rows) => format!("{rows:?}"),
                Block::Picture(_) => "imagen".into(),
            })
            .collect()
    }

    #[test]
    fn two_columns_are_read_one_after_the_other() {
        let mut g = text_at("Titulo general", 50.0, 40.0, 16.0, true);
        let left = [
            "la columna izquierda empieza aquí y",
            "sigue con una línea y una pala-",
            "bra partida al final.",
        ];
        let right = [
            "la columna derecha tiene también",
            "su propio texto, que ocupa tres",
            "líneas y termina.",
        ];
        for (k, t) in left.iter().enumerate() {
            g.extend(text_at(t, 50.0, 80.0 + k as f32 * 12.0, 10.0, false));
        }
        for (k, t) in right.iter().enumerate() {
            g.extend(text_at(t, 240.0, 80.0 + k as f32 * 12.0, 10.0, false));
        }
        assert_eq!(
            order(&g),
            [
                "Titulo general",
                "la columna izquierda empieza aquí y sigue con una línea y una palabra partida al final.",
                "la columna derecha tiene también su propio texto, que ocupa tres líneas y termina."
            ]
        );
    }

    #[test]
    fn aligned_short_cells_form_a_table_and_markers_start_list_items() {
        let mut g = Vec::new();
        for (k, (a, b)) in [
            ("Factura", "BF-11"),
            ("Recibo", "2281"),
            ("Fecha", "22 oct"),
        ]
        .iter()
        .enumerate()
        {
            let base = 50.0 + k as f32 * 14.0;
            g.extend(text_at(a, 40.0, base, 10.0, false));
            g.extend(text_at(b, 160.0, base, 10.0, false));
        }
        g.extend(text_at("• primer punto que", 40.0, 120.0, 10.0, false));
        g.extend(text_at("continúa aquí", 50.0, 132.0, 10.0, false));
        g.extend(text_at("• segundo punto", 40.0, 144.0, 10.0, false));
        assert_eq!(
            order(&g),
            [
                r#"[["Factura", "BF-11"], ["Recibo", "2281"], ["Fecha", "22 oct"]]"#,
                "• primer punto que continúa aquí",
                "• segundo punto"
            ]
        );
    }

    #[test]
    fn line_breaks_join_hyphenated_words_only_between_lowercase_letters() {
        let runs = |t: &str| {
            vec![Run {
                text: t.into(),
                bold: false,
                italic: false,
                link: None,
            }]
        };
        let (r, n) = join_runs(&runs("health-\ncare y AI-\nassisted\nfin"));
        assert_eq!(r[0].text, "healthcare y AI-assisted fin");
        assert_eq!(n, 1);
        assert_eq!(
            list_marker("1. Primero"),
            Some(("1.".into(), "Primero".into()))
        );
        assert_eq!(list_marker("a) uno"), Some(("a)".into(), "uno".into())));
        assert_eq!(list_marker("Página 1"), None);
    }
}
