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
    /// Posición dentro del listado, independiente del margen físico de cada página.
    pub(crate) code: Option<CodePosition>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CodePosition {
    pub(crate) indent: usize,
    pub(crate) blank_lines: usize,
    pub(crate) wrapped: bool,
}

/// Hojas reservadas para un listado y sus anotaciones, fuera del flujo corriente.
pub(crate) const CODE_FLOW: usize = usize::MAX - 4;
pub(crate) const CODE_NOTE: usize = usize::MAX - 3;

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
/// Caracteres visibles máximos de un superíndice o subíndice que se une a su línea.
const SCRIPT_CHARS: usize = 6;
/// Distancia a partir de la cual se supone un espacio que el PDF no escribe.
const IMPLICIT_SPACE: f32 = 0.3;
/// Un espacio escrito entre dos letras no separa palabras si las letras se tocan (hueco
/// de hasta `TOUCHING` tamaños de letra) y el espacio tiene anchura (más de `ZERO_WIDTH`
/// tamaños): la expansión de una ligadura («Th is», «de ﬁnition») o un corte de palabra
/// de la capa de texto («serv ing»). Un espacio de ancho nulo marca una posición, no una
/// ligadura, y un espacio que cabe en un hueco real («matches movie») es un espacio.
const TOUCHING: f32 = 0.03;
const ZERO_WIDTH: f32 = 0.05;
/// Tampoco separa palabras un espacio que casi toca a la letra anterior (hueco menor que
/// `GLUED_GAP` tamaños) y que la siguiente letra solapa en `OVERLAP` tamaños o más: un
/// ajuste de interletraje («T each»).
const GLUED_GAP: f32 = 0.15;
const OVERLAP: f32 = 0.5;

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
    // Tramos de cada grupo, guardados en su raíz.
    let mut members: Vec<Vec<usize>> = (0..pieces.len()).map(|i| vec![i]).collect();
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
            // El número de un encabezado («1», «2.3») separado de su título por un
            // hueco amplio, con el mismo tamaño y en negrita los dos.
            let numbered = !marker
                && l.glyphs.iter().all(|g| g.c.is_whitespace() || g.bold)
                && r.glyphs.iter().all(|g| g.c.is_whitespace() || g.bold)
                && (a.size - b.size).abs() <= 0.5
                && heading_label(l.glyphs.iter().map(|g| g.c).collect::<String>().trim())
                && r.glyphs
                    .iter()
                    .find(|g| !g.c.is_whitespace())
                    .is_some_and(|g| g.c.is_uppercase());
            let reach = if marker || numbered {
                MARKER_GAP
            } else {
                FRAGMENT_GAP
            };
            let same_line = (a.base - b.base).abs() <= 0.25 * small
                && gap <= reach * large
                && gap >= -0.5 * large;
            // Un superíndice o subíndice es una pieza corta (llamada de nota, exponente).
            // Una capital grande de una o dos letras solo se une a la línea alineada con
            // su borde superior: unida a las líneas de arriba y de abajo, las fundía en
            // una e intercalaba sus letras.
            let script = small <= 0.85 * large && {
                let (at, ab) = a.span();
                let (bt, bb) = b.span();
                let overlap = ab.min(bb) - at.max(bt);
                let (minor, major) = if a.size <= b.size { (a, b) } else { (b, a) };
                let visible = |p: &Piece| p.glyphs.iter().filter(|g| !g.c.is_whitespace()).count();
                let fits = visible(minor) <= SCRIPT_CHARS
                    || (visible(major) <= 2
                        && (minor.span().0 - major.span().0).abs() <= 0.25 * large);
                fits && overlap >= 0.3 * small && (-0.3 * large..=0.3 * large).contains(&gap)
            };
            if same_line || script {
                let (x, y) = (find(&mut parent, i), find(&mut parent, j));
                // Dos tramos de líneas base distintas que se solapan en horizontal no son
                // de la misma línea: unir sus grupos intercalaría sus letras. El texto
                // duplicado para simular negrita comparte la línea base y se descarta
                // después.
                let clash = x != y
                    && members[x].iter().any(|&m| {
                        members[y].iter().any(|&n| {
                            let (p, q) = (&pieces[m], &pieces[n]);
                            let size = p.size.min(q.size);
                            p.x1.min(q.x1) - p.x0.max(q.x0) > 0.3 * size
                                && (p.base - q.base).abs() > 0.1 * size
                        })
                    });
                if x != y && !clash {
                    parent[x] = y;
                    let moved = std::mem::take(&mut members[x]);
                    members[y].extend(moved);
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
    let mono = visible.iter().all(|g| g.mono);
    let mut pending_spaces = 0;
    let mut sizes: Vec<(f32, usize)> = Vec::new();
    // Último espacio escrito desde la letra anterior.
    let mut space: Option<&Glyph> = None;
    for g in glyphs {
        if g.c.is_whitespace() {
            pending_spaces += usize::from(prev.is_some());
            space = Some(g);
            continue;
        }
        // Un espacio escrito entre dos letras que se tocan no separa palabras: es la
        // expansión de una ligadura («Th») o un ajuste de interletraje («T each»).
        let glued = |p: &Glyph| {
            let Some(s) = space else { return false };
            let size = p.size.min(g.size);
            let (gap, width) = (g.x0 - p.x1, s.x1 - s.x0);
            !mono
                && p.c.is_alphanumeric()
                && g.c.is_alphanumeric()
                && ((width > ZERO_WIDTH * size && gap <= TOUCHING * size)
                    || (gap < GLUED_GAP * size && s.x1 - g.x0 >= OVERLAP * size))
        };
        if let Some(p) = prev
            && ((pending_spaces > 0 && !glued(p))
                || g.x0 - p.x1 > IMPLICIT_SPACE * p.size.min(g.size))
            && let Some(r) = runs.last_mut()
            && !r.text.ends_with(' ')
        {
            let spaces = if mono {
                (((g.x0 - p.x1) / (0.6 * p.size)).round().max(1.0) as usize)
                    .max(pending_spaces)
                    .min(256)
            } else {
                1
            };
            r.text.extend(std::iter::repeat_n(' ', spaces));
        }
        pending_spaces = 0;
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
        mono,
        code: None,
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
#[derive(Clone, Debug)]
pub(crate) struct Leaf {
    pub(crate) items: Vec<Item>,
    pub(crate) row: bool,
    /// Celdas de una fila, con los elementos de cada una.
    pub(crate) cells: Vec<Vec<Item>>,
    table: bool,
}

impl Leaf {
    fn flow(items: Vec<Item>) -> Self {
        Leaf {
            items,
            row: false,
            cells: Vec::new(),
            table: false,
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
    if let Some((top, bottom, rows)) = table_band(region, &items) {
        let before = items
            .iter()
            .copied()
            .filter(|&i| region.rect(i).bottom <= top)
            .collect();
        let after = items
            .iter()
            .copied()
            .filter(|&i| region.rect(i).top >= bottom)
            .collect();
        let mut out = reading_order(region, before);
        out.extend(rows);
        out.extend(reading_order(region, after));
        return out;
    }
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

/// Tres filas de al menos tres celdas alineadas fijan una banda tabular antes del
/// corte por columnas. No cambia el recorrido de un artículo corriente a dos columnas.
fn table_band(region: &Region, items: &[Item]) -> Option<(f32, f32, Vec<Leaf>)> {
    let mut order: Vec<_> = items
        .iter()
        .copied()
        .filter(|&i| matches!(i, Item::Line(_)))
        .collect();
    order.sort_by(|&a, &b| region.rect(a).top.total_cmp(&region.rect(b).top));
    let mut rows: Vec<Leaf> = Vec::new();
    for it in order {
        if let Some(last) = rows.last_mut()
            && (region.rect(it).bottom - region.rect(last.items[0]).bottom).abs()
                < 0.3 * region.size(it)
        {
            last.items.push(it);
            last.cells.push(vec![it]);
        } else {
            rows.push(Leaf {
                items: vec![it],
                row: true,
                cells: vec![vec![it]],
                table: false,
            });
        }
    }
    for row in &mut rows {
        row.items
            .sort_by(|&a, &b| region.rect(a).x0.total_cmp(&region.rect(b).x0));
        row.cells = row.items.iter().map(|&i| vec![i]).collect();
    }
    let candidates: Vec<_> = rows
        .iter()
        .filter(|r| {
            r.cells.len() >= 3
                && r.items
                    .iter()
                    .all(|i| matches!(i,Item::Line(k) if !region.lines[*k].mono))
        })
        .collect();
    for pair in candidates.windows(2) {
        // Los renglones consecutivos de tarjetas o párrafos paralelos no son
        // filas independientes: hace falta espacio entre los anclajes iniciales.
        let first = cell_rect(region, &pair[0].items)?;
        let second = cell_rect(region, &pair[1].items)?;
        if second.top - first.bottom < 0.6 * region.size(pair[0].items[0]) {
            continue;
        }
        let Some(cols) = table_columns(region, pair[0], pair[1]) else {
            continue;
        };
        // Una tabla breve dentro de una columna no convierte la columna narrativa
        // vecina en una tercera celda: el canal entre columnas es mucho más ancho.
        let gaps: Vec<_> = cols.windows(2).map(|w| w[1].0 - w[0].1).collect();
        if gaps.iter().copied().fold(0.0, f32::max)
            > 3.0 * gaps.iter().copied().fold(f32::MAX, f32::min)
        {
            continue;
        }
        let size = region.size(pair[0].items[0]);
        let edges: Vec<_> = cols.windows(2).map(|w| w[1].0 - 1.0).collect();
        let fits = |r: Rect| {
            (0..cols.len()).any(|i| {
                (i == 0 || r.x0 >= edges[i - 1] - 1.0)
                    && (i + 1 == cols.len() || r.x1 <= edges[i] + 1.0)
            })
        };
        let top = cell_rect(region, &pair[0].items)?.top;
        let mut bottom = cell_rect(region, &pair[1].items)?.bottom;
        let mut count = 2;
        let initial_bottom = bottom;
        for row in candidates
            .iter()
            .filter(|r| region.rect(r.items[0]).top > initial_bottom)
        {
            let r = cell_rect(region, &row.items)?;
            if r.top - bottom > 12.0 * size
                || row.cells.len() != cols.len()
                || row
                    .items
                    .iter()
                    .any(|&i| !fits(region.rect(i)) || (region.size(i) - size).abs() > 0.5)
            {
                break;
            }
            bottom = r.bottom;
            count += 1;
        }
        if count < 3 {
            continue;
        }
        // Incluye las últimas líneas de las celdas que continúan debajo de la fila.
        let last_anchor = bottom;
        let mut selected = Vec::new();
        let mut valid = true;
        for row in &rows {
            let r = cell_rect(region, &row.items)?;
            if r.top >= top && r.top <= bottom + 2.0 * size {
                if row.items.iter().any(|&i| {
                    let mono = matches!(i, Item::Line(k) if region.lines[k].mono);
                    !fits(region.rect(i))
                        || ((region.size(i) - size).abs() > 0.5
                            && (!mono || !(0.7 * size..=1.3 * size).contains(&region.size(i))))
                }) {
                    if r.top >= last_anchor {
                        break;
                    }
                    valid = false;
                    break;
                }
                bottom = bottom.max(r.bottom);
                let mut leaf = if row.items.len() == 1 {
                    Leaf::flow(row.items.clone())
                } else {
                    row.clone()
                };
                leaf.table = true;
                selected.push(leaf);
            }
        }
        if !valid
            || items.iter().any(|&i| {
                let r = region.rect(i);
                r.top < bottom
                    && r.bottom > top
                    && (matches!(i, Item::Picture(_)) || r.top < top || r.bottom > bottom)
            })
        {
            continue;
        }
        let (x0, x1) = selected.iter().flat_map(|l| l.items.iter()).fold(
            (f32::MAX, f32::MIN),
            |(a, b), &i| {
                let r = region.rect(i);
                (a.min(r.x0), b.max(r.x1))
            },
        );
        if captioned_figure(
            region,
            Rect {
                x0,
                top,
                x1,
                bottom,
            },
        ) {
            continue;
        }
        return Some((top, bottom, selected));
    }
    None
}

/// Caracteres máximos de un rótulo de figura: una línea más larga entre la zona y el
/// pie es texto corrido, y el pie ya no es de la zona.
const FIGURE_LABEL_CHARS: usize = 60;
/// Distancia máxima, en tamaños de letra de la zona, entre la zona y su pie de figura:
/// un diagrama puede ocupar media página por debajo de sus primeras filas alineadas.
const FIGURE_REACH: f32 = 60.0;

/// ¿Empieza el texto como un pie de figura numerado: «Figure 1.1», «Figura 3»,
/// «Fig. 2»?
fn figure_caption(text: &str) -> bool {
    let t = text.trim_start();
    ["Figure ", "Figura ", "Fig. ", "Fig "].iter().any(|p| {
        t.strip_prefix(p)
            .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
    })
}

/// ¿Es la zona una figura con rótulos? Debajo hay un pie de figura numerado (puede
/// empezar en el margen, fuera de la zona) y entre los dos, de lo que se solapa con la
/// zona en horizontal, solo hay rótulos breves, sin una imagen que ocupe la mitad de su
/// ancho (esa imagen sería la figura del pie). Una figura con texto no se convierte en
/// tabla, aunque sus rótulos estén alineados fila a fila. Solo se mira debajo: encima
/// suele estar el párrafo que la cita («Figure 8.2 shows…»).
fn captioned_figure(region: &Region, zone: Rect) -> bool {
    let overlaps = |r: &Rect| r.x1 > zone.x0 && r.x0 < zone.x1;
    let inside = |r: &Rect| r.x0 >= zone.x0 - 1.0 && r.x1 <= zone.x1 + 1.0;
    let size = median(
        region
            .lines
            .iter()
            .filter(|l| l.top() >= zone.top - 1.0 && l.bottom() <= zone.bottom + 1.0)
            .filter(|l| inside(&l.rect()))
            .map(|l| l.size)
            .collect(),
    )
    .max(4.0);
    let reach = FIGURE_REACH * size;
    let mut below: Vec<&Line> = region
        .lines
        .iter()
        .filter(|l| {
            l.top() >= zone.bottom - 0.5 * l.size
                && (overlaps(&l.rect()) || figure_caption(&l.text()))
        })
        .collect();
    below.sort_by(|a, b| a.top().total_cmp(&b.top()));
    for line in below {
        if line.top() - zone.bottom > reach {
            return false;
        }
        let text = line.text();
        if figure_caption(&text) {
            let width = zone.x1 - zone.x0;
            return !region.pictures.iter().any(|p| {
                p.top >= zone.bottom - 1.0
                    && p.bottom <= line.top() + 1.0
                    && overlaps(p)
                    && p.x1 - p.x0 >= 0.5 * width
            });
        }
        if text.trim().chars().count() > FIGURE_LABEL_CHARS {
            return false;
        }
    }
    false
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
                table: false,
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

/// Líneas mínimas de cada parte para leerla como texto corrido.
const RUNNING_LINES: usize = 6;
/// Proporción de las líneas de una parte que continúan la anterior para que sea texto
/// corrido (un índice o un glosario).
const RUNNING_SHARE: f32 = 0.5;
/// Ancho mínimo de la parte más estrecha frente a la más ancha: las columnas de un
/// índice son iguales; una tabla de rótulo y descripción, no.
const RUNNING_WIDTH: f32 = 0.6;
/// Hueco, en interlineados habituales de la parte, a partir del cual una línea abre
/// un bloque (una fila de tabla, una sección).
const START_GAP: f32 = 1.4;

/// ¿Es la parte texto corrido? Al menos `RUNNING_LINES` líneas y la mayoría continúa la
/// anterior: empieza en minúscula o con una cifra tras una línea sin puntuación final,
/// como las subentradas y los números de página de un índice.
fn running(region: &Region, part: &[usize]) -> bool {
    if part.len() < RUNNING_LINES {
        return false;
    }
    let continued = part
        .windows(2)
        .filter(|w| {
            let next = region.lines[w[1]].text();
            !terminal(&region.lines[w[0]].text())
                && next
                    .trim_start()
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_lowercase() || c.is_ascii_digit())
        })
        .count();
    continued as f32 >= RUNNING_SHARE * (part.len() - 1) as f32
}

/// Líneas de la parte que abren un bloque, sin contar la primera: las que siguen a un
/// hueco mayor que `START_GAP` interlineados habituales.
fn starts(region: &Region, part: &[usize]) -> Vec<f32> {
    let gaps: Vec<f32> = part
        .windows(2)
        .map(|w| region.lines[w[1]].base - region.lines[w[0]].base)
        .collect();
    let usual = median(gaps.clone());
    part.windows(2)
        .zip(gaps)
        .filter(|(_, g)| *g > START_GAP * usual)
        .map(|(w, _)| region.lines[w[1]].base)
        .collect()
}

/// Dos columnas de texto corrido de ancho parecido cuyos bloques no empiezan a la vez,
/// como las de un índice o un glosario: no son filas de una tabla. En una tabla, cada
/// fila abre un bloque en todas sus columnas.
fn running_columns(region: &Region, l: &[usize], r: &[usize]) -> bool {
    let width = |a: &[usize]| {
        let x0 = a
            .iter()
            .map(|&i| region.lines[i].x0)
            .fold(f32::MAX, f32::min);
        let x1 = a
            .iter()
            .map(|&i| region.lines[i].x1)
            .fold(f32::MIN, f32::max);
        x1 - x0
    };
    let (wl, wr) = (width(l), width(r));
    if !running(region, l) || !running(region, r) || wl.min(wr) < RUNNING_WIDTH * wl.max(wr) {
        return false;
    }
    let (sl, sr) = (starts(region, l), starts(region, r));
    let (few, many) = if sl.len() <= sr.len() {
        (sl, sr)
    } else {
        (sr, sl)
    };
    if few.is_empty() {
        return true;
    }
    let size = median(l.iter().map(|&i| region.lines[i].size).collect());
    let shared = few
        .iter()
        .filter(|a| many.iter().any(|b| (*b - **a).abs() <= 0.3 * size))
        .count();
    (shared as f32) < 0.5 * few.len() as f32
}

/// Dos partes con líneas alineadas fila a fila y al menos una de ellas con líneas
/// breves forman una tabla, no dos columnas de texto, salvo que sean dos columnas de
/// texto corrido (`running_columns`).
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
    let (mut l, mut r) = (lines(left), lines(right));
    if l.is_empty() || r.is_empty() || l.len() + r.len() < 3 {
        return false;
    }
    for part in [&mut l, &mut r] {
        part.sort_by(|&a, &b| region.lines[a].base.total_cmp(&region.lines[b].base));
    }
    if running_columns(region, &l, &r) {
        return false;
    }
    let zone = l.iter().chain(&r).map(|&i| region.lines[i].rect()).fold(
        Rect {
            x0: f32::MAX,
            top: f32::MAX,
            x1: f32::MIN,
            bottom: f32::MIN,
        },
        |a, b| Rect {
            x0: a.x0.min(b.x0),
            top: a.top.min(b.top),
            x1: a.x1.max(b.x1),
            bottom: a.bottom.max(b.bottom),
        },
    );
    if captioned_figure(region, zone) {
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
    pub(crate) fn new(line: Line) -> Self {
        let (marker, line) = match (!line.mono).then(|| list_marker(&line.text())).flatten() {
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

/// ¿Termina el texto en un número, en cifras o en romanos en minúscula (una página)?
fn ends_in_number(text: &str) -> bool {
    text.split_whitespace().next_back().is_some_and(|w| {
        w.chars().all(|c| c.is_ascii_digit())
            || w.chars().all(|c| matches!(c, 'i' | 'v' | 'x' | 'l' | 'c'))
    })
}

fn terminal(text: &str) -> bool {
    text.trim_end()
        .ends_with(['.', '!', '?', ':', ';', '…', '»', '"', '”'])
}

/// Bloques de una hoja en orden: párrafos, elementos de lista, tablas e imágenes.
/// Cada bloque lleva su rectángulo y la hoja de lectura de la que procede.
pub(crate) fn blocks(region: &Region, leaves: &[Leaf]) -> Vec<(Block, Rect, usize)> {
    let all: Vec<Item> = leaves
        .iter()
        .flat_map(|l| l.items.iter().copied())
        .collect();
    let mut in_table = vec![false; region.lines.len()];
    for leaf in leaves.iter().filter(|l| l.table) {
        for it in &leaf.items {
            if let Item::Line(k) = it {
                in_table[*k] = true;
            }
        }
    }
    let mut mono: Vec<usize> = all
        .iter()
        .filter_map(|i| match *i {
            Item::Line(k) if region.lines[k].mono && !in_table[k] => Some(k),
            _ => None,
        })
        .collect();
    mono.sort_by(|&a, &b| {
        region.lines[a]
            .base
            .total_cmp(&region.lines[b].base)
            .then(region.lines[a].x0.total_cmp(&region.lines[b].x0))
    });
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for i in mono {
        let line = &region.lines[i];
        let continued = groups.last().is_some_and(|g| {
            let prev = &region.lines[*g.last().unwrap()];
            let left = g
                .iter()
                .map(|&k| region.lines[k].x0)
                .fold(line.x0, f32::min);
            let barrier = region.lines.iter().any(|l| {
                !l.mono
                    && l.base > prev.base
                    && l.base < line.base
                    && l.x0 >= left - line.size
                    && l.x0 <= left + 2.5 * line.size
                    && l.x1 > left + 3.0 * line.size
            });
            (line.size - prev.size).abs() < 0.5
                && line.base - prev.base <= 4.5 * line.size
                && !barrier
        });
        if continued {
            groups.last_mut().unwrap().push(i);
        } else {
            groups.push(vec![i]);
        }
    }
    groups.retain(|g| g.len() >= 2);
    if groups.is_empty() {
        return flow_blocks(region, leaves);
    }
    let mut taken = vec![false; region.lines.len()];
    let mut bundles = Vec::new();
    for g in groups {
        let left = g
            .iter()
            .map(|&i| region.lines[i].x0)
            .fold(f32::MAX, f32::min);
        let size = region.lines[g[0]].size;
        let top = region.lines[g[0]].top();
        let bottom = region.lines[*g.last().unwrap()].bottom();
        let mut code_lines: Vec<Line> = Vec::new();
        for &i in &g {
            taken[i] = true;
            let line = &region.lines[i];
            if let Some(prev) = code_lines.last_mut()
                && (prev.base - line.base).abs() < 0.25 * size
            {
                let spaces = ((line.x0 - prev.x1) / (0.6 * size)).round().max(1.0) as usize;
                prev.runs.push(Run {
                    text: " ".repeat(spaces.min(256)),
                    bold: false,
                    italic: false,
                    link: None,
                });
                prev.runs.extend(line.runs.clone());
                prev.x1 = prev.x1.max(line.x1);
            } else {
                code_lines.push(line.clone());
            }
        }
        let pitch = median(
            code_lines
                .windows(2)
                .map(|w| w[1].base - w[0].base)
                .filter(|g| *g > 0.5 * size)
                .collect(),
        )
        .clamp(size, 1.5 * size);
        let mut previous = None;
        let mut previous_text = String::new();
        for line in &mut code_lines {
            let indent = ((line.x0 - left) / (0.6 * size)).round().max(0.0) as usize;
            let marker = region.lines.iter().any(|l| {
                matches!(l.text().trim(), "➥" | "↪") && (l.base - line.base).abs() < 0.5 * size
            });
            let wrapped = joins_wrapped_literal(&previous_text, &line.text())
                && previous.is_some_and(|base: f32| line.base - base <= 1.8 * pitch)
                && (marker || !indent.is_multiple_of(4));
            line.code = Some(CodePosition {
                indent,
                blank_lines: previous.map_or(0, |base: f32| {
                    ((line.base - base) / pitch).round().max(1.0) as usize - 1
                }),
                wrapped,
            });
            previous = Some(line.base);
            previous_text = line.text();
        }
        let mut code = Para::new(code_lines[0].clone());
        code.lines = code_lines;
        code.x0 = left;
        let rect = cell_rect(
            region,
            &g.iter().copied().map(Item::Line).collect::<Vec<_>>(),
        )
        .unwrap();
        let mut bundle = vec![(Block::Para(code), rect, CODE_FLOW)];
        let notes: Vec<Item> = all
            .iter()
            .filter_map(|i| match *i {
                Item::Line(k) if !taken[k] && !in_table[k] && !region.lines[k].mono => {
                    let l = &region.lines[k];
                    (l.base >= top
                        && l.top() <= bottom + 3.0 * size
                        && (l.x1 <= left + 0.25 * size || l.x0 > left + 4.0 * size))
                        .then_some(Item::Line(k))
                }
                _ => None,
            })
            .collect();
        for i in &notes {
            if let Item::Line(k) = i {
                taken[*k] = true;
            }
        }
        let note_leaves: Vec<Leaf> = reading_order(region, notes)
            .into_iter()
            .flat_map(|l| {
                if l.row {
                    l.cells.into_iter().map(Leaf::flow).collect()
                } else {
                    vec![l]
                }
            })
            .collect();
        bundle.extend(
            flow_blocks(region, &note_leaves)
                .into_iter()
                .map(|(b, r, _)| (b, r, CODE_NOTE)),
        );
        bundles.push((top, bundle));
    }
    let remaining = all
        .into_iter()
        .filter(|i| match *i {
            Item::Line(k) => !taken[k],
            _ => true,
        })
        .collect();
    let mut out = flow_blocks(region, &reading_order(region, remaining));
    // Un listado y sus notas forman una secuencia: las notas nunca interrumpen código.
    for (top, bundle) in bundles {
        let at = out
            .iter()
            .position(|(_, r, _)| r.top >= top)
            .unwrap_or(out.len());
        out.splice(at..at, bundle);
    }
    out
}

/// Tablas de diapositivas delimitadas por reglas pintadas. Los anclajes salen de
/// columnas que reaparecen en varias bandas; los controles vacíos fijan columnas
/// de respuesta sin inventar selecciones. Fuera de esas reglas no se adivinan tablas.
pub(crate) fn slide_blocks(
    region: &Region,
    rules: &[Rect],
    controls: &[Rect],
    width: f32,
    height: f32,
) -> Vec<(Block, Rect, usize)> {
    let mut candidates: Vec<_> = rules
        .iter()
        .copied()
        .filter(|r| r.width() >= 0.25 * width)
        .collect();
    candidates.sort_by(|a, b| b.width().total_cmp(&a.width()));
    let mut taken = vec![false; region.lines.len()];
    let mut tables = Vec::new();
    for wide in candidates {
        let full = rules
            .iter()
            .filter(|r| (r.x0 - wide.x0).abs() < 2. && (r.x1 - wide.x1).abs() < 2.)
            .count();
        if full < 3 {
            continue;
        }
        let mut ys: Vec<_> = rules
            .iter()
            .filter(|r| {
                r.x0 >= wide.x0 - 2. && r.x1 <= wide.x1 + 2. && r.width() >= 0.4 * wide.width()
            })
            .map(|r| r.top)
            .collect();
        ys.sort_by(f32::total_cmp);
        ys.dedup_by(|a, b| (*a - *b).abs() < 1.);
        let (first, last) = (*ys.first().unwrap(), *ys.last().unwrap());
        let inside = |l: &Line| l.x0 >= wide.x0 - 2. && l.x1 <= wide.x1 + 3. && !l.mono;
        let band = |y: f32| ys.partition_point(|&p| p < y);
        let mut anchors: Vec<(f32, std::collections::HashSet<usize>)> = Vec::new();
        let mut positions: Vec<_> = region
            .lines
            .iter()
            .filter(|l| inside(l) && l.top() > first && l.bottom() < last)
            .map(|l| (l.x0, band(l.base)))
            .collect();
        positions.extend(
            controls
                .iter()
                .filter(|r| {
                    r.x0 >= wide.x0 && r.x1 <= wide.x1 + 3. && r.top > first && r.bottom < last
                })
                .map(|r| (r.x0, band(r.top))),
        );
        positions.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (x, b) in positions {
            if let Some((left, rows)) = anchors.last_mut()
                && x - *left < 3.
            {
                rows.insert(b);
            } else {
                anchors.push((x, std::collections::HashSet::from([b])));
            }
        }
        let mut cols: Vec<_> = anchors
            .into_iter()
            .filter(|(_, rows)| rows.len() >= 2)
            .map(|(x, _)| x)
            .collect();
        if cols.len() < 2 {
            continue;
        }
        let pitch = cols
            .windows(2)
            .map(|w| w[1] - w[0])
            .fold(f32::MAX, f32::min);
        for x in &mut cols {
            let text_anchor = region
                .lines
                .iter()
                .any(|l| (l.x0 - *x).abs() < 3. && l.top() > first && l.bottom() < last);
            if !text_anchor
                && controls.iter().any(|r| (r.x0 - *x).abs() < 3.)
                && let Some(left) = region
                    .lines
                    .iter()
                    .filter(|l| {
                        inside(l)
                            && l.top() > 0.12 * height
                            && l.bottom() < first
                            && ((l.x0 + l.x1) / 2. - *x).abs() < 0.5 * pitch
                    })
                    .map(|l| l.x0)
                    .min_by(f32::total_cmp)
            {
                *x = left;
            }
        }
        let col_of = |l: &Line| cols.partition_point(|&x| x <= l.x0 + 3.).saturating_sub(1);
        let fits = |l: &Line| {
            let col = col_of(l);
            inside(l) && (col + 1 == cols.len() || l.x1 <= cols[col + 1] + 1.)
        };
        let eligible: Vec<_> = region
            .lines
            .iter()
            .enumerate()
            .filter(|(_, l)| fits(l) && l.top() > 0.12 * height && l.bottom() < height * 0.95)
            .collect();
        let Some(top) = eligible
            .iter()
            .filter(|(_, l)| l.bottom() <= first)
            .map(|(_, l)| l.top())
            .min_by(f32::total_cmp)
        else {
            continue;
        };
        let size = median(
            eligible
                .iter()
                .filter(|(_, l)| l.top() > first && l.bottom() < last)
                .map(|(_, l)| l.size)
                .collect(),
        )
        .max(4.);
        let mut bottom = last;
        let mut after: Vec<_> = eligible.iter().filter(|(_, l)| l.top() >= last).collect();
        after.sort_by(|a, b| a.1.top().total_cmp(&b.1.top()));
        for (_, l) in after {
            if l.top() - bottom > 3. * size {
                break;
            }
            bottom = bottom.max(l.bottom());
        }
        if region
            .lines
            .iter()
            .enumerate()
            .any(|(i, l)| taken[i] && inside(l) && l.top() >= top && l.bottom() <= bottom)
        {
            continue;
        }
        // Una cabecera separada de la primera fila por espacio vertical debe ser
        // una fila propia, aunque falte una regla dibujada debajo de ella.
        let mut initial: Vec<_> = eligible
            .iter()
            .filter(|(_, l)| l.top() >= top && l.bottom() <= first)
            .map(|(_, l)| (l.top(), l.bottom()))
            .collect();
        initial.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut reach = top;
        let mut gap = (0., 0.);
        for (a, b) in initial {
            if a - reach > gap.0 {
                gap = (a - reach, (a + reach) / 2.);
            }
            reach = reach.max(b);
        }
        if gap.0 > 0.45 * size {
            ys.push(gap.1);
        }
        ys.retain(|&y| y > top && y < bottom);
        ys.sort_by(f32::total_cmp);
        ys.dedup_by(|a, b| (*a - *b).abs() < 1.);
        let mut rows = vec![vec![String::new(); cols.len()]; ys.len() + 1];
        let mut selected: Vec<_> = region
            .lines
            .iter()
            .enumerate()
            .filter(|(i, l)| {
                !taken[*i] && inside(l) && l.top() >= top - 0.1 && l.bottom() <= bottom + 0.1
            })
            .collect();
        selected.sort_by(|a, b| {
            a.1.base
                .total_cmp(&b.1.base)
                .then(a.1.x0.total_cmp(&b.1.x0))
        });
        let mut indices = Vec::new();
        for (i, l) in selected {
            if !fits(l) && l.bottom() > first {
                continue;
            }
            let row = ys.partition_point(|&y| y < l.base);
            let col = col_of(l);
            let cell = &mut rows[row][col];
            if !cell.is_empty() {
                cell.push('\n');
            }
            cell.push_str(&l.text());
            indices.push(i);
        }
        // Una regla que empieza después de una columna deja esa celda combinada
        // verticalmente. Markdown no tiene rowspan: repetimos su etiqueta.
        for r in 1..rows.len() {
            let boundary = ys[r - 1];
            if rows[r][0].is_empty()
                && !rows[r - 1][0].is_empty()
                && rules.iter().any(|line| {
                    (line.top - boundary).abs() < 1.
                        && line.x0 > cols[0] + 3.
                        && line.x0 <= cols[1] + 3.
                })
                && !rules
                    .iter()
                    .any(|line| (line.top - boundary).abs() < 1. && line.x0 <= cols[0] + 3.)
            {
                rows[r][0] = rows[r - 1][0].clone();
            }
        }
        rows.retain(|row| row.iter().any(|c| !c.is_empty()));
        if rows.len() < 3 {
            continue;
        }
        for row in &mut rows {
            for cell in row {
                *cell = join_runs(&[Run {
                    text: std::mem::take(cell),
                    bold: false,
                    italic: false,
                    link: None,
                }])
                .0
                .into_iter()
                .map(|r| r.text)
                .collect();
            }
        }
        for i in indices {
            taken[i] = true;
        }
        tables.push((
            Block::Table(rows),
            Rect {
                x0: wide.x0,
                x1: wide.x1,
                top,
                bottom,
            },
            0,
        ));
    }
    let items = (0..region.lines.len())
        .filter(|&i| !taken[i])
        .map(Item::Line)
        .chain((0..region.pictures.len()).map(Item::Picture))
        .collect();
    let leaves: Vec<_> = reading_order(region, items)
        .into_iter()
        .flat_map(|l| {
            if l.row {
                l.cells.into_iter().map(Leaf::flow).collect::<Vec<_>>()
            } else {
                vec![l]
            }
        })
        .collect();
    let mut out = blocks(region, &leaves);
    tables.sort_by(|a, b| a.1.top.total_cmp(&b.1.top));
    for table in tables {
        let at = out
            .iter()
            .position(|(_, r, _)| r.top >= table.1.top)
            .unwrap_or(out.len());
        out.insert(at, table);
    }
    out
}

/// Un salto editorial dentro de un literal simple: la segunda parte no abre otro
/// literal y cierra la comilla que quedó abierta. Nunca modifica cadenas triples ni
/// una continuación explícita con barra inversa.
fn joins_wrapped_literal(previous: &str, next: &str) -> bool {
    if previous.ends_with('\\')
        || [previous, next]
            .iter()
            .any(|s| s.contains("\"\"\"") || s.contains("'''"))
    {
        return false;
    }
    let mut quote = None;
    let mut escaped = false;
    for c in previous.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' {
            escaped = true;
            continue;
        }
        if matches!(c, '\'' | '"') {
            if quote == Some(c) {
                quote = None;
            } else if quote.is_none() {
                quote = Some(c);
            }
        }
    }
    quote.is_some_and(|q| !next.trim_start().starts_with(['\'', '"']) && next.contains(q))
}

fn flow_blocks(region: &Region, leaves: &[Leaf]) -> Vec<(Block, Rect, usize)> {
    let mut out: Vec<(Block, Rect, usize)> = Vec::new();
    let mut k = 0;
    while k < leaves.len() {
        let leaf = &leaves[k];
        // Filas consecutivas de una tabla de contenido: número, título y página.
        if let Some((rows, rect, used)) = contents_run(region, leaves, k) {
            out.push((Block::Table(rows), rect, k));
            k = used + 1;
            continue;
        }
        // Filas consecutivas de varias celdas de texto: tabla.
        let text_row = |l: &Leaf| l.row && l.items.iter().all(|i| matches!(i, Item::Line(_)));
        if text_row(leaf)
            && let Some(next) = leaves.get(k + 1).filter(|l| text_row(l))
            && let Some(cols) = table_columns(region, leaf, next)
        {
            let (rows, rect, used) = table(region, leaves, k, &cols);
            // Los rótulos de una figura con pie se escriben celda a celda; una sola fila
            // lógica, por columnas: no hay tabla donde no hay filas que comparar.
            if let Some(columns) = column_leaves(region, &leaves[k..=used], &cols)
                && single_row(&rows)
                && (rows.len() <= 1 || normal_leading(region, &columns))
            {
                out.extend(
                    flow_blocks(region, &columns)
                        .into_iter()
                        .map(|(b, r, _)| (b, r, k)),
                );
                k = used + 1;
                continue;
            }
            if !captioned_figure(region, rect) {
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
        // Borde de un texto justificado: donde terminan al menos la mitad de las líneas.
        let justified = lines
            .iter()
            .map(|&i| {
                let l = &region.lines[i];
                let n = lines
                    .iter()
                    .filter(|&&j| (region.lines[j].x1 - l.x1).abs() <= 0.3 * l.size)
                    .count();
                (n, l.x1)
            })
            .max_by(|a, b| a.0.cmp(&b.0).then(b.1.total_cmp(&a.1)))
            .filter(|(n, _)| lines.len() >= 4 && n * 2 >= lines.len())
            .map(|(_, x1)| x1);
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
                && continues(p, line, right, justified, usual_gap)
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
fn continues(p: &Para, line: &Line, right: f32, justified: Option<f32>, usual_gap: f32) -> bool {
    let prev = p.last();
    let text = line.text();
    if list_marker(&text).is_some() || line.mono != p.mono {
        return false;
    }
    if (line.size - prev.size).abs() > 0.15 * prev.size {
        return false;
    }
    let prev_text = prev.text();
    // En texto justificado, la línea que llega a su borde está llena aunque la hoja sea
    // más ancha (un título a todo el ancho sobre la columna).
    let full = prev.x1 >= right - prev.size
        || justified.is_some_and(|edge| (prev.x1 - edge).abs() <= 0.3 * prev.size);
    // La negrita que cambia entre dos líneas corta el párrafo, salvo que la anterior
    // llegue al margen sin cerrar la frase y la nueva empiece en minúscula: es un
    // énfasis que sigue en la línea nueva.
    if line.bold != prev.bold
        && (p.mono
            || terminal(&prev_text)
            || !full
            || !text.trim_start().starts_with(char::is_lowercase))
    {
        return false;
    }
    // Un título numerado breve («2 Related Work») no se une a la línea siguiente de
    // otro tamaño.
    if p.lines.len() == 1
        && p.marker.is_none()
        && (line.size - prev.size).abs() > 0.5
        && prev.x1 < right - 3.0 * prev.size
        && prev_text
            .split_whitespace()
            .next()
            .is_some_and(section_number)
    {
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
    let short = prev.x1 < right - 3.0 * prev.size && !full;
    if terminal(&prev_text) && (short || line.x0 > prev.x0 + 0.8 * prev.size) && p.marker.is_none()
    {
        return false;
    }
    true
}

/// Tabla de las filas `from..=to`; añade a la celda de su columna las líneas sueltas
/// que la continúan justo debajo. Devuelve las filas, el rectángulo y la última hoja
/// usada.
fn cell_rect(region: &Region, cell: &[Item]) -> Option<Rect> {
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
}

/// Dos filas vecinas fijan un número estable de columnas. Una leyenda o encabezado
/// que las cruza no puede ensancharlas hasta fundirlas.
fn table_columns(region: &Region, a: &Leaf, b: &Leaf) -> Option<Vec<(f32, f32)>> {
    if a.cells.len() < 2 || a.cells.len() != b.cells.len() {
        return None;
    }
    let mut cols = Vec::new();
    let size = median(a.items.iter().map(|&i| region.size(i)).collect()).max(4.0);
    for (a, b) in a.cells.iter().zip(&b.cells) {
        let (a, b) = (cell_rect(region, a)?, cell_rect(region, b)?);
        if b.top - a.bottom > 2.5 * size {
            return None;
        }
        cols.push((a.x0.min(b.x0), a.x1.max(b.x1)));
    }
    cols.windows(2)
        .all(|w| w[0].1 + 1.0 < w[1].0)
        .then_some(cols)
}

/// Filas mínimas de una tabla de contenido.
const CONTENTS_ROWS: usize = 3;
/// Separación máxima, en tamaños de letra, entre dos filas seguidas de un contenido.
const CONTENTS_PITCH: f32 = 3.5;

/// ¿Es el texto un número de página, en cifras o en romanos en minúscula?
fn page_number(t: &str) -> bool {
    let t = t.trim();
    !t.is_empty()
        && t.chars().count() <= 5
        && (t.chars().all(|c| c.is_ascii_digit()) || t.chars().all(|c| "ivxlc".contains(c)))
}

/// ¿Es el texto el número de un encabezado («2», «2.3») o la letra de un apéndice
/// («A», «B.2»)?
fn heading_label(t: &str) -> bool {
    section_number(t)
        || t.split_once('.').map_or(t, |(a, _)| a).len() == 1
            && t.starts_with(|c: char| c.is_ascii_uppercase())
            && (t.len() == 1 || section_number(&t[2..]) && t[1..].starts_with('.'))
}

/// ¿Es el texto un número de sección o de capítulo («2», «2.3», «10»)?
fn section_number(t: &str) -> bool {
    let t = t.trim();
    t.chars().count() <= 7
        && t.starts_with(|c: char| c.is_ascii_digit())
        && t.ends_with(|c: char| c.is_ascii_digit())
        && t.chars().all(|c| c.is_ascii_digit() || c == '.')
}

/// Celdas de una fila de contenido, con una sola línea cada una: `[número, título,
/// página]` o `[título, página]`; el número se deja vacío si falta.
fn contents_cells(region: &Region, leaf: &Leaf) -> Option<[String; 3]> {
    if !leaf.row || leaf.cells.iter().any(|c| c.len() != 1) {
        return None;
    }
    let text: Vec<String> = leaf
        .cells
        .iter()
        .map(|c| match c[0] {
            Item::Line(i) => Some(region.lines[i].text().trim().to_string()),
            Item::Picture(_) => None,
        })
        .collect::<Option<_>>()?;
    let titled = |t: &str| t.chars().any(char::is_alphabetic) && !page_number(t);
    match text.as_slice() {
        [number, title, page] if section_number(number) && titled(title) && page_number(page) => {
            Some([number.clone(), title.clone(), page.clone()])
        }
        [title, page] if titled(title) && !section_number(title) && page_number(page) => {
            Some([String::new(), title.clone(), page.clone()])
        }
        _ => None,
    }
}

/// Una tabla de contenido que empieza en la hoja `from`: al menos `CONTENTS_ROWS` filas
/// seguidas de número, título y página, donde la página sigue al título a la distancia
/// que deja su longitud. Las columnas de dos filas vecinas se solapan, así que se
/// reconocen por su papel y no por su posición. Solo si todas las filas con forma de
/// entrada de la página están en la racha. Devuelve las filas, el rectángulo y la última
/// hoja usada.
fn contents_run(
    region: &Region,
    leaves: &[Leaf],
    from: usize,
) -> Option<(Vec<Vec<String>>, Rect, usize)> {
    let mut rows: Vec<[String; 3]> = Vec::new();
    let mut rect: Option<Rect> = None;
    let mut last = from;
    for (k, leaf) in leaves.iter().enumerate().skip(from) {
        let Some(cells) = contents_cells(region, leaf) else {
            break;
        };
        let r = cell_rect(region, &leaf.items)?;
        if let Some(b) = rect
            && r.top - b.bottom > CONTENTS_PITCH * region.size(leaf.items[0]).max(4.0)
        {
            break;
        }
        rect = Some(match rect {
            None => r,
            Some(o) => Rect {
                x0: o.x0.min(r.x0),
                top: o.top.min(r.top),
                x1: o.x1.max(r.x1),
                bottom: o.bottom.max(r.bottom),
            },
        });
        rows.push(cells);
        last = k;
    }
    // Solo una página cuyo contenido es una sola lista de entradas: si otras filas con
    // forma de entrada quedan fuera de la racha (los subapartados de un contenido
    // detallado la interrumpen), un tramo suelto sería un fragmento de tabla.
    let entries = leaves
        .iter()
        .filter(|l| contents_cells(region, l).is_some())
        .count();
    if rows.len() < CONTENTS_ROWS || rows.len() != entries {
        return None;
    }
    let numbered = rows.iter().any(|r| !r[0].is_empty());
    let rows = rows
        .into_iter()
        .map(|r| {
            if numbered {
                r.to_vec()
            } else {
                r[1..].to_vec()
            }
        })
        .collect();
    Some((rows, rect?, last))
}

/// ¿Sigue la fila `row` el texto de la fila de `above`? Toda celda con texto empieza en
/// minúscula o con paréntesis bajo una celda que no cierra la frase (rótulos de un
/// diagrama o celdas de varias líneas). Una fila con un número, o bajo uno (las páginas
/// de un índice, en cifras o en romanos), es una entrada propia.
fn continues_row(above: &[String], row: &[String]) -> bool {
    row.iter().any(|t| !t.is_empty())
        && !row.iter().chain(above).any(|t| ends_in_number(t))
        && row.iter().zip(above).all(|(t, a)| {
            t.is_empty()
                || (!a.is_empty()
                    && !terminal(a)
                    && t.starts_with(|c: char| c.is_lowercase() || c == '('))
        })
}

/// ¿Forman las filas una sola fila lógica? Una fila sola, o cada una continuando el texto
/// de la anterior: no hay filas que comparar y los rótulos, que se parten en varias
/// líneas, no son una tabla.
fn single_row(rows: &[Vec<String>]) -> bool {
    rows.len() <= 1 || rows.windows(2).all(|w| continues_row(&w[0], &w[1]))
}

/// ¿Se siguen las líneas de cada columna con el interlineado normal, sin el hueco que
/// separa las filas de una tabla? Entonces cada columna es un párrafo partido en líneas.
fn normal_leading(region: &Region, columns: &[Leaf]) -> bool {
    columns.iter().all(|leaf| {
        let mut lines: Vec<&Line> = leaf
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Line(k) => Some(&region.lines[*k]),
                Item::Picture(_) => None,
            })
            .collect();
        lines.sort_by(|a, b| a.base.total_cmp(&b.base));
        lines
            .windows(2)
            .all(|w| w[1].base - w[0].base <= 1.6 * w[0].size.max(w[1].size))
    })
}

/// Las hojas de una tabla leídas por columnas: los elementos de cada columna, en orden,
/// como una hoja de texto corrido. `None` si algún elemento no cae en una columna.
fn column_leaves(region: &Region, leaves: &[Leaf], cols: &[(f32, f32)]) -> Option<Vec<Leaf>> {
    let edges: Vec<f32> = cols.windows(2).map(|w| w[1].0 - 1.0).collect();
    let col_of = |r: &Rect| {
        (0..cols.len()).find(|&i| {
            (i == 0 || r.x0 >= edges[i - 1] - 1.0)
                && (i == cols.len() - 1 || r.x1 <= edges[i] + 1.0)
        })
    };
    let mut per: Vec<Vec<Item>> = vec![Vec::new(); cols.len()];
    for it in leaves.iter().flat_map(|l| l.items.iter().copied()) {
        per[col_of(&region.rect(it))?].push(it);
    }
    Some(
        per.into_iter()
            .filter(|v| !v.is_empty())
            .map(Leaf::flow)
            .collect(),
    )
}

fn table(
    region: &Region,
    leaves: &[Leaf],
    from: usize,
    cols: &[(f32, f32)],
) -> (Vec<Vec<String>>, Rect, usize) {
    let edges: Vec<f32> = cols.windows(2).map(|w| w[1].0 - 1.0).collect();
    let col_of = |r: &Rect| -> Option<usize> {
        (0..cols.len()).find(|&i| {
            (i == 0 || r.x0 >= edges[i - 1] - 1.0)
                && (i == cols.len() - 1 || r.x1 <= edges[i] + 1.0)
        })
    };
    let text_of = |it: Item| match it {
        Item::Line(i) => region.lines[i].text(),
        Item::Picture(_) => String::new(),
    };
    let body_tops: Vec<_> = leaves[from + 1..]
        .iter()
        .take_while(|l| l.table)
        .filter(|l| l.cells.len() == cols.len())
        .filter_map(|l| cell_rect(region, &l.items).map(|r| r.top))
        .collect();
    let multiline = body_tops
        .windows(2)
        .filter(|w| w[1] - w[0] > 1.7 * region.size(leaves[from].items[0]))
        .count()
        >= 2;
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
    let append = |cell: &mut String, text: &str| {
        if !cell.is_empty() {
            cell.push('\n');
        }
        cell.push_str(text);
    };
    let mut last = from;
    let mut k = from;
    let mut previous_top = None;
    while k < leaves.len() {
        let leaf = &leaves[k];
        let is_row = leaf.row && leaf.items.iter().all(|i| matches!(i, Item::Line(_)));
        if leaf.row && !is_row {
            break;
        }
        let current = cell_rect(region, &leaf.items);
        if k > from
            && current.is_some_and(|r| {
                r.top - rect.map_or(0.0, |b| b.bottom)
                    > (if leaf.table { 12.0 } else { 2.5 }) * region.size(leaf.items[0]).max(4.0)
            })
        {
            break;
        }
        if is_row {
            let cells: Vec<_> = leaf
                .cells
                .iter()
                .filter_map(|c| cell_rect(region, c))
                .collect();
            if !cells.iter().all(|r| col_of(r).is_some()) {
                break;
            }
            let mut row = vec![String::new(); cols.len()];
            for c in &leaf.cells {
                let Some(r) = cell_rect(region, c) else {
                    continue;
                };
                grow(&mut rect, r);
                let cell = &mut row[col_of(&r).expect("celda en su columna")];
                for &it in c {
                    append(cell, &text_of(it));
                }
            }
            let size = region.size(leaf.items[0]);
            let close = |limit: f32| {
                current.is_some_and(|r| previous_top.is_some_and(|p| r.top - p <= limit * size))
            };
            // Renglones que siguen el texto de la fila de arriba con interlineado normal:
            // toda celda con texto empieza en minúscula o con paréntesis bajo una celda que
            // no cierra la frase (rótulos de un diagrama o celdas de varias líneas). Una
            // fila con un número, o bajo uno (las páginas de un índice, en cifras o en
            // romanos), es una entrada propia.
            let continues = rows
                .last()
                .is_some_and(|above: &Vec<String>| continues_row(above, &row));
            if !rows.is_empty()
                && (row[0].is_empty() || (multiline && close(1.4)) || (continues && close(1.35)))
            {
                for (cell, text) in rows.last_mut().unwrap().iter_mut().zip(row) {
                    if !text.is_empty() {
                        append(cell, &text);
                    }
                }
            } else {
                rows.push(row);
            }
        } else {
            // Línea suelta dentro de una columna, pegada a la fila anterior.
            let bottom = rect.map_or(0.0, |r| r.bottom);
            let fits = leaf.items.len() == 1 && matches!(leaf.items[0], Item::Line(_)) && {
                let r = region.rect(leaf.items[0]);
                col_of(&r).is_some() && r.top - bottom <= 0.75 * region.size(leaf.items[0])
            };
            if !fits {
                break;
            }
            let r = region.rect(leaf.items[0]);
            grow(&mut rect, r);
            if let Some(row) = rows.last_mut() {
                let cell = &mut row[col_of(&r).expect("continuación en su columna")];
                append(cell, &text_of(leaf.items[0]));
            }
        }
        last = k;
        previous_top = current.map(|r| r.top);
        k += 1;
    }
    let rect = rect.unwrap_or(Rect {
        x0: 0.0,
        top: 0.0,
        x1: 0.0,
        bottom: 0.0,
    });
    for cell in rows.iter_mut().flatten() {
        *cell = join_runs(&[Run {
            text: std::mem::take(cell),
            bold: false,
            italic: false,
            link: None,
        }])
        .0
        .into_iter()
        .map(|r| r.text)
        .collect();
    }
    (rows, rect, last)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ruled_scale_uses_control_columns_and_preserves_wrapped_headers_and_merged_labels() {
        let mut glyphs = Vec::new();
        let mut controls = Vec::new();
        let choices = ["Always", "Often", "Sometimes", "Rarely", "Never"];
        for (i, text) in choices.iter().enumerate() {
            glyphs.extend(text_at(text, 350. + 90. * i as f32, 100., 8., true));
        }
        for row in 0..5 {
            let y = 140. + 40. * row as f32;
            if row != 1 {
                glyphs.extend(text_at(
                    if row == 0 { "Group one" } else { "Other group" },
                    40.,
                    y,
                    8.,
                    true,
                ));
            }
            glyphs.extend(text_at("A row description", 170., y, 8., false));
            for i in 0..5 {
                controls.push(Rect {
                    x0: 370. + 90. * i as f32,
                    x1: 385. + 90. * i as f32,
                    top: y - 9.,
                    bottom: y + 6.,
                });
            }
        }
        let rules = vec![
            Rect {
                x0: 170.,
                x1: 820.,
                top: 170.,
                bottom: 170.,
            },
            Rect {
                x0: 40.,
                x1: 820.,
                top: 210.,
                bottom: 210.,
            },
            Rect {
                x0: 40.,
                x1: 820.,
                top: 250.,
                bottom: 250.,
            },
            Rect {
                x0: 40.,
                x1: 820.,
                top: 290.,
                bottom: 290.,
            },
        ];
        let (lines, _) = lines(&glyphs);
        let region = Region {
            lines: &lines,
            pictures: &[],
        };
        let output = slide_blocks(&region, &rules, &controls, 1000., 600.);
        let tables: Vec<_> = output
            .iter()
            .filter_map(|(b, _, _)| {
                if let Block::Table(rows) = b {
                    Some(rows)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(tables.len(), 1, "{output:?}");
        let table = tables[0];
        assert_eq!(table.len(), 6, "{table:?}");
        assert_eq!(&table[0][2..], choices);
        assert_eq!(table[1][0], "Group one");
        assert_eq!(table[2][0], "Group one");
        for row in &table[1..] {
            assert_eq!(row[1], "A row description");
        }
        let without_rules = slide_blocks(&region, &[], &controls, 1000., 600.);
        assert!(
            !without_rules
                .iter()
                .any(|(b, _, _)| matches!(b, Block::Table(_)))
        );
    }

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

    fn code_at(s: &str, x: f32, base: f32) -> Vec<Glyph> {
        let mut glyphs = text_at(s, x, base, 8.0, false);
        for (i, g) in glyphs.iter_mut().enumerate() {
            g.mono = true;
            g.x0 = x + i as f32 * 4.8;
            g.x1 = g.x0 + 4.8;
        }
        glyphs
    }

    #[test]
    fn three_columns_keep_multiline_cells_and_exclude_the_caption() {
        let mut g = text_at("Informe", 220.0, 30.0, 9.0, true);
        g.extend(text_at("Tabla 2", 40.0, 60.0, 8.0, true));
        g.extend(text_at(
            "Descripción general de los registros",
            90.0,
            60.0,
            8.0,
            true,
        ));
        for (text, x) in [("Clave", 45.0), ("Petición", 180.0), ("Respuesta", 330.0)] {
            g.extend(text_at(text, x, 85.0, 8.0, true));
        }
        for (i, label) in ["Uno", "Dos", "Tres", "Cuatro"].iter().enumerate() {
            let base = 105.0 + i as f32 * 27.0;
            g.extend(text_at(label, 40.0, base, 8.0, false));
            g.extend(text_at(
                "Una petición de longitud considerable",
                140.0,
                base,
                8.0,
                false,
            ));
            g.extend(text_at(
                "Devuelve una respuesta con docu-",
                310.0,
                base,
                8.0,
                false,
            ));
            g.extend(text_at(
                "continuación de la petición",
                140.0,
                base + 10.0,
                8.0,
                false,
            ));
            g.extend(text_at("mentos", 310.0, base + 10.0, 8.0, false));
        }
        let (ls, _) = lines(&g);
        let region = Region {
            lines: &ls,
            pictures: &[],
        };
        let leaves = reading_order(&region, (0..ls.len()).map(Item::Line).collect());
        let result = blocks(&region, &leaves);
        let tables: Vec<_> = result
            .iter()
            .filter_map(|(b, _, _)| match b {
                Block::Table(t) => Some(t),
                _ => None,
            })
            .collect();
        assert_eq!(tables.len(), 1, "{result:?}");
        assert_eq!(tables[0].len(), 5);
        assert_eq!(tables[0][0], ["Clave", "Petición", "Respuesta"]);
        for row in &tables[0][1..] {
            assert_eq!(row.len(), 3);
            assert!(row[1].ends_with("continuación de la petición"));
            assert_eq!(row[2], "Devuelve una respuesta con documentos");
        }
        assert!(
            result
                .iter()
                .any(|(b, _, _)| matches!(b,Block::Para(p) if p.text().contains("Tabla 2")))
        );
    }

    #[test]
    fn code_keeps_spaces_indentation_and_blank_lines_outside_side_notes() {
        let mut g = Vec::new();
        for (text, base) in [
            ("def plan():", 100.0),
            ("    value = \"a  b\"", 120.0),
            ("    return value", 130.0),
            ("print(plan())", 140.0),
        ] {
            g.extend(code_at(text, 50.0, base));
        }
        g.extend(text_at("Nota lateral", 220.0, 113.0, 9.0, true));
        g.extend(text_at("sobre el valor", 220.0, 123.0, 9.0, true));
        g.extend(text_at(
            "Texto que sigue al listado.",
            50.0,
            175.0,
            10.0,
            false,
        ));
        let (ls, _) = lines(&g);
        let region = Region {
            lines: &ls,
            pictures: &[],
        };
        let result = blocks(
            &region,
            &reading_order(&region, (0..ls.len()).map(Item::Line).collect()),
        );
        let code: Vec<_> = result
            .iter()
            .filter_map(|(b, _, _)| match b {
                Block::Para(p) if p.mono => Some(p),
                _ => None,
            })
            .collect();
        assert_eq!(code.len(), 1, "{result:?}");
        assert_eq!(
            code[0]
                .lines
                .iter()
                .map(|l| l.code.unwrap().indent)
                .collect::<Vec<_>>(),
            [0, 4, 4, 0]
        );
        assert_eq!(code[0].lines[1].code.unwrap().blank_lines, 1);
        assert_eq!(code[0].lines[1].text(), "value = \"a  b\"");
        assert!(result.iter().any(|(b, _, k)| *k == CODE_NOTE
            && matches!(b,Block::Para(p) if p.text().contains("Nota lateral"))));
        assert!(
            matches!(&result.last().unwrap().0,Block::Para(p) if p.text()=="Texto que sigue al listado.")
        );
    }

    #[test]
    fn tall_table_rows_keep_monospace_fragments_inside_their_cells() {
        let mut g = Vec::new();
        for (text, x) in [("Clave", 40.0), ("Ejemplo", 180.0), ("Notas", 330.0)] {
            g.extend(text_at(text, x, 80.0, 8.0, true));
        }
        for (i, label) in ["Uno", "Dos", "Tres"].iter().enumerate() {
            let base = 100.0 + i as f32 * 60.0;
            g.extend(text_at(label, 40.0, base, 8.0, false));
            g.extend(text_at(
                "Parámetros que deben conservarse",
                140.0,
                base,
                8.0,
                false,
            ));
            g.extend(text_at(
                "Descripción explicativa de esta fila",
                310.0,
                base,
                8.0,
                false,
            ));
            for j in 1..=3 {
                g.extend(text_at(
                    "más información de la misma celda",
                    310.0,
                    base + j as f32 * 10.0,
                    8.0,
                    false,
                ));
            }
            g.extend(code_at("alpha,", 140.0, base + 10.0));
            g.extend(text_at("continuación", 40.0, base + 10.0, 8.0, false));
            g.extend(code_at("beta", 140.0, base + 20.0));
            g.extend(text_at(
                "Una continuación más larga que el inicio",
                140.0,
                base + 30.0,
                8.0,
                false,
            ));
        }
        let (ls, _) = lines(&g);
        let region = Region {
            lines: &ls,
            pictures: &[],
        };
        let result = blocks(
            &region,
            &reading_order(&region, (0..ls.len()).map(Item::Line).collect()),
        );
        let tables: Vec<_> = result
            .iter()
            .filter_map(|(b, _, _)| match b {
                Block::Table(t) => Some(t),
                _ => None,
            })
            .collect();
        assert_eq!(tables.len(), 1, "{result:?}");
        assert_eq!(tables[0].len(), 4);
        assert_eq!(tables[0][1][0], "Uno continuación");
        assert!(tables[0][3][1].ends_with("Una continuación más larga que el inicio"));
        assert_eq!(tables[0][3][2].matches("más información").count(), 3);
        assert!(tables[0][1][1].contains("alpha, beta"));
        assert!(
            !result
                .iter()
                .any(|(b, _, _)| matches!(b,Block::Para(p) if p.mono))
        );
    }

    #[test]
    fn only_editorial_wraps_in_simple_literals_are_joined() {
        assert!(joins_wrapped_literal(
            "text = \"primera parte",
            "segunda parte\","
        ));
        assert!(!joins_wrapped_literal(
            "text = \"\"\"primera",
            "segunda\"\"\""
        ));
        assert!(!joins_wrapped_literal("text = \"primera\\", "segunda\""));
        assert!(!joins_wrapped_literal(
            "text = (\"primera\"",
            "\"segunda\")"
        ));
        let mut g = code_at("text = \"primera parte", 50.0, 100.0);
        g.extend(code_at("segunda parte\"", 83.6, 110.0));
        g.extend(code_at("print(text)", 50.0, 120.0));
        let (ls, _) = lines(&g);
        let region = Region {
            lines: &ls,
            pictures: &[],
        };
        let result = blocks(
            &region,
            &reading_order(&region, (0..ls.len()).map(Item::Line).collect()),
        );
        let code = result
            .iter()
            .find_map(|(b, _, _)| match b {
                Block::Para(p) if p.mono => Some(p),
                _ => None,
            })
            .unwrap();
        assert!(code.lines[1].code.unwrap().wrapped);
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
    fn a_heading_number_joins_its_title_but_not_the_next_line() {
        // «1» y su título, separados por un hueco de 1,5 tamaños, ambos en negrita.
        let mut g = text_at("1", 50.0, 40.0, 12.0, true);
        g.extend(text_at("Introduction", 74.0, 40.0, 12.0, true));
        g.extend(text_at(
            "Knowledge conflicts in retrieval.",
            50.0,
            58.0,
            11.0,
            true,
        ));
        g.extend(text_at(
            "El texto corriente sigue en la columna con su propio formato.",
            50.0,
            72.0,
            11.0,
            false,
        ));
        let got = order(&g);
        assert_eq!(got[0], "1 Introduction");
        assert_eq!(got[1], "Knowledge conflicts in retrieval.");
    }

    #[test]
    fn bold_emphasis_that_fills_a_line_does_not_split_the_paragraph() {
        let (a, b) = ("This leads to a question: ", "Can a decision model");
        let mut g = text_at(a, 50.0, 40.0, 10.0, false);
        g.extend(text_at(b, 50.0 + 5.0 * a.len() as f32, 40.0, 10.0, true));
        g.extend(text_at(
            "replace a dedicated reranker while meeting the",
            50.0,
            52.0,
            10.0,
            true,
        ));
        g.extend(text_at(
            "same goals? A model may fail.",
            50.0,
            64.0,
            10.0,
            false,
        ));
        assert_eq!(
            order(&g),
            [
                "This leads to a question: Can a decision model replace a dedicated reranker \
              while meeting the same goals? A model may fail."
            ]
        );
    }

    /// Glifo con la caja dada, como los entrega pdf_oxide.
    fn glyph_at(c: char, x0: f32, x1: f32, span: usize) -> Glyph {
        Glyph {
            c,
            x0,
            x1,
            base: 100.0,
            size: 11.5,
            bold: false,
            italic: false,
            mono: false,
            rotated: false,
            span,
            link: None,
        }
    }

    fn line_text(glyphs: &[Glyph]) -> String {
        let (lines, _) = lines(glyphs);
        lines
            .iter()
            .map(|l| l.text())
            .collect::<Vec<_>>()
            .join(" | ")
    }

    #[test]
    fn a_ligature_expanded_with_a_space_over_its_own_box_is_one_word() {
        // La ligadura «Th» llega como «T», «h» y un espacio con la caja de la «h»:
        // la siguiente letra empieza donde acaba esa caja, sin hueco.
        let mut g = vec![
            glyph_at('T', 117.72, 123.84, 16),
            glyph_at('h', 123.84, 129.96, 16),
            glyph_at(' ', 123.84, 129.96, 17),
            glyph_at('i', 129.96, 132.92, 17),
            glyph_at('s', 132.92, 136.63, 17),
            glyph_at(' ', 136.63, 139.51, 17),
        ];
        g.extend(
            "book"
                .chars()
                .enumerate()
                .map(|(k, c)| glyph_at(c, 139.51 + k as f32 * 5.6, 145.11 + k as f32 * 5.6, 17)),
        );
        assert_eq!(line_text(&g), "This book");
    }

    #[test]
    fn a_space_that_overlaps_the_next_letter_is_not_a_word_break() {
        // «T each»: el espacio mide más que el hueco real, y la «e» empieza dentro de él.
        let mut g = vec![
            glyph_at('T', 344.54, 348.83, 152),
            glyph_at(' ', 348.76, 353.99, 152),
        ];
        for (k, c) in "each".chars().enumerate() {
            g.push(glyph_at(
                c,
                349.52 + k as f32 * 3.9,
                353.42 + k as f32 * 3.9,
                152,
            ));
        }
        // La figura dibuja este texto en cuerpo 7.
        g.iter_mut().for_each(|x| x.size = 7.0);
        assert_eq!(line_text(&g), "Teach");
    }

    #[test]
    fn a_zero_width_space_between_touching_letters_is_kept() {
        // «Artificial Intelligence» en una tabla: las letras se tocan y el espacio mide
        // cero; marca una posición, no una ligadura.
        let mut g: Vec<Glyph> = "Artificia"
            .chars()
            .enumerate()
            .map(|(k, c)| glyph_at(c, 100.0 + k as f32 * 1.6, 101.6 + k as f32 * 1.6, 17))
            .collect();
        g.push(glyph_at('l', 114.4, 117.36, 17));
        g.push(glyph_at(' ', 117.36, 117.36, 17));
        for (k, c) in "Intelligence".chars().enumerate() {
            g.push(glyph_at(
                c,
                117.36 + k as f32 * 1.7,
                119.06 + k as f32 * 1.7,
                17,
            ));
        }
        assert_eq!(line_text(&g), "Artificial Intelligence");
    }

    #[test]
    fn a_narrow_space_that_fits_a_real_gap_keeps_the_words_apart() {
        // «matches movie»: hay un hueco real de 1,08 y el espacio cabe justo en él.
        let mut g: Vec<Glyph> = "matches"
            .chars()
            .enumerate()
            .map(|(k, c)| glyph_at(c, 424.0 + k as f32 * 3.6, 427.6 + k as f32 * 3.6, 71))
            .collect();
        g.push(glyph_at(' ', 449.22, 450.30, 71));
        for (k, c) in "movie".chars().enumerate() {
            g.push(glyph_at(
                c,
                450.30 + k as f32 * 4.0,
                454.30 + k as f32 * 4.0,
                71,
            ));
        }
        assert_eq!(line_text(&g), "matches movie");
    }

    #[test]
    fn a_real_space_keeps_the_words_apart() {
        // «PT Nexus»: el espacio avanza y la «N» empieza donde él termina.
        let mut g = vec![
            glyph_at('P', 191.08, 197.12, 34),
            glyph_at('T', 197.12, 204.38, 34),
            glyph_at(' ', 204.38, 207.13, 34),
        ];
        for (k, c) in "Nexus".chars().enumerate() {
            g.push(glyph_at(
                c,
                207.13 + k as f32 * 4.8,
                211.93 + k as f32 * 4.8,
                34,
            ));
        }
        assert_eq!(line_text(&g), "PT Nexus");
    }

    /// Fila de contenido: número, título y página, con la página pegada al título.
    fn contents_row(g: &mut Vec<Glyph>, base: f32, number: Option<&str>, title: &str, page: &str) {
        let mut x = 100.0;
        if let Some(n) = number {
            g.extend(text_at(n, x, base, 10.0, false));
        }
        x = 130.0;
        g.extend(text_at(title, x, base, 10.0, false));
        g.extend(text_at(
            page,
            x + title.chars().count() as f32 * 5.0 + 12.0,
            base,
            10.0,
            false,
        ));
    }

    #[test]
    fn contents_rows_with_trailing_page_numbers_form_a_table() {
        // El número de página sigue a cada título a distinta distancia: las columnas de
        // dos filas vecinas se solapan, pero cada fila es número, título y página.
        let mut g = Vec::new();
        contents_row(&mut g, 100.0, Some("1"), "Introduction to agents", "1");
        contents_row(
            &mut g,
            118.0,
            Some("2"),
            "Harnessing the power of large language models",
            "14",
        );
        contents_row(&mut g, 136.0, Some("3"), "Engaging GPT assistants", "39");
        contents_row(&mut g, 154.0, Some("10"), "Agent reasoning", "244");
        let out = order(&g);
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(
            out[0],
            r#"[["1", "Introduction to agents", "1"], ["2", "Harnessing the power of large language models", "14"], ["3", "Engaging GPT assistants", "39"], ["10", "Agent reasoning", "244"]]"#
        );
    }

    #[test]
    fn contents_rows_without_numbers_keep_two_columns() {
        let mut g = Vec::new();
        contents_row(&mut g, 100.0, None, "preface", "xiii");
        contents_row(&mut g, 114.0, None, "acknowledgments", "xv");
        contents_row(&mut g, 128.0, None, "about this book", "xvii");
        let out = order(&g);
        assert_eq!(
            out,
            [r#"[["preface", "xiii"], ["acknowledgments", "xv"], ["about this book", "xvii"]]"#]
        );
    }

    #[test]
    fn contents_runs_split_by_other_text_are_not_a_table() {
        // Un contenido detallado: las entradas se interrumpen con otro texto (los
        // subapartados), y un tramo suelto sería un fragmento de tabla.
        let mut g = Vec::new();
        contents_row(&mut g, 100.0, Some("1"), "Introduction to agents", "1");
        contents_row(
            &mut g,
            118.0,
            Some("2"),
            "Harnessing the power of models",
            "14",
        );
        contents_row(&mut g, 136.0, Some("3"), "Engaging GPT assistants", "39");
        g.extend(text_at(
            "Subentries come here in a longer line of plain text",
            140.0,
            154.0,
            10.0,
            false,
        ));
        contents_row(
            &mut g,
            172.0,
            Some("4"),
            "Exploring multi-agent systems",
            "68",
        );
        contents_row(
            &mut g,
            190.0,
            Some("5"),
            "Empowering agents with actions and more tools",
            "98",
        );
        contents_row(&mut g, 208.0, Some("6"), "Building assistants", "129");
        let out = order(&g);
        assert!(out.iter().all(|b| !b.starts_with("[[")), "{out:?}");
    }

    #[test]
    fn two_contents_rows_are_not_enough_for_a_table() {
        let mut g = Vec::new();
        contents_row(&mut g, 100.0, Some("1"), "Introduction to agents", "1");
        contents_row(
            &mut g,
            118.0,
            Some("2"),
            "Harnessing the power of large language models",
            "14",
        );
        let out = order(&g);
        assert!(out.iter().all(|b| !b.starts_with("[[")), "{out:?}");
    }

    #[test]
    fn bibliography_columns_are_not_a_tabular_band() {
        let doc =
            crate::pdf::read::open(&std::fs::read("examples/pdf/corpus/p2.pdf").unwrap()).unwrap();
        let raw = crate::pdf::read::page(&doc, 7).unwrap();
        let (ls, _) = lines(&raw.glyphs);
        let region = Region {
            lines: &ls,
            pictures: &[],
        };
        let leaves = reading_order(&region, (0..ls.len()).map(Item::Line).collect());
        let selected: Vec<_> = leaves
            .iter()
            .filter(|l| l.table)
            .map(|l| {
                l.items
                    .iter()
                    .filter_map(|i| match i {
                        Item::Line(k) => Some(region.lines[*k].text()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        assert!(selected.is_empty(), "{selected:?}");
    }

    #[test]
    fn text_cards_are_not_promoted_to_a_tabular_band() {
        let doc =
            crate::pdf::read::open(&std::fs::read("examples/pdf/corpus/p1.pdf").unwrap()).unwrap();
        let raw = crate::pdf::read::page(&doc, 36).unwrap();
        let (ls, _) = lines(&raw.glyphs);
        let region = Region {
            lines: &ls,
            pictures: &[],
        };
        let leaves = reading_order(&region, (0..ls.len()).map(Item::Line).collect());
        assert!(leaves.iter().all(|l| !l.table));
        let text = blocks(&region, &leaves)
            .into_iter()
            .filter_map(|(b, _, _)| match b {
                Block::Para(p) => Some(p.text()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains(
            "Escribir bien ayuda a potenciar el perfil profesional Expresarse sin errores genera credibilidad"
        ));
        assert!(text.contains("Darle valor a la importancia de la autoría de un texto"));
    }

    /// Columna de un índice: entradas al margen, subentradas sangradas; una letra de
    /// sección deja un hueco antes.
    fn index_column(x: f32, top: f32, entries: &[&str]) -> Vec<Glyph> {
        let mut g = Vec::new();
        let mut base = top;
        for e in entries {
            if let Some(letter) = e.strip_prefix('#') {
                base += 8.0;
                g.extend(text_at(letter, x, base, 10.0, true));
            } else if let Some(sub) = e.strip_prefix("  ") {
                g.extend(text_at(sub, x + 8.0, base, 10.0, false));
            } else {
                g.extend(text_at(e, x, base, 10.0, false));
            }
            base += 12.0;
        }
        g
    }

    #[test]
    fn index_columns_are_read_as_columns_not_as_a_table() {
        let left = [
            "agents 12",
            "  building with tools 14",
            "  memory and recall 20",
            "  planning 31",
            "#B",
            "batch runs 40",
            "  installing 41",
            "  running locally 42",
            "behavior trees 50",
            "  custom actions 52",
            "  knowledge 55",
            "  file uploads 57",
        ];
        let right = [
            "memory 70",
            "  compression 72",
            "  retrieval 75",
            "  semantic 77",
            "  overview of 83",
            "#N",
            "native functions 91",
            "  applying 93",
            "  embedding 95",
            "nonplayer roles 98",
            "  scripted 99",
            "  web APIs 101",
        ];
        let mut g = index_column(50.0, 60.0, &left);
        g.extend(index_column(240.0, 60.0, &right));
        let out = order(&g);
        assert!(out.iter().all(|b| !b.starts_with("[[")), "{out:?}");
        let text = out.join(" ");
        let at = |s: &str| text.find(s).unwrap_or_else(|| panic!("{s}: {text}"));
        // La columna izquierda entera antes que la derecha.
        assert!(at("file uploads 57") < at("memory 70"), "{text}");
        assert!(at("agents 12") < at("batch runs 40"), "{text}");
        assert!(at("memory 70") < at("native functions 91"), "{text}");
    }

    /// Diagrama de tres columnas de cajas con rótulos breves alineados fila a fila,
    /// y debajo `caption`.
    fn diagram(caption: &str) -> Vec<Glyph> {
        let columns = [
            [
                "Entrada",
                "lee el archivo",
                "valida el formato",
                "Archivo leído",
            ],
            [
                "Proceso",
                "convierte el texto",
                "ordena las páginas",
                "Texto ordenado",
            ],
            [
                "Salida",
                "escribe el bundle",
                "registra el informe",
                "Bundle escrito",
            ],
        ];
        let mut g = Vec::new();
        for (c, col) in columns.iter().enumerate() {
            for (r, label) in col.iter().enumerate() {
                g.extend(text_at(
                    label,
                    50.0 + c as f32 * 130.0,
                    60.0 + r as f32 * 30.0,
                    9.0,
                    false,
                ));
            }
        }
        g.extend(text_at(caption, 50.0, 200.0, 9.0, true));
        g
    }

    #[test]
    fn a_captioned_figure_is_read_by_columns_not_as_a_table() {
        let out = order(&diagram("Figura 3. Etapas de la conversión"));
        assert!(out.iter().all(|b| !b.starts_with("[[")), "{out:?}");
        let text = out.join(" | ");
        let at = |s: &str| text.find(s).unwrap_or_else(|| panic!("{s}: {text}"));
        // Cada columna del diagrama entera, de izquierda a derecha.
        assert!(at("Archivo leído") < at("Proceso"), "{text}");
        assert!(at("Texto ordenado") < at("Salida"), "{text}");
        assert!(at("Bundle escrito") < at("Figura 3."), "{text}");
    }

    #[test]
    fn a_grid_without_a_figure_caption_stays_a_table() {
        let out = order(&diagram(
            "La tabla anterior resume las etapas de la conversión.",
        ));
        assert!(out[0].starts_with("[["), "{out:?}");
    }

    #[test]
    fn wrapped_two_column_table_keeps_its_rows() {
        // Celdas de dos líneas que continúan su frase en las dos columnas, con un
        // hueco entre filas: los comienzos de fila coinciden en las dos columnas.
        let rows = [
            (
                "el primer elemento",
                "tiene una descripción",
                "de la tabla",
                "que sigue abajo",
            ),
            (
                "el segundo elemento",
                "describe otra cosa",
                "con dos líneas",
                "también partida",
            ),
            (
                "el tercer elemento",
                "cierra la serie",
                "de la tabla",
                "con su texto",
            ),
            (
                "el cuarto elemento",
                "añade una fila",
                "más al final",
                "de la tabla",
            ),
        ];
        let mut g = Vec::new();
        for (k, (a1, b1, a2, b2)) in rows.iter().enumerate() {
            let base = 60.0 + k as f32 * 34.0;
            g.extend(text_at(a1, 50.0, base, 10.0, false));
            g.extend(text_at(b1, 240.0, base, 10.0, false));
            g.extend(text_at(a2, 50.0, base + 12.0, 10.0, false));
            g.extend(text_at(b2, 240.0, base + 12.0, 10.0, false));
        }
        let out = order(&g);
        assert!(out.len() == 1 && out[0].starts_with("[["), "{out:?}");
    }

    #[test]
    fn a_wrapped_pair_of_labels_is_read_by_columns_not_as_a_table() {
        // Dos rótulos que se parten en dos líneas cada uno: la segunda línea continúa la
        // primera en las dos columnas, así que es una sola fila lógica, no una tabla.
        let mut g = text_at("Saludar, pedir el favor,", 50.0, 60.0, 10.0, false);
        g.extend(text_at("¿Con qué", 240.0, 60.0, 10.0, false));
        g.extend(text_at("dar las gracias", 50.0, 76.0, 10.0, false));
        g.extend(text_at("asocias el", 240.0, 76.0, 10.0, false));
        let out = order(&g);
        assert!(out.iter().all(|b| !b.starts_with("[[")), "{out:?}");
        let text = out.join(" | ");
        assert!(
            text.contains("Saludar, pedir el favor, dar las gracias"),
            "{text}"
        );
        assert!(text.contains("¿Con qué asocias el"), "{text}");
    }

    #[test]
    fn a_single_row_of_labels_is_not_a_table() {
        // Dos rótulos de dos líneas con interlineado normal: las líneas se unen en una
        // sola fila, y una fila no es una tabla.
        let mut g = text_at("Habilidades de", 50.0, 60.0, 10.0, false);
        g.extend(text_at("Lenguaje", 240.0, 60.0, 10.0, false));
        g.extend(text_at("escucha", 50.0, 72.0, 10.0, false));
        g.extend(text_at("dinámico", 240.0, 72.0, 10.0, false));
        let out = order(&g);
        assert!(out.iter().all(|b| !b.starts_with("[[")), "{out:?}");
        let text = out.join(" | ");
        assert!(text.contains("Habilidades de escucha"), "{text}");
        assert!(text.contains("Lenguaje dinámico"), "{text}");
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
