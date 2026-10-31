//! Regiones de una hoja: tablas de Excel, tablas dinámicas y bloques de celdas con
//! contenido separados por filas o columnas vacías.
use super::a1::Rect;
use super::read::{Kind, PivotDef, Sheet, TableDef};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub(crate) enum Origin {
    Table(TableDef),
    Pivot(PivotDef),
    Free,
}

#[derive(Clone, Debug)]
pub(crate) struct Region {
    pub(crate) rect: Rect,
    pub(crate) origin: Origin,
    /// Celda de texto suelta justo encima de la región, que la titula.
    pub(crate) caption: Option<usize>,
    /// Índices de las celdas de la hoja que pertenecen a la región, en orden de filas.
    pub(crate) cells: Vec<usize>,
}

impl Region {
    pub(crate) fn tabular(&self) -> bool {
        self.rect.rows() >= 2 && self.rect.cols() >= 2
    }
}

fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

/// Divide las celdas con contenido en regiones. Las tablas de Excel y las tablas
/// dinámicas son regiones fijas; el resto de celdas se agrupa por contigüidad
/// (incluidas las diagonales), las celdas combinadas extienden su bloque y los bloques
/// cuyos rangos se solapan se unen. Una celda de texto suelta a una o dos filas por
/// encima de una región tabular, dentro de sus columnas, pasa a ser su título.
pub(crate) fn regions(sh: &Sheet, tables: &[TableDef], pivots: &[PivotDef]) -> Vec<Region> {
    let mut out: Vec<Region> = Vec::new();
    let fixed = tables
        .iter()
        .cloned()
        .map(Origin::Table)
        .chain(pivots.iter().cloned().map(Origin::Pivot));
    for origin in fixed {
        let rect = match &origin {
            Origin::Table(t) => t.range,
            Origin::Pivot(p) => p.range,
            Origin::Free => continue,
        };
        if out.iter().any(|r| r.rect.overlaps(&rect)) {
            continue;
        }
        out.push(Region {
            rect,
            origin,
            caption: None,
            cells: Vec::new(),
        });
    }
    let fixed_count = out.len();
    let mut free = Vec::new();
    for (i, c) in sh.cells.iter().enumerate() {
        match out[..fixed_count]
            .iter_mut()
            .find(|r| r.rect.contains(c.row, c.col))
        {
            Some(r) => r.cells.push(i),
            None => free.push(i),
        }
    }
    let pos: HashMap<(u32, u32), usize> = free
        .iter()
        .enumerate()
        .map(|(k, &i)| ((sh.cells[i].row, sh.cells[i].col), k))
        .collect();
    let mut parent: Vec<usize> = (0..free.len()).collect();
    for (k, &i) in free.iter().enumerate() {
        let (r, c) = (sh.cells[i].row, sh.cells[i].col);
        let neighbours = [
            (r.wrapping_sub(1), c.wrapping_sub(1)),
            (r.wrapping_sub(1), c),
            (r.wrapping_sub(1), c + 1),
            (r, c.wrapping_sub(1)),
        ];
        for n in neighbours {
            if let Some(&j) = pos.get(&n) {
                let (a, b) = (find(&mut parent, k), find(&mut parent, j));
                if a != b {
                    parent[a] = b;
                }
            }
        }
    }
    let mut groups: HashMap<usize, Region> = HashMap::new();
    for (k, &i) in free.iter().enumerate() {
        let root = find(&mut parent, k);
        let cell = Rect::cell(sh.cells[i].row, sh.cells[i].col);
        let g = groups.entry(root).or_insert_with(|| Region {
            rect: cell,
            origin: Origin::Free,
            caption: None,
            cells: Vec::new(),
        });
        g.rect = g.rect.union(&cell);
        g.cells.push(i);
    }
    let mut blocks: Vec<Region> = groups.into_values().collect();
    for m in &sh.merges {
        if let Some(b) = blocks.iter_mut().find(|b| b.rect.contains(m.r0, m.c0)) {
            b.rect = b.rect.union(m);
        }
    }
    // Une los bloques cuyos rangos se solapan, y los textos de una sola columna
    // separados por una fila vacía (párrafos de una misma nota), hasta que no quede
    // ninguno.
    blocks.sort_by_key(|b| (b.rect.r0, b.rect.c0));
    let joins = |a: &Rect, b: &Rect| {
        a.overlaps(b)
            || (a.cols() == 1
                && b.cols() == 1
                && a.c0 == b.c0
                && (b.r0 > a.r1 && b.r0 - a.r1 <= 2 || a.r0 > b.r1 && a.r0 - b.r1 <= 2))
    };
    let mut merged = true;
    while merged {
        merged = false;
        'outer: for a in 0..blocks.len() {
            for b in a + 1..blocks.len() {
                if joins(&blocks[a].rect, &blocks[b].rect) {
                    let other = blocks.remove(b);
                    blocks[a].rect = blocks[a].rect.union(&other.rect);
                    blocks[a].cells.extend(other.cells);
                    merged = true;
                    break 'outer;
                }
            }
        }
    }
    out.extend(blocks);
    // Títulos sueltos sobre una región tabular.
    let mut k = 0;
    while k < out.len() {
        let candidate = matches!(out[k].origin, Origin::Free) && out[k].cells.len() == 1 && {
            let c = &sh.cells[out[k].cells[0]];
            c.kind == Kind::Text && c.formula.is_none() && out[k].rect.rows() == 1
        };
        let target = candidate
            .then(|| {
                let c = &sh.cells[out[k].cells[0]];
                out.iter().position(|r| {
                    r.tabular()
                        && r.caption.is_none()
                        && r.rect.r0 > c.row
                        && r.rect.r0 - c.row <= 2
                        && (r.rect.c0..=r.rect.c1).contains(&c.col)
                })
            })
            .flatten();
        match target {
            Some(t) => {
                let caption = out.remove(k);
                let t = if t > k { t - 1 } else { t };
                let i = caption.cells[0];
                out[t].rect = out[t].rect.union(&caption.rect);
                out[t].caption = Some(i);
                out[t].cells.insert(0, i);
                k = 0;
            }
            None => k += 1,
        }
    }
    for r in out.iter_mut() {
        r.cells.sort_by_key(|&i| (sh.cells[i].row, sh.cells[i].col));
    }
    out.sort_by_key(|r| (r.rect.r0, r.rect.c0));
    out
}
