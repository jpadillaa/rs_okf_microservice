//! Adaptador XLSX: lee el libro con el lector OOXML propio, hoja por hoja y en
//! streaming, divide cada hoja en regiones y genera un concepto por hoja, con
//! conceptos adicionales y datos completos (CSV y registros por celda) para las
//! regiones grandes.
mod a1;
mod read;
mod regions;

use crate::bundle::*;
use crate::format::Format;
use crate::ooxml::*;
use crate::output::*;
use crate::settings::Settings;
use crate::worker::{Config, Rejected};
use a1::{Rect, cell_name, cells_phrase, col_name, shift_formula};
use anyhow::{Context, Result, bail};
use read::{Cell, Drawn, Fmt, Kind, Link, Sheet, SheetInfo, SheetKind, Workbook, Zip};
use regions::{Origin, Region};
use std::io::Write;
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Cursor,
    path::Path as FsPath,
    time::Instant,
};
use yaml_serde::Value as YamlValue;
use zip::{ZipArchive, write::SimpleFileOptions};

/// Una región con más filas de datos o más celdas que estos límites es grande: tiene
/// concepto propio, una muestra y sus datos completos como recurso.
pub(crate) const LARGE_ROWS: usize = 60;
pub(crate) const LARGE_CELLS: usize = 1500;
/// Filas de datos de la muestra de una región grande.
const SAMPLE_ROWS: usize = 10;
/// Grupos de fórmulas que se listan en el concepto de una región grande; el resto
/// queda en los registros por celda.
const MAX_FORMULA_LINES: usize = 40;

/// Recuentos del libro para el informe.
#[derive(Default)]
struct Tally {
    cells: usize,
    formulas: usize,
    inline_regions: usize,
    own_regions: usize,
    tables: usize,
    pivots: usize,
    csv: usize,
    records: usize,
    charts: usize,
    images: usize,
    comments: usize,
    merges: usize,
    dates: usize,
}

/// Lo que documento.md y los índices necesitan de cada hoja.
struct SheetSummary {
    file: String,
    name: String,
    state: Option<String>,
    kind: SheetKind,
    used: Option<Rect>,
    regions: Vec<RegionSummary>,
    cells: usize,
}

struct RegionSummary {
    title: String,
    rect: Rect,
    file: Option<String>,
}

/// Disposición de una región: título, fila de encabezado y filas de datos.
struct Layout {
    title: String,
    title_rows: Vec<u32>,
    header: Option<u32>,
    data_rows: Vec<u32>,
}

fn md_cell(text: &str) -> String {
    escape_inline(text)
        .replace('|', "\\|")
        .replace('\n', "<br>")
}

/// Fórmula como código en línea, con delimitadores que no aparezcan en ella.
fn code(f: &str) -> String {
    if f.contains('`') {
        format!("`` {f} ``")
    } else {
        format!("`{f}`")
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Tramo de procedencia de un rango A1 de una hoja.
fn range_span(id: &str, sheet: &str, range: &Rect) -> Fields {
    span(id, "xlsx_range")
        .with("sheet", sheet)
        .with("range", range.a1())
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) || s.starts_with(' ') || s.ends_with(' ') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Número de registros de un CSV, respetando los saltos de línea entre comillas.
fn csv_records(data: &str) -> usize {
    let mut n = 0;
    let mut quoted = false;
    let mut pending = false;
    for c in data.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                pending = true;
            }
            '\n' if !quoted => {
                n += 1;
                pending = false;
            }
            '\r' => {}
            _ => pending = true,
        }
    }
    n + usize::from(pending)
}

struct Builder<'a> {
    wb: &'a Workbook,
    filename: &'a str,
    generated: &'a str,
    z_in: Zip<'a>,
    out: BundleWriter,
    issues: Vec<Issue>,
    t: Tally,
    sheet_index: HashMap<String, usize>,
    budget: usize,
}

impl Builder<'_> {
    fn sheet_file(n: usize) -> String {
        format!("hoja-{n:03}.md")
    }

    #[allow(clippy::too_many_arguments)]
    fn frontmatter(
        &self,
        kind: &str,
        title: &str,
        description: &str,
        n: usize,
        info: &SheetInfo,
        draft: bool,
        spans: Vec<Fields>,
    ) -> Fields {
        let fields = concept_fields(Format::Xlsx, kind, title, description, self.generated)
            .with_opt("status", draft.then_some("draft"))
            .with("sheet_index", n)
            .with_opt("sheet_state", info.state.as_ref());
        if spans.is_empty() {
            fields
        } else {
            fields.with("source_spans", spans)
        }
    }

    /// Destino Markdown de cada celda con hipervínculo.
    fn links(&self, sh: &Sheet, rels: &[Rel]) -> HashMap<(u32, u32), String> {
        let mut out = HashMap::new();
        for (rect, link) in &sh.links {
            let href = match link {
                Link::External(id) => rels
                    .iter()
                    .find(|r| &r.id == id && r.external)
                    .map(|r| r.target.clone()),
                // Una ubicación del libro que no es una hoja existente (hoja ausente o
                // nombre definido) es una referencia interna sin destino (plan 3, I5).
                Link::Internal(loc) => Some(
                    read::location_sheet(loc)
                        .and_then(|s| self.sheet_index.get(&s))
                        .map_or_else(|| UNRESOLVED_DEST.to_string(), |&n| Self::sheet_file(n)),
                ),
            };
            let Some(href) = href else { continue };
            let href = if href.contains([' ', '(', ')']) {
                format!("<{href}>")
            } else {
                href
            };
            for c in sh.cells.iter().filter(|c| rect.contains(c.row, c.col)) {
                out.insert((c.row, c.col), href.clone());
            }
        }
        out
    }

    fn cell_text(&self, c: &Cell, links: &HashMap<(u32, u32), String>) -> String {
        let text = if c.kind == Kind::NoValue {
            md_cell("[sin valor almacenado]")
        } else {
            md_cell(c.shown())
        };
        match links.get(&(c.row, c.col)) {
            Some(href) if !text.is_empty() && href == UNRESOLVED_DEST => {
                format!("{text} {UNRESOLVED}")
            }
            Some(href) if !text.is_empty() => format!("[{text}]({href})"),
            _ => text,
        }
    }

    fn layout(&self, sh: &Sheet, r: &Region) -> Layout {
        let rows_of = |row: u32| r.cells.iter().filter(move |&&i| sh.cells[i].row == row);
        let mut rows: Vec<u32> = r.cells.iter().map(|&i| sh.cells[i].row).collect();
        rows.dedup();
        let mut title_rows = Vec::new();
        let mut title = match &r.origin {
            Origin::Table(t) => format!("Tabla «{}»", t.name),
            Origin::Pivot(p) => format!("Tabla dinámica «{}»", p.name),
            Origin::Free => String::new(),
        };
        if let Some(i) = r.caption {
            let c = &sh.cells[i];
            title_rows.push(c.row);
            if title.is_empty() {
                title = truncate_chars(c.shown(), 100);
            } else {
                title = format!("{title}: {}", truncate_chars(c.shown(), 100));
            }
        } else if matches!(r.origin, Origin::Free)
            && r.tabular()
            && rows.len() >= 3
            && rows_of(rows[0]).count() == 1
        {
            let c = &sh.cells[*rows_of(rows[0]).next().unwrap_or(&r.cells[0])];
            if c.kind == Kind::Text && c.formula.is_none() {
                title_rows.push(c.row);
                title = truncate_chars(c.shown(), 100);
            }
        }
        if title.is_empty() {
            title = format!("Región {}", r.rect.a1());
        }
        let rest: Vec<u32> = rows
            .iter()
            .copied()
            .filter(|row| !title_rows.contains(row))
            .collect();
        let header = match &r.origin {
            Origin::Table(t) if t.header_rows == 0 => None,
            Origin::Table(t) => Some(t.range.r0),
            _ if !r.tabular() || rest.len() < 2 => None,
            _ => {
                let h = rest[0];
                let cells: Vec<&Cell> = rows_of(h).map(|&i| &sh.cells[i]).collect();
                (cells.len() >= 2
                    && cells.len() * 2 >= r.rect.cols() as usize
                    && cells
                        .iter()
                        .all(|c| c.kind == Kind::Text && c.formula.is_none()))
                .then_some(h)
            }
        };
        let data_rows = rest
            .into_iter()
            .filter(|row| Some(*row) != header)
            .collect();
        Layout {
            title,
            title_rows,
            header,
            data_rows,
        }
    }

    /// Tabla Markdown de las filas indicadas, con la fila de la hoja en la primera
    /// columna; sin encabezado detectado se usan las letras de columna.
    fn table_md(
        &self,
        sh: &Sheet,
        r: &Region,
        layout: &Layout,
        rows: &[u32],
        links: &HashMap<(u32, u32), String>,
    ) -> String {
        // Solo las celdas de las filas pedidas y del encabezado.
        let (first, last) = (
            rows.first().copied().unwrap_or(0),
            rows.last().copied().unwrap_or(0),
        );
        let grid: HashMap<(u32, u32), &Cell> = r
            .cells
            .iter()
            .map(|&i| &sh.cells[i])
            .filter(|c| (first..=last).contains(&c.row) || Some(c.row) == layout.header)
            .map(|c| ((c.row, c.col), c))
            .collect();
        let cols: Vec<u32> = (r.rect.c0..=r.rect.c1).collect();
        let mut table = Vec::new();
        let mut head = vec!["Fila".to_string()];
        for &c in &cols {
            let label = layout
                .header
                .and_then(|h| grid.get(&(h, c)))
                .map(|cell| self.cell_text(cell, links))
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| col_name(c));
            head.push(label);
        }
        table.push(head);
        for &row in rows {
            let mut line = vec![row.to_string()];
            for &c in &cols {
                line.push(
                    grid.get(&(row, c))
                        .map(|cell| self.cell_text(cell, links))
                        .unwrap_or_default(),
                );
            }
            table.push(line);
        }
        table_markdown(&table)
    }

    /// Bloque de una región pequeña: lista de celdas si ocupa una sola columna, tabla
    /// en los demás casos.
    fn region_block(
        &self,
        sh: &Sheet,
        r: &Region,
        layout: &Layout,
        links: &HashMap<(u32, u32), String>,
    ) -> String {
        if r.rect.cols() == 1 && r.caption.is_none() {
            return r
                .cells
                .iter()
                .map(|&i| {
                    let c = &sh.cells[i];
                    format!("- {}: {}\n", c.name(), self.cell_text(c, links))
                })
                .collect();
        }
        self.table_md(sh, r, layout, &layout.data_rows, links)
    }

    /// Fórmulas de una región agrupadas: las celdas contiguas de una columna (o de una
    /// fila) cuya fórmula es la anterior con las referencias relativas desplazadas
    /// forman un solo grupo, como al rellenar en Excel.
    fn formulas_md(&self, sh: &Sheet, r: &Region, max: usize) -> (String, usize) {
        struct Run<'c> {
            cells: Vec<&'c Cell>,
        }
        let mut cells: Vec<&Cell> = r
            .cells
            .iter()
            .map(|&i| &sh.cells[i])
            .filter(|c| c.formula.is_some() || c.shared.is_some())
            .collect();
        if cells.is_empty() {
            return (String::new(), 0);
        }
        cells.sort_by_key(|c| (c.col, c.row));
        let follows = |a: &Cell, b: &Cell, dr: i64, dc: i64| match (&a.formula, &b.formula) {
            (Some(fa), Some(fb)) => shift_formula(&fa[1..], dr, dc).is_some_and(|s| s == fb[1..]),
            _ => false,
        };
        let mut vertical: Vec<Run> = Vec::new();
        for c in cells {
            match vertical.last_mut() {
                Some(run)
                    if run.cells.last().is_some_and(|l| {
                        l.col == c.col && l.row + 1 == c.row && follows(l, c, 1, 0)
                    }) =>
                {
                    run.cells.push(c)
                }
                _ => vertical.push(Run { cells: vec![c] }),
            }
        }
        let (mut runs, singles): (Vec<Run>, Vec<Run>) =
            vertical.into_iter().partition(|r| r.cells.len() > 1);
        let mut singles: Vec<&Cell> = singles.into_iter().map(|r| r.cells[0]).collect();
        singles.sort_by_key(|c| (c.row, c.col));
        for c in singles {
            match runs.last_mut() {
                Some(run)
                    if run.cells.iter().all(|x| x.row == c.row)
                        && run
                            .cells
                            .last()
                            .is_some_and(|l| l.col + 1 == c.col && follows(l, c, 0, 1)) =>
                {
                    run.cells.push(c)
                }
                _ => runs.push(Run { cells: vec![c] }),
            }
        }
        runs.sort_by_key(|r| (r.cells[0].row, r.cells[0].col));
        let total = runs.len();
        let mut out = String::new();
        for run in runs.iter().take(max) {
            let first = run.cells[0];
            let last = run.cells[run.cells.len() - 1];
            let f = first
                .formula
                .as_deref()
                .unwrap_or("(fórmula compartida no reconstruible)");
            let mut line = if run.cells.len() == 1 {
                format!("- {}: {}", first.name(), code(f))
            } else {
                let range = Rect {
                    r0: first.row,
                    c0: first.col,
                    r1: last.row,
                    c1: last.col,
                };
                let example = run.cells[1]
                    .formula
                    .as_deref()
                    .map(|g| format!(" ({}: {})", run.cells[1].name(), code(g)))
                    .unwrap_or_default();
                format!(
                    "- {}: {} en {}; las demás celdas repiten la fórmula con sus referencias relativas desplazadas{example}",
                    range.a1(),
                    code(f),
                    first.name()
                )
            };
            if let Some(a) = first.array {
                line.push_str(&format!(" — fórmula matricial sobre {}", a.a1()));
            }
            let missing: Vec<(u32, u32)> = run
                .cells
                .iter()
                .filter(|c| c.kind == Kind::NoValue)
                .map(|c| (c.row, c.col))
                .collect();
            if missing.len() == run.cells.len() {
                line.push_str(" — sin valor almacenado");
            } else if !missing.is_empty() {
                line.push_str(&format!(
                    " — sin valor almacenado en {}",
                    cells_phrase(&missing, 10)
                ));
            }
            out.push_str(&line);
            out.push('\n');
        }
        if total > max {
            out.push_str(&format!(
                "- … y {} grupos más; todas las fórmulas están en los registros por celda.\n",
                total - max
            ));
        }
        (out, total)
    }

    /// Registro JSON de una celda: tipo, valor almacenado y mostrado, fórmula, formato
    /// y rango combinado.
    fn record(&self, sh: &Sheet, c: &Cell) -> String {
        let mut m = serde_json::Map::new();
        m.insert("celda".into(), c.name().into());
        m.insert("tipo".into(), c.kind.label().into());
        m.insert("almacenado".into(), c.raw.clone().into());
        m.insert("mostrado".into(), c.shown().into());
        if let Some(f) = &c.formula {
            m.insert("formula".into(), f.clone().into());
        }
        if let Some(a) = c.array {
            m.insert("matricial".into(), a.a1().into());
        }
        if let Some(code) = self.wb.styles.code(c.style) {
            m.insert("formato".into(), code.into());
        }
        if let Some(mr) = sh.merges.iter().find(|m| m.r0 == c.row && m.c0 == c.col) {
            m.insert("combinada".into(), mr.a1().into());
        }
        serde_json::Value::Object(m).to_string()
    }

    /// Concepto de una región grande, con su CSV y, si hace falta, sus registros por
    /// celda. Devuelve el nombre del concepto.
    #[allow(clippy::too_many_arguments)]
    fn large_region(
        &mut self,
        n: usize,
        k: usize,
        info: &SheetInfo,
        sh: &Sheet,
        r: &Region,
        layout: &Layout,
        links: &HashMap<(u32, u32), String>,
    ) -> Result<String> {
        let base = format!("hoja-{n:03}-region-{k:02}");
        let cells: Vec<&Cell> = r.cells.iter().map(|&i| &sh.cells[i]).collect();
        // CSV: el rango completo, fila por fila, incluidas las filas vacías; las celdas
        // de la región llegan en orden de filas y se escriben sin acumular el archivo.
        let csv_path = format!("assets/{base}.csv");
        self.out
            .zip()
            .start_file(&csv_path, SimpleFileOptions::default())?;
        let mut next = cells.iter().peekable();
        let mut line = vec![String::new(); r.rect.cols() as usize];
        for row in r.rect.r0..=r.rect.r1 {
            line.iter_mut().for_each(String::clear);
            while let Some(c) = next.next_if(|c| c.row == row) {
                line[(c.col - r.rect.c0) as usize] = csv_field(c.shown());
            }
            self.out.zip().write_all(line.join(",").as_bytes())?;
            self.out.zip().write_all(b"\r\n")?;
        }
        self.t.csv += 1;
        let header: HashMap<u32, &Cell> = cells
            .iter()
            .filter(|c| Some(c.row) == layout.header)
            .map(|c| (c.col, *c))
            .collect();
        let needs_records = cells.iter().any(|c| {
            c.formula.is_some()
                || c.shared.is_some()
                || !matches!(c.kind, Kind::Text | Kind::Number)
                || c.is_date(&self.wb.styles)
                || matches!(
                    self.wb.styles.fmt(c.style),
                    Fmt::Meaningful(_) | Fmt::Percent
                )
        }) || sh.merges.iter().any(|m| r.rect.contains(m.r0, m.c0));
        let records_path = format!("assets/{base}-celdas.jsonl");
        if needs_records {
            self.out
                .zip()
                .start_file(&records_path, SimpleFileOptions::default())?;
            for c in &cells {
                let line = self.record(sh, c);
                self.out.zip().write_all(line.as_bytes())?;
                self.out.zip().write_all(b"\n")?;
            }
            self.t.records += 1;
        }
        // Esquema por columna.
        let mut schema = vec![vec![
            "Columna".to_string(),
            "Encabezado".into(),
            "Tipos".into(),
            "Celdas con contenido".into(),
            "Fórmulas".into(),
        ]];
        for c in r.rect.c0..=r.rect.c1 {
            let col_cells: Vec<&&Cell> = cells
                .iter()
                .filter(|x| {
                    x.col == c
                        && Some(x.row) != layout.header
                        && !layout.title_rows.contains(&x.row)
                })
                .collect();
            let mut kinds: Vec<&str> = Vec::new();
            for x in &col_cells {
                let label = if x.is_date(&self.wb.styles) {
                    "fecha"
                } else {
                    x.kind.label()
                };
                if !kinds.contains(&label) {
                    kinds.push(label);
                }
            }
            let header = layout
                .header
                .and(header.get(&c))
                .map(|x| md_cell(x.shown()))
                .unwrap_or_default();
            schema.push(vec![
                col_name(c),
                header,
                if kinds.is_empty() {
                    "vacía".into()
                } else {
                    kinds.join(", ")
                },
                col_cells.len().to_string(),
                col_cells
                    .iter()
                    .filter(|x| x.formula.is_some() || x.shared.is_some())
                    .count()
                    .to_string(),
            ]);
        }
        let (formulas, groups) = self.formulas_md(sh, r, MAX_FORMULA_LINES);
        let sample: Vec<u32> = layout.data_rows.iter().copied().take(SAMPLE_ROWS).collect();
        let sample_rect = Rect {
            r0: r.rect.r0,
            c0: r.rect.c0,
            r1: sample.last().copied().unwrap_or(r.rect.r0),
            c1: r.rect.c1,
        };
        let formula_count = cells
            .iter()
            .filter(|c| c.formula.is_some() || c.shared.is_some())
            .count();
        let origin = match &r.origin {
            Origin::Table(t) => format!(
                "Tabla de Excel «{}»{}{}.",
                t.name,
                if t.header_rows > 0 {
                    format!("; encabezado en la fila {}", t.range.r0)
                } else {
                    String::new()
                },
                if t.totals_rows > 0 {
                    format!("; fila de totales {}", t.range.r1)
                } else {
                    String::new()
                }
            ),
            Origin::Pivot(p) => format!(
                "Tabla dinámica «{}»: se representan sus valores almacenados; su definición no se reproduce{}.",
                p.name,
                p.source
                    .as_deref()
                    .map(|s| format!(" (origen de datos: {s})"))
                    .unwrap_or_default()
            ),
            Origin::Free => match layout.header {
                Some(h) => format!("Encabezado detectado en la fila {h}."),
                None => {
                    "Sin fila de encabezado detectada; las columnas se nombran por su letra.".into()
                }
            },
        };
        let mut body = format!(
            "# {}\n\nRegión {} de la hoja [«{}»]({}): {} y {}, {} con contenido ({}). {origin}\n\n## Esquema\n\n{}\n## Datos completos\n\n- [Valores en CSV](../{csv_path}): las {} filas del rango {}, en orden y con todas sus columnas, incluidas las filas vacías; la primera fila del CSV es la fila {} de la hoja. Las fechas están en ISO 8601 y los números con la precisión que muestra Excel.\n",
            escape_inline(&layout.title),
            r.rect.a1(),
            escape_inline(&info.name),
            Self::sheet_file(n),
            plural(layout.data_rows.len(), "fila de datos", "filas de datos"),
            plural(r.rect.cols() as usize, "columna", "columnas"),
            plural(cells.len(), "celda", "celdas"),
            plural(formula_count, "fórmula", "fórmulas"),
            table_markdown(&schema),
            r.rect.rows(),
            r.rect.a1(),
            r.rect.r0,
        );
        if needs_records {
            body.push_str(&format!(
                "- [Registros por celda](../{records_path}): una línea JSON por celda con contenido, con su tipo, valor almacenado, valor mostrado, fórmula, formato y rango combinado.\n"
            ));
        }
        if groups > 0 {
            body.push_str(&format!(
                "\n## Fórmulas\n\nValores almacenados en el archivo; el conversor no recalcula.\n\n{formulas}"
            ));
        }
        body.push_str(&format!(
            "\n## Muestra\n\nPrimeras {} de {} filas de datos; **no es la tabla completa**, que está en el CSV.\n\n<!-- okf-source-span: muestra -->\n{}",
            sample.len(),
            layout.data_rows.len(),
            self.table_md(sh, r, layout, &sample, links)
        ));
        let spans = vec![
            range_span("region", &info.name, &r.rect),
            range_span("muestra", &info.name, &sample_rect),
        ];
        let missing = cells.iter().any(|c| c.kind == Kind::NoValue);
        let description = format!(
            "Región {} de la hoja «{}» del libro {}: {} y {}, con datos completos en CSV.",
            r.rect.a1(),
            info.name,
            self.filename,
            plural(layout.data_rows.len(), "fila de datos", "filas de datos"),
            plural(r.rect.cols() as usize, "columna", "columnas"),
        );
        let fields = self.frontmatter(
            "Sheet Region",
            &layout.title,
            &description,
            n,
            info,
            missing,
            spans,
        );
        let file = format!("{base}.md");
        self.note_unresolved(&info.name, &body);
        // Una región grande es hija del concepto de su hoja.
        self.out.concept_in(
            &format!("contenido/{file}"),
            &format!("contenido/{}", Self::sheet_file(n)),
            &fields,
            &body,
        )?;
        Ok(file)
    }

    /// Imágenes, gráficos, cuadros de texto y objetos de un dibujo; devuelve el
    /// Markdown de cada sección.
    fn drawn(
        &mut self,
        n: usize,
        info: &SheetInfo,
        items: &[Drawn],
        losses: &mut Vec<String>,
    ) -> Result<(String, String, String)> {
        let at = |a: &Option<(u32, u32)>| {
            a.map(|(r, c)| format!(" (anclado en {})", cell_name(r, c)))
                .unwrap_or_default()
        };
        let at_f = |a: &Option<(u32, u32)>| {
            a.map(|(r, c)| format!(" (anclada en {})", cell_name(r, c)))
                .unwrap_or_default()
        };
        let (mut charts, mut images, mut shapes) = (String::new(), String::new(), String::new());
        let mut image_n = 0;
        for d in items {
            match d {
                Drawn::Chart { anchor, name, part } => {
                    self.t.charts += 1;
                    let xml = part
                        .as_deref()
                        .and_then(|p| read_entry(&mut self.z_in, p).ok())
                        .map(|b| String::from_utf8_lossy(&b).into_owned())
                        .unwrap_or_default();
                    let text = chart_text(&xml);
                    let sources = read::chart_sources(&xml);
                    if text.is_empty() {
                        charts.push_str(&format!(
                            "- **{}**{}: _gráfico sin datos extraíbles en caché; no se representa._\n",
                            escape_inline(name),
                            at(anchor)
                        ));
                        losses.push(format!(
                            "gráfico «{name}»{} sin datos extraíbles",
                            at(anchor)
                        ));
                    } else {
                        let src = if sources.is_empty() {
                            String::new()
                        } else {
                            format!(
                                " Datos de origen: {}.",
                                sources
                                    .iter()
                                    .map(|s| code(s))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            )
                        };
                        charts.push_str(&format!(
                            "- **{}**{}: {}.{src}\n",
                            escape_inline(name),
                            at(anchor),
                            escape_inline(text.trim_end_matches('.'))
                        ));
                    }
                }
                Drawn::Image {
                    anchor,
                    target,
                    linked,
                    alt,
                } => {
                    let label = alt.as_deref().map(escape_inline).unwrap_or_default();
                    match target {
                        Some(t) if !linked && self.z_in.by_name(t).is_ok() => {
                            image_n += 1;
                            let ext = t
                                .rsplit_once('.')
                                .map(|(_, e)| e.to_ascii_lowercase())
                                .unwrap_or_else(|| "bin".into());
                            let path = format!("assets/hoja-{n:03}-imagen-{image_n:02}.{ext}");
                            let data = read_entry(&mut self.z_in, t)?;
                            self.out.file(&path, &data)?;
                            self.t.images += 1;
                            images.push_str(&format!(
                                "![{label}](../{path})\n\nImagen {image_n}{}{}.\n\n",
                                at_f(anchor),
                                if alt.is_none() {
                                    "; sin texto alternativo en el original"
                                } else {
                                    ""
                                }
                            ));
                        }
                        Some(t) if *linked => {
                            images.push_str(&generated_note(&format!(
                                "_Imagen{} vinculada a un archivo externo ({}), no incluida en el paquete._\n\n",
                                at_f(anchor),
                                escape_inline(t)
                            )));
                            losses.push(format!(
                                "imagen{} vinculada a un archivo externo",
                                at_f(anchor)
                            ));
                        }
                        _ => {
                            images.push_str(&generated_note(&format!(
                                "_Imagen{} ausente del paquete original._\n\n",
                                at_f(anchor)
                            )));
                            losses.push(format!(
                                "imagen{} ausente del paquete original",
                                at_f(anchor)
                            ));
                        }
                    }
                }
                Drawn::Shape { anchor, text } => {
                    let quoted: Vec<String> = text
                        .lines()
                        .map(|l| format!("> {}", escape_line_start(&escape_inline(l))))
                        .collect();
                    shapes.push_str(&format!(
                        "> **Cuadro de texto{}:**\n>\n{}\n\n",
                        at(anchor),
                        quoted.join("\n>\n")
                    ));
                }
            }
        }
        let _ = info;
        Ok((charts, images, shapes))
    }

    fn sheet(&mut self, n: usize, info: &SheetInfo) -> Result<SheetSummary> {
        let file = Self::sheet_file(n);
        let total = self.wb.sheets.len();
        let mut summary = SheetSummary {
            file: file.clone(),
            name: info.name.clone(),
            state: info.state.clone(),
            kind: info.kind.clone(),
            used: None,
            regions: Vec::new(),
            cells: 0,
        };
        let mut losses: Vec<String> = Vec::new();
        let mut notes: Vec<String> = Vec::new();
        let hidden_note = match info.state.as_deref() {
            Some("veryHidden") => {
                generated_note("> Hoja muy oculta en el original (solo visible desde VBA).\n\n")
            }
            Some(_) => generated_note("> Hoja oculta en el original.\n\n"),
            None => String::new(),
        };
        if info.state.is_some() {
            notes.push("hoja oculta en el original; se representa completa".into());
        }
        let part = info.part.clone();
        let mut body = format!("# {}\n\n", escape_inline(&info.name));
        let mut spans = Vec::new();
        match (&info.kind, part) {
            (SheetKind::Chart, Some(part)) => {
                let rels = relationships(&mut self.z_in, &part);
                let items: Vec<Drawn> = related(&rels, &part, "/drawing")
                    .iter()
                    .flat_map(|d| read::drawing(&mut self.z_in, d))
                    .collect();
                let (charts, images, _) = self.drawn(n, info, &items, &mut losses)?;
                body.push_str(&format!(
                    "{GENERATED}\nHoja de gráfico {n} de {total}: no tiene celdas, solo un gráfico.\n\n{hidden_note}"
                ));
                if !charts.is_empty() {
                    body.push_str(&format!("## Gráficos\n\n{charts}\n"));
                }
                if !images.is_empty() {
                    body.push_str(&format!("## Imágenes\n\n{images}"));
                }
            }
            (SheetKind::Work, Some(part)) => {
                let mut budget = self.budget;
                let sh = read::sheet_part(&mut self.z_in, &part, self.wb, &mut budget).map_err(|e| {
                    if e.to_string().contains("máximo de celdas") {
                        anyhow::Error::from(Rejected(format!(
                            "el libro supera el máximo de {} celdas con contenido (OKF_MAX_CELLS)",
                            self.budget_limit()
                        )))
                    } else {
                        e
                    }
                })?;
                self.budget = budget;
                if sh.protected {
                    self.issues.extend(restrictions_note(&[format!(
                        "la hoja «{}» está protegida",
                        info.name
                    )]));
                }
                self.sheet_cells(
                    n,
                    info,
                    &part,
                    &sh,
                    &mut summary,
                    &mut body,
                    &mut spans,
                    &mut losses,
                    &mut notes,
                    &hidden_note,
                )?;
            }
            (SheetKind::Other(kind), _) => {
                body.push_str(&format!(
                    "{GENERATED}\nHoja {n} de {total}.\n\n{hidden_note}{GENERATED}\n_Esta {kind} no está cubierta por el perfil y su contenido no se representa._\n"
                ));
                losses.push(format!("{kind} no representada"));
            }
            (_, None) => {
                body.push_str(&format!(
                    "Hoja {n} de {total}.\n\n_La parte de esta hoja no está en el paquete._\n"
                ));
                losses.push("la parte de la hoja falta en el paquete".into());
            }
        }
        for l in &losses {
            self.issues
                .push(Issue::loss(format!("Hoja «{}»: {l}", info.name)));
        }
        for l in &notes {
            self.issues
                .push(Issue::note(format!("Hoja «{}»: {l}", info.name)));
        }
        let description = match summary.used {
            Some(u) => format!(
                "Hoja {n} del libro {}: rango {} con {}.",
                self.filename,
                u.a1(),
                plural(summary.regions.len(), "región", "regiones")
            ),
            None => format!(
                "Hoja {n} del libro {}, sin celdas con contenido.",
                self.filename
            ),
        };
        let fields = self.frontmatter(
            "Sheet",
            &info.name,
            &description,
            n,
            info,
            !losses.is_empty(),
            spans,
        );
        self.note_unresolved(&info.name, &body);
        self.out
            .concept(&format!("contenido/{file}"), &fields, &body)?;
        Ok(summary)
    }

    /// Observación si un concepto de la hoja tiene hipervínculos internos sin destino.
    fn note_unresolved(&mut self, sheet: &str, body: &str) {
        let n = body.matches(UNRESOLVED).count();
        if n > 0 {
            self.issues.push(Issue::note(format!(
                "Hoja «{sheet}»: {n} hipervínculo(s) interno(s) sin destino (hoja ausente o nombre definido); se conserva su texto con la marca {UNRESOLVED_TEXT}"
            )));
        }
    }

    fn budget_limit(&self) -> usize {
        self.t.cells + self.budget
    }

    #[allow(clippy::too_many_arguments)]
    fn sheet_cells(
        &mut self,
        n: usize,
        info: &SheetInfo,
        part: &str,
        sh: &Sheet,
        summary: &mut SheetSummary,
        body: &mut String,
        spans: &mut Vec<Fields>,
        losses: &mut Vec<String>,
        notes: &mut Vec<String>,
        hidden_note: &str,
    ) -> Result<()> {
        let total = self.wb.sheets.len();
        let rels = relationships(&mut self.z_in, part);
        let tables: Vec<read::TableDef> = sh
            .tables
            .iter()
            .filter_map(|id| rels.iter().find(|r| &r.id == id))
            .map(|r| resolve(part, &r.target))
            .collect::<Vec<_>>()
            .iter()
            .filter_map(|p| read::table(&mut self.z_in, p))
            .collect();
        let pivots: Vec<read::PivotDef> = related(&rels, part, "/pivotTable")
            .iter()
            .filter_map(|p| read::pivot(&mut self.z_in, p))
            .collect();
        self.t.tables += tables.len();
        self.t.pivots += pivots.len();
        let links = self.links(sh, &rels);
        let formulas = sh
            .cells
            .iter()
            .filter(|c| c.formula.is_some() || c.shared.is_some())
            .count();
        self.t.cells += sh.cells.len();
        self.t.formulas += formulas;
        self.t.merges += sh.merges.len();
        self.t.dates += sh
            .cells
            .iter()
            .filter(|c| c.is_date(&self.wb.styles))
            .count();
        summary.cells = sh.cells.len();
        summary.used = sh.used();
        let regions = regions::regions(sh, &tables, &pivots);
        match summary.used {
            Some(u) => {
                spans.push(range_span("hoja", &info.name, &u));
                body.push_str(&format!(
                    "{GENERATED}\nHoja {n} de {total} · rango con contenido {} ({} × {}) · {} con contenido, {} · {}.\n\n{hidden_note}",
                    u.a1(),
                    plural(u.rows() as usize, "fila", "filas"),
                    plural(u.cols() as usize, "columna", "columnas"),
                    plural(sh.cells.len(), "celda", "celdas"),
                    plural(formulas, "fórmula", "fórmulas"),
                    plural(regions.len(), "región", "regiones"),
                ));
            }
            None => body.push_str(&format!(
                "{GENERATED}\nHoja {n} de {total}. La hoja no tiene celdas con contenido.\n\n{hidden_note}"
            )),
        }
        if formulas > 0 {
            body.push_str("Los valores de las fórmulas son los almacenados en el archivo; el conversor no recalcula.\n\n");
        }
        let mut own = String::new();
        for (k, r) in regions.iter().enumerate() {
            let k = k + 1;
            let layout = self.layout(sh, r);
            let large = layout.data_rows.len() > LARGE_ROWS || r.cells.len() > LARGE_CELLS;
            let heading = if layout.title.starts_with("Región ") {
                escape_inline(&layout.title)
            } else {
                format!("{} ({})", escape_inline(&layout.title), r.rect.a1())
            };
            if large {
                let file = self.large_region(n, k, info, sh, r, &layout, &links)?;
                self.t.own_regions += 1;
                own.push_str(&format!(
                    "- [{heading}]({file}) — {} y {}; datos completos en CSV.\n",
                    plural(layout.data_rows.len(), "fila de datos", "filas de datos"),
                    plural(r.rect.cols() as usize, "columna", "columnas")
                ));
                notes.push(format!(
                    "región grande {} con concepto propio y datos completos en CSV",
                    r.rect.a1()
                ));
                summary.regions.push(RegionSummary {
                    title: layout.title.clone(),
                    rect: r.rect,
                    file: Some(file),
                });
                continue;
            }
            self.t.inline_regions += 1;
            let id = format!("r-{k}");
            spans.push(range_span(&id, &info.name, &r.rect));
            body.push_str(&format!(
                "## {heading}\n\n<!-- okf-source-span: {id} -->\n{}\n",
                self.region_block(sh, r, &layout, &links)
            ));
            match &r.origin {
                Origin::Pivot(p) => {
                    body.push_str(&generated_note(&format!(
                        "> Tabla dinámica: se muestran sus valores almacenados; su definición (campos, filtros y origen{}) no se reproduce.\n\n",
                        p.source.as_deref().map(|s| format!(" {s}")).unwrap_or_default()
                    )));
                    notes.push(format!(
                        "tabla dinámica «{}» en {}: se representan sus valores almacenados, no su definición",
                        p.name,
                        p.range.a1()
                    ));
                }
                Origin::Table(t) if t.totals_rows > 0 => body.push_str(&generated_note(&format!(
                    "> La fila {} es la fila de totales de la tabla.\n\n",
                    t.range.r1
                ))),
                _ => {}
            }
            let (formulas, groups) = self.formulas_md(sh, r, usize::MAX);
            if groups > 0 {
                body.push_str(&format!("**Fórmulas**\n\n{formulas}\n"));
            }
            summary.regions.push(RegionSummary {
                title: layout.title.clone(),
                rect: r.rect,
                file: None,
            });
        }
        if !own.is_empty() {
            body.push_str(&format!("## Regiones con concepto propio\n\n{own}\n"));
        }
        // Dibujos: gráficos, imágenes y cuadros de texto.
        let items = sh
            .drawing
            .as_ref()
            .and_then(|id| rels.iter().find(|r| &r.id == id))
            .map(|r| read::drawing(&mut self.z_in, &resolve(part, &r.target)))
            .unwrap_or_default();
        let (charts, images, shapes) = self.drawn(n, info, &items, losses)?;
        if !charts.is_empty() {
            body.push_str(&format!("## Gráficos\n\n{charts}\n"));
            if charts.lines().any(|l| !l.contains("sin datos extraíbles")) {
                notes.push("gráficos representados por los datos de su caché".into());
            }
        }
        if !images.is_empty() {
            body.push_str(&format!("## Imágenes\n\n{images}"));
        }
        if !shapes.is_empty() {
            body.push_str(&format!("## Cuadros de texto\n\n{shapes}"));
            notes.push("cuadros de texto conservados como cita".into());
        }
        if !sh.objects.is_empty() {
            body.push_str("## Objetos incrustados\n\n");
            for p in &sh.objects {
                body.push_str(&format!(
                    "- _Objeto incrustado ({}) sin representación fiel._\n",
                    escape_inline(p)
                ));
                losses.push(format!("objeto incrustado ({p}) sin representación fiel"));
            }
            body.push('\n');
        }
        // Señales de las celdas.
        let no_value: Vec<(u32, u32)> = sh
            .cells
            .iter()
            .filter(|c| c.kind == Kind::NoValue)
            .map(|c| (c.row, c.col))
            .collect();
        if !no_value.is_empty() {
            losses.push(format!(
                "fórmulas sin valor almacenado en {}; el conversor no recalcula",
                cells_phrase(&no_value, 20)
            ));
        }
        for (i, target) in self.wb.external.iter().enumerate() {
            let tag = format!("[{}]", i + 1);
            let using: Vec<(u32, u32)> = sh
                .cells
                .iter()
                .filter(|c| c.formula.as_deref().is_some_and(|f| f.contains(&tag)))
                .map(|c| (c.row, c.col))
                .collect();
            if !using.is_empty() {
                losses.push(format!(
                    "fórmulas en {} que leen el libro externo {tag} ({target}); conservan su último valor almacenado y el libro externo no está incluido",
                    cells_phrase(&using, 20)
                ));
            }
        }
        let errors: Vec<String> = sh
            .cells
            .iter()
            .filter(|c| c.kind == Kind::Error)
            .map(|c| format!("{} ({})", c.name(), c.raw))
            .collect();
        if !errors.is_empty() {
            notes.push(format!(
                "valores de error almacenados: {}",
                errors
                    .iter()
                    .take(20)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        let mut formats: Vec<(String, Vec<(u32, u32)>)> = Vec::new();
        for c in &sh.cells {
            if let Fmt::Meaningful(_) = self.wb.styles.fmt(c.style)
                && let Some(code) = self.wb.styles.code(c.style)
                && c.kind == Kind::Number
            {
                match formats.iter_mut().find(|(f, _)| f == code) {
                    Some((_, v)) => v.push((c.row, c.col)),
                    None => formats.push((code.to_string(), vec![(c.row, c.col)])),
                }
            }
        }
        if !formats.is_empty() {
            body.push_str("## Formatos numéricos con significado\n\nEl valor mostrado es el número almacenado; el formato añade una unidad o un texto que no aparece en él.\n\n");
            for (f, cells) in &formats {
                body.push_str(&format!("- {}: {}\n", cells_phrase(cells, 20), code(f)));
                notes.push(format!(
                    "formato numérico {f} en {}: el valor mostrado no incluye su unidad o texto",
                    cells_phrase(cells, 10)
                ));
            }
            body.push('\n');
        }
        if !sh.merges.is_empty() {
            let list: Vec<String> = sh.merges.iter().map(Rect::a1).collect();
            body.push_str(&format!(
                "## Celdas combinadas\n\nEl valor está en la primera celda de cada rango; las demás quedan vacías: {}.\n\n",
                list.join(", ")
            ));
            notes.push(format!("celdas combinadas: {}", list.join(", ")));
        }
        if !sh.conditional.is_empty() {
            body.push_str("## Formato condicional\n\nReglas del original; el resultado visual no se reproduce.\n\n");
            for (range, rule) in &sh.conditional {
                body.push_str(&format!("- {range}: {}\n", escape_inline(rule)));
            }
            body.push('\n');
            notes.push(format!(
                "formato condicional en {}; se listan sus reglas",
                sh.conditional
                    .iter()
                    .map(|(r, _)| r.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if !sh.hidden_rows.is_empty() || !sh.hidden_cols.is_empty() {
            body.push_str(
                "## Filas y columnas ocultas\n\nSu contenido se incluye en las regiones.\n\n",
            );
            let rows: Vec<usize> = sh.hidden_rows.iter().map(|&r| r as usize).collect();
            let mut parts = Vec::new();
            if !rows.is_empty() {
                parts.push(units_phrase("fila", "filas", &rows));
                body.push_str(&format!(
                    "- Ocultas: {}\n",
                    units_phrase("fila", "filas", &rows)
                ));
            }
            if !sh.hidden_cols.is_empty() {
                parts.push(read::columns_phrase(&sh.hidden_cols));
                body.push_str(&format!(
                    "- Ocultas: {}\n",
                    read::columns_phrase(&sh.hidden_cols)
                ));
            }
            body.push('\n');
            notes.push(format!(
                "{} ocultas; su contenido se incluye",
                parts.join(" y ")
            ));
        }
        let comments = read::comments(&mut self.z_in, &rels, part, self.wb);
        if !comments.is_empty() {
            self.t.comments += comments
                .iter()
                .map(|(_, c)| 1 + c.replies.len())
                .sum::<usize>();
            let list: Vec<Comment> = comments
                .iter()
                .map(|(cell, c)| Comment {
                    text: format!("{} (celda {cell})", c.text),
                    ..c.clone()
                })
                .collect();
            body.push_str(&comments_markdown(&list));
            notes.push("comentarios de revisión conservados como contenido auxiliar".into());
        }
        Ok(())
    }
}

/// Protección de la estructura del libro (plan 3, I7); la de cada hoja se lee con la
/// hoja. Se declara y no se aplica.
fn protections(z: &mut ZipArchive<Cursor<&[u8]>>) -> Vec<String> {
    let on = |n: &crate::xml::Node, k: &[u8]| n.attr(k).is_some_and(|v| matches!(v, "1" | "true"));
    let mut out = Vec::new();
    let main = crate::ooxml::main_part(z).unwrap_or_else(|| "xl/workbook.xml".into());
    if let Some(root) = crate::ooxml::parse_entry(z, &main)
        && let Some(p) = root.find(b"workbookProtection")
        && (on(p, b"lockStructure") || on(p, b"lockWindows"))
    {
        out.push("el libro protege su estructura".to_string());
    }
    out
}

/// Extrae el libro y ensambla el bundle.
pub(crate) fn build_bundle(
    bytes: &[u8],
    filename: &str,
    source: &crate::identity::Source,
    max_cells: usize,
    attempt: &FsPath,
) -> Result<Built> {
    let started = Instant::now();
    let generated = utc_timestamp(crate::now());
    let mut z_in = ZipArchive::new(Cursor::new(bytes))?;
    enforce_package_limits(&mut z_in)?;
    let wb = read::workbook(&mut z_in)?;
    if wb.sheets.is_empty() {
        return Err(Rejected("el libro no tiene hojas".into()).into());
    }
    let restrictions = protections(&mut z_in);
    let mut b = Builder {
        wb: &wb,
        filename,
        generated: &generated,
        z_in,
        out: BundleWriter::create(Format::Xlsx, attempt, source, &Settings::none())?,
        issues: restrictions_note(&restrictions).into_iter().collect(),
        t: Tally::default(),
        sheet_index: wb
            .sheets
            .iter()
            .enumerate()
            .map(|(i, s)| (s.name.clone(), i + 1))
            .collect(),
        budget: max_cells,
    };
    let mut sheets = Vec::new();
    for (i, info) in wb.sheets.iter().enumerate() {
        sheets.push(b.sheet(i + 1, info)?);
    }
    let t = &b.t;
    let hidden = sheets.iter().filter(|s| s.state.is_some()).count();
    let chartsheets = sheets.iter().filter(|s| s.kind == SheetKind::Chart).count();
    // Señales del libro.
    let mut issues = std::mem::take(&mut b.issues);
    if wb.recalc_on_load {
        issues.push(Issue::note(
            "El libro pide recalcular al abrirse (fullCalcOnLoad): los valores almacenados pueden no estar actualizados; el conversor no recalcula".into(),
        ));
    }
    for (i, target) in wb.external.iter().enumerate() {
        issues.push(Issue::loss(format!(
            "Vínculo externo [{}] al libro {target}: no está incluido en el bundle",
            i + 1
        )));
    }
    if t.dates > 0 {
        issues.push(Issue::note(format!(
            "{} con formato de fecha u hora convertidas de su número de serie a ISO 8601 (sistema {})",
            plural(t.dates, "celda", "celdas"),
            if wb.date1904 { "1904" } else { "1900" }
        )));
    }
    issues.push(Issue::note(format!(
        "Título del libro tomado {}",
        if wb.title.is_some() {
            "de las propiedades del documento"
        } else {
            "del nombre del archivo; el libro no lo declara"
        }
    )));
    let title = wb.title.clone().unwrap_or_else(|| {
        FsPath::new(filename)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| filename.to_string())
    });
    let summary = [
        format!(
            "Hojas inventariadas: {} ({} de gráfico; ocultas: {hidden})",
            sheets.len(),
            chartsheets
        ),
        format!(
            "Hojas representadas: {} (un concepto por hoja)",
            sheets.len()
        ),
        format!(
            "Celdas con contenido: {}; fórmulas: {}",
            t.cells, t.formulas
        ),
        format!(
            "Regiones: {} (en el concepto de su hoja: {}; con concepto propio: {})",
            t.inline_regions + t.own_regions,
            t.inline_regions,
            t.own_regions
        ),
        format!(
            "Tablas de Excel: {}; tablas dinámicas: {}",
            t.tables, t.pivots
        ),
        format!(
            "Recursos de datos: {} CSV y {} de registros por celda",
            t.csv, t.records
        ),
        format!("Gráficos: {}; imágenes adjuntas: {}", t.charts, t.images),
        format!("Rangos de celdas combinadas: {}", t.merges),
        format!("Comentarios de revisión: {}", t.comments),
        format!(
            "Nombres definidos: {}; vínculos externos: {}",
            wb.names.len(),
            wb.external.len()
        ),
        format!(
            "Duración de ensamblado: {} ms",
            started.elapsed().as_millis()
        ),
    ];
    let report = report(
        Format::Xlsx,
        &generated,
        source,
        &issues,
        &summary,
        "",
        &Settings::none(),
    );
    // documento.md e índices.
    let mut doc_spans = Vec::new();
    let mut structure = String::new();
    let mut entries = String::new();
    let mut content_entries = String::new();
    for (i, s) in sheets.iter().enumerate() {
        let n = i + 1;
        if let Some(u) = s.used {
            doc_spans.push(range_span(&format!("hoja-{n}"), &s.name, &u));
        }
        let mut detail = Vec::new();
        match s.kind {
            SheetKind::Chart => detail.push("hoja de gráfico".to_string()),
            SheetKind::Other(k) => detail.push(k.to_string()),
            SheetKind::Work => {}
        }
        match s.state.as_deref() {
            Some("veryHidden") => detail.push("muy oculta".into()),
            Some(_) => detail.push("oculta".into()),
            None => {}
        }
        match s.used {
            Some(u) => {
                detail.push(u.a1());
                detail.push(plural(s.cells, "celda", "celdas"));
                detail.push(plural(s.regions.len(), "región", "regiones"));
            }
            None if s.kind == SheetKind::Work => detail.push("sin celdas con contenido".into()),
            None => {}
        }
        let name = escape_inline(&s.name);
        structure.push_str(&format!(
            "- [{name}](contenido/{}) — {}\n",
            s.file,
            detail.join(" · ")
        ));
        entries.push_str(&format!("- [{name}](contenido/{})\n", s.file));
        content_entries.push_str(&format!("- [{name}]({})\n", s.file));
        for r in &s.regions {
            let label = if r.title.starts_with("Región ") {
                escape_inline(&r.title)
            } else {
                format!("{} ({})", escape_inline(&r.title), r.rect.a1())
            };
            match &r.file {
                Some(f) => {
                    structure
                        .push_str(&format!("  - [{label}](contenido/{f}) — concepto propio\n"));
                    entries.push_str(&format!("  - [{label}](contenido/{f})\n"));
                    content_entries.push_str(&format!("  - [{label}]({f})\n"));
                }
                None => structure.push_str(&format!("  - {label}\n")),
            }
        }
    }
    let mut extra = String::new();
    if !wb.names.is_empty() {
        extra.push_str("\n## Nombres definidos\n\n");
        for d in &wb.names {
            let scope = d
                .scope
                .and_then(|i| wb.sheets.get(i))
                .map(|s| format!(" (solo en la hoja «{}»)", escape_inline(&s.name)))
                .unwrap_or_default();
            extra.push_str(&format!(
                "- {}: {}{scope}\n",
                escape_inline(&d.name),
                code(&d.value)
            ));
        }
    }
    if !wb.external.is_empty() {
        extra.push_str("\n## Vínculos externos\n\nLibros de los que leen algunas fórmulas; no se incluyen en el bundle.\n\n");
        for (i, target) in wb.external.iter().enumerate() {
            extra.push_str(&format!("- [{}]: {}\n", i + 1, code(target)));
        }
    }
    if wb.recalc_on_load {
        extra.push_str(&generated_note("\n> El libro pide recalcular al abrirse; los valores almacenados de las fórmulas pueden no estar actualizados.\n"));
    }
    let mut doc_fields = document_fields(
        Format::Xlsx,
        &title,
        &format!("Libro {filename} convertido a conceptos OKF por hojas y regiones."),
        &generated,
        source,
        &Settings::none(),
    );
    if !doc_spans.is_empty() {
        doc_fields.push("source_spans", doc_spans);
    }
    let doc = format!(
        "# {}\n\nLibro de {} ({} de gráfico; ocultas: {hidden}) con {} con contenido y {}.\n\n## Hojas\n\n{structure}{extra}",
        escape_inline(&title),
        plural(sheets.len(), "hoja", "hojas"),
        chartsheets,
        plural(t.cells, "celda", "celdas"),
        plural(t.formulas, "fórmula", "fórmulas"),
    );
    let nav = Navigation {
        contents_label: "Índice de contenido",
        heading: "Hojas",
        root_entries: entries,
        contents_entries: content_entries,
    };
    b.out
        .finish(bytes, (&doc_fields, &doc), &nav, &report, &issues)
}

/// Extrae y ensambla el bundle de un libro.
pub(crate) fn convert(
    cfg: &Config,
    bytes: &[u8],
    filename: &str,
    source: &crate::identity::Source,
    dir: &FsPath,
) -> Result<Built> {
    build_bundle(bytes, filename, source, cfg.max_cells, dir)
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

fn span_rect(s: &YamlValue) -> Option<(String, Rect)> {
    (s.get("kind").and_then(YamlValue::as_str) == Some("xlsx_range")).then_some(())?;
    let sheet = s.get("sheet").and_then(YamlValue::as_str)?.to_string();
    let range = Rect::parse(s.get("range").and_then(YamlValue::as_str)?)?;
    Some((sheet, range))
}

/// Comprobaciones XLSX del bundle: un concepto por hoja en el orden del libro, con su
/// nombre exacto; las regiones con concepto propio pertenecen a su hoja; cada celda con
/// contenido del original cae en alguna región de su hoja; cada CSV tiene tantas filas
/// como su región, y los índices enumeran los conceptos en orden.
pub(crate) fn validate(
    z: &mut ZipArchive<fs::File>,
    names: &HashSet<String>,
    original: &[u8],
) -> Result<()> {
    let mut zin = ZipArchive::new(Cursor::new(original))?;
    let wb = read::workbook(&mut zin)?;
    let mut expected: Vec<String> = Vec::new();
    for (i, info) in wb.sheets.iter().enumerate() {
        let n = i + 1;
        let sheet_file = format!("contenido/hoja-{n:03}.md");
        if !names.contains(&sheet_file) {
            bail!("falta el concepto de la hoja {n} («{}»)", info.name)
        }
        let prefix = format!("contenido/hoja-{n:03}-region-");
        let mut region_files: Vec<&String> = names
            .iter()
            .filter(|f| f.starts_with(&prefix) && f.ends_with(".md"))
            .collect();
        region_files.sort();
        let mut rects: Vec<Rect> = Vec::new();
        let mut used = None;
        for (k, f) in std::iter::once(&sheet_file)
            .chain(region_files.iter().copied())
            .enumerate()
        {
            let fm = frontmatter(&read_text(z, f)?)?;
            if fm.get("sheet_index").and_then(YamlValue::as_u64) != Some(n as u64) {
                bail!("{f} no declara sheet_index {n}")
            }
            if k == 0 && fm.get("title").and_then(YamlValue::as_str) != Some(info.name.as_str()) {
                bail!("{f} no lleva el nombre exacto de la hoja «{}»", info.name)
            }
            for s in fm
                .get("source_spans")
                .and_then(YamlValue::as_sequence)
                .map(Vec::as_slice)
                .unwrap_or_default()
            {
                let (sheet, rect) = span_rect(s)
                    .with_context(|| format!("{f} tiene un localizador xlsx_range mal formado"))?;
                if sheet != info.name {
                    bail!(
                        "{f} localiza la hoja «{sheet}» y pertenece a «{}»",
                        info.name
                    )
                }
                // El rango de la hoja la delimita; las celdas las cubren las regiones.
                if k == 0 && s.get("id").and_then(YamlValue::as_str) == Some("hoja") {
                    used = Some(rect);
                } else {
                    rects.push(rect);
                }
            }
            if k > 0 {
                let region = fm
                    .get("source_spans")
                    .and_then(YamlValue::as_sequence)
                    .and_then(|s| {
                        s.iter()
                            .find(|s| s.get("id").and_then(YamlValue::as_str) == Some("region"))
                    })
                    .and_then(span_rect)
                    .map(|(_, r)| r)
                    .with_context(|| format!("{f} carece de un localizador «region» válido"))?;
                let md = read_text(z, f)?;
                let csv = listed(&md, "](../assets/")
                    .into_iter()
                    .find(|p| p.ends_with(".csv"))
                    .with_context(|| format!("{f} no enlaza los datos completos en CSV"))?;
                let data = read_text(z, &format!("assets/{csv}"))?;
                if csv_records(&data) != region.rows() as usize {
                    bail!(
                        "el CSV de {f} tiene {} filas y la región {} tiene {}",
                        csv_records(&data),
                        region.a1(),
                        region.rows()
                    )
                }
            }
        }
        if let Some(u) = used
            && let Some(r) = rects.iter().find(|r| !u.contains_rect(r))
        {
            bail!(
                "{sheet_file} tiene un rango {} fuera del rango de la hoja",
                r.a1()
            )
        }
        if info.kind == SheetKind::Work
            && let Some(part) = &info.part
        {
            let sh = read::sheet_part(&mut zin, part, &wb, &mut usize::MAX.clone())?;
            if sh.used() != used {
                bail!(
                    "{sheet_file} no localiza el rango con contenido de la hoja «{}» ({})",
                    info.name,
                    sh.used().map(|u| u.a1()).unwrap_or_else(|| "vacía".into())
                )
            }
            if let Some(c) = sh
                .cells
                .iter()
                .find(|c| !rects.iter().any(|r| r.contains(c.row, c.col)))
            {
                bail!(
                    "la celda {}!{} del original no está en ninguna región del bundle",
                    info.name,
                    c.name()
                )
            }
        }
        expected.push(sheet_file.trim_start_matches("contenido/").to_string());
        expected.extend(
            region_files
                .iter()
                .map(|f| f.trim_start_matches("contenido/").to_string()),
        );
    }
    let content: HashSet<&String> = names
        .iter()
        .filter(|n| n.starts_with("contenido/") && n.ends_with(".md") && *n != "contenido/index.md")
        .collect();
    if content.len() != expected.len() {
        bail!("hay conceptos de contenido que no corresponden a ninguna hoja")
    }
    let root: Vec<String> = listed(&read_text(z, "index.md")?, "](contenido/")
        .into_iter()
        .filter(|f| f != "index.md")
        .collect();
    let index = listed(&read_text(z, "contenido/index.md")?, "](");
    if root != expected || index != expected {
        bail!("un índice no enumera los conceptos en el orden del libro")
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    /// Convierte un XLSX y devuelve el estado y el contenido de texto de cada entrada.
    #[test]
    fn a_link_to_a_missing_sheet_is_marked_and_declared() {
        use std::io::{Read, Write};
        let (_, md) = convert_example("examples/xlsx/muestras/ventas_trimestrales.xlsx");
        assert!(md["contenido/hoja-001.md"].contains("[Ver detalle por operación](hoja-002.md)"));
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("ventas_rotas.xlsx");
        let mut src = ZipArchive::new(
            fs::File::open("examples/xlsx/muestras/ventas_trimestrales.xlsx").unwrap(),
        )
        .unwrap();
        let mut dst = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        for i in 0..src.len() {
            let mut e = src.by_index(i).unwrap();
            let name = e.name().unwrap().into_owned();
            let mut data = Vec::new();
            e.read_to_end(&mut data).unwrap();
            if name == "xl/worksheets/sheet1.xml" {
                data = String::from_utf8(data)
                    .unwrap()
                    .replace("location=\"'Detalle'!A1\"", "location=\"'Falta'!A1\"")
                    .into_bytes();
            }
            dst.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            dst.write_all(&data).unwrap();
        }
        dst.finish().unwrap();
        let (_, md) = convert_example(path.to_str().unwrap());
        let sheet = &md["contenido/hoja-001.md"];
        assert!(
            sheet.contains(&format!("Ver detalle por operación {UNRESOLVED}")),
            "{sheet}"
        );
        assert!(
            md["conversion-report.md"]
                .contains("Hoja «Resumen»: 1 hipervínculo(s) interno(s) sin destino")
        );
    }

    #[test]
    fn workbook_and_sheet_protection_are_declared_and_not_applied() {
        use std::io::{Read, Write};
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("ventas_protegidas.xlsx");
        let mut src = ZipArchive::new(
            fs::File::open("examples/xlsx/muestras/ventas_trimestrales.xlsx").unwrap(),
        )
        .unwrap();
        let mut dst = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        for i in 0..src.len() {
            let mut e = src.by_index(i).unwrap();
            let name = e.name().unwrap().into_owned();
            let mut data = Vec::new();
            e.read_to_end(&mut data).unwrap();
            let text = String::from_utf8_lossy(&data).into_owned();
            if name == "xl/workbook.xml" {
                data = text
                    .replacen(
                        "<sheets>",
                        "<workbookProtection lockStructure=\"1\"/><sheets>",
                        1,
                    )
                    .into_bytes();
            } else if name == "xl/worksheets/sheet1.xml" {
                data = text
                    .replacen(
                        "</sheetData>",
                        "</sheetData><sheetProtection sheet=\"1\"/>",
                        1,
                    )
                    .into_bytes();
            }
            dst.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            dst.write_all(&data).unwrap();
        }
        dst.finish().unwrap();
        let (state, md) = convert_example(path.to_str().unwrap());
        assert_eq!(state, "complete");
        let report = &md["conversion-report.md"];
        assert!(report.contains("El original declara restricciones: el libro protege su estructura; el conversor no las aplica"), "{report}");
        assert!(report.contains("El original declara restricciones: la hoja «Resumen» está protegida; el conversor no las aplica"), "{report}");
        // El contenido se convierte entero.
        assert!(md["contenido/hoja-001.md"].contains("Ver detalle por operación"));
    }

    fn convert_example(path: &str) -> (String, HashMap<String, String>) {
        let bytes = fs::read(path).unwrap_or_else(|e| panic!("no se pudo leer {path}: {e}"));
        let tmp = tempfile::tempdir().unwrap();
        let name = FsPath::new(path)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let Built {
            path: zip,
            status: state,
            ..
        } = build_bundle(
            &bytes,
            &name,
            &crate::identity::Source::new(&hex(&Sha256::digest(&bytes))),
            1_000_000,
            tmp.path(),
        )
        .unwrap();
        let mut z = ZipArchive::new(fs::File::open(&zip).unwrap()).unwrap();
        let mut out = HashMap::new();
        for i in 0..z.len() {
            let name = z.by_index(i).unwrap().name().unwrap().into_owned();
            if name.ends_with(".md") || name.ends_with(".csv") || name.ends_with(".jsonl") {
                out.insert(name.clone(), read_text(&mut z, &name).unwrap());
            }
        }
        (state, out)
    }

    #[test]
    fn sales_workbook_keeps_formulas_tables_charts_comments_and_hidden_sheets() {
        let (state, md) = convert_example("examples/xlsx/muestras/ventas_trimestrales.xlsx");
        assert_eq!(state, "complete");
        let resumen = &md["contenido/hoja-001.md"];
        assert!(
            resumen.contains("## Resumen de ventas 2026 (A1:F8)"),
            "{resumen}"
        );
        assert!(resumen.contains("| Fila | Región | T1 | T2 | T3 | T4 | Variación T4/T1 |"));
        assert!(
            resumen
                .contains("| 4 | Andina | 120500 | 131200 | 128900 | 140250 | 16.3900414937759% |")
        );
        assert!(resumen.contains("- B8:E8: `=SUM(B4:B7)` en B8; las demás celdas repiten la fórmula con sus referencias relativas desplazadas (C8: `=SUM(C4:C7)`)"));
        assert!(resumen.contains("- F4:F8: `=E4/B4-1` en F4"));
        assert!(resumen.contains("[Ver detalle por operación](hoja-002.md)"));
        assert!(resumen.contains("[Tablero publicado](https://tablero.example.org/ventas-2026)"));
        assert!(resumen.contains("Ventas por región y trimestre. T1: Andina = 120500"));
        assert!(resumen.contains("`Resumen!$A$4:$A$7`"));
        assert!(resumen.contains("- B4:E7: valor de celda \\< 50000"));
        assert!(resumen.contains(
            "**Marta Ruiz**: Incluye la venta extraordinaria a la cadena hotelera. (celda B4)"
        ));
        assert!(resumen.contains("**Andrés Pineda** · 2026-10-20: ¿El total de T4 ya incluye las devoluciones? (celda E8)"));
        assert!(resumen.contains("  - **Marta Ruiz** · 2026-10-21: Sí, están descontadas"));
        assert!(!resumen.contains("Comentario encadenado"));
        assert!(resumen.contains("A1:F1"));
        let detalle = &md["contenido/hoja-002.md"];
        assert!(detalle.contains("[Tabla «TablaDetalle» (A1:G301)](hoja-002-region-01.md)"));
        let region = &md["contenido/hoja-002-region-01.md"];
        assert!(region.contains("type: \"Sheet Region\""));
        assert!(
            region.contains("| A | Fecha | fecha | 300 | 0 |"),
            "{region}"
        );
        assert!(region.contains("| F | Importe | número | 300 | 300 |"));
        assert!(region.contains("- F2:F301: `=D2*E2` en F2"));
        assert!(region.contains("Primeras 10 de 300 filas de datos; **no es la tabla completa**"));
        assert!(
            region.contains("| 2 | 2026-01-02 | Andina | Café especial | 5 | 18.5 | 92.5 | TRUE |")
        );
        let csv = &md["assets/hoja-002-region-01.csv"];
        assert_eq!(csv_records(csv), 301);
        assert!(csv.starts_with(
            "Fecha,Región,Producto,Unidades,Precio unitario,Importe,Entregado\r\n2026-01-02,Andina,"
        ));
        let records = &md["assets/hoja-002-region-01-celdas.jsonl"];
        assert!(records.contains(r##"{"almacenado":"92.5","celda":"F2","formato":"#,##0.00\\ \"€\"","formula":"=D2*E2","mostrado":"92.5","tipo":"número"}"##), "{}", &records[..600]);
        assert!(
            records.contains(r##""celda":"F3","formato":"#,##0.00\\ \"€\"","formula":"=D3*E3""##)
        );
        let supuestos = &md["contenido/hoja-003.md"];
        assert!(supuestos.contains("sheet_state: \"hidden\""));
        assert!(supuestos.contains("| 2 | Tasa de cambio objetivo | 3.5% |"));
        assert!(supuestos.contains("#DIV/0!"));
        let notas = &md["contenido/hoja-004.md"];
        assert!(notas.contains("## Región A1:A7"));
        assert!(
            notas.contains("- A5: 3. Las regiones siguen la división comercial vigente:<br>Andina")
        );
        let doc = &md["documento.md"];
        assert!(doc.contains("title: \"Ventas trimestrales 2026\""));
        assert!(doc.contains("- Tasa: `Supuestos!$B$2`"));
        assert!(!doc.contains("Print_Area"));
        let report = &md["conversion-report.md"];
        assert!(report.contains("- Hojas inventariadas: 4 (0 de gráfico; ocultas: 1)"));
        assert!(
            report.contains("Hoja «Detalle»: formato numérico #,##0.00\\\\ \"€\""),
            "{report}"
        );
        assert!(report.contains("valores de error almacenados: B4 (#DIV/0!)"));
    }

    #[test]
    fn inventory_splits_regions_and_keeps_pivots_formats_images_and_chartsheets() {
        let (state, md) = convert_example("examples/xlsx/muestras/inventario_regiones.xlsx");
        assert_eq!(state, "complete");
        let alm = &md["contenido/hoja-001.md"];
        assert!(alm.contains("## Almacén Norte (A1:C7)"), "{alm}");
        assert!(alm.contains("## Almacén Sur (E1:G4)"));
        assert!(alm.contains("## Pedidos pendientes (A10:D15)"));
        assert!(alm.contains("| 13 | P-2026-118 | Ferretería Central | 2026-11-04 | 1830.5 |"));
        assert!(alm.contains("- J20: Inventario físico pendiente"));
        assert!(alm.contains("- Ocultas: fila 14"));
        assert!(alm.contains("- Ocultas: columna F"));
        assert!(alm.contains(
            "![Plano de distribución del almacén Norte](../assets/hoja-001-imagen-01.png)"
        ));
        let din = &md["contenido/hoja-002.md"];
        assert!(
            din.contains("## Tabla dinámica «TablaDinámica1»: Existencias por proveedor (tabla dinámica) (A1:B6)"),
            "{din}"
        );
        assert!(din.contains("origen Almacenes!A12:D15"));
        let fmt = &md["contenido/hoja-003.md"];
        for expected in [
            "| 2 | Fecha | 2026-03-15 |",
            "| 3 | Fecha y hora | 2026-03-15 14:30:00 |",
            "| 4 | Hora | 14:30:00 |",
            "| 5 | Mes y año | 2026-07-01 |",
            "| 6 | Moneda | 1234.5 |",
            "| 7 | Porcentaje | 12.5% |",
            "| 8 | Miles | 9876543.21 |",
            "| 9 | Científico | 0.000123 |",
            "| 11 | Texto con ceros | 00123 |",
            "| 12 | Suma flotante | 0.3 |",
            "| 13 | Booleano | TRUE |",
            "| 14 | Fórmula de texto | Inventario Norte |",
            "| 15 | Celda con \\| barra | a \\| b |",
        ] {
            assert!(fmt.contains(expected), "{expected}\n{fmt}");
        }
        let chart = &md["contenido/hoja-004.md"];
        assert!(chart.contains("Hoja de gráfico 4 de 4"));
        assert!(
            chart.contains(
                "Existencias del almacén Norte. Existencias: Tornillo hexagonal M8 = 1250"
            )
        );
    }

    #[test]
    fn unrepresentable_content_is_declared_and_the_workbook_is_partial() {
        let (state, md) = convert_example("examples/xlsx/muestras/libro_con_perdidas.xlsx");
        assert_eq!(state, "partial");
        let pre = &md["contenido/hoja-001.md"];
        assert!(pre.contains("status: \"draft\""));
        assert!(pre.contains("\\[sin valor almacenado\\]"), "{pre}");
        assert!(pre.contains("- C2:C4: `=B2*(1+Tasa)` en C2; las demás celdas repiten la fórmula con sus referencias relativas desplazadas (C3: `=B3*(1+Tasa)`) — sin valor almacenado"));
        assert!(pre.contains("- B6: `=SUM(B2:B4*B2:B4)` — fórmula matricial sobre B6"));
        assert!(pre.contains("gráfico sin datos extraíbles"));
        assert!(pre.contains("_Imagen (anclada en F17) ausente del paquete original._"));
        assert!(pre.contains("Objeto incrustado (Word.Document.12)"));
        let empty = &md["contenido/hoja-002.md"];
        assert!(empty.contains("La hoja no tiene celdas con contenido."));
        let report = &md["conversion-report.md"];
        for expected in [
            "Vínculo externo \\[1\\] al libro file:///C:/Compartido/Tarifas/precios\\_2025.xlsx",
            "fórmulas sin valor almacenado en C2:C4 y D2:D4",
            "fórmulas en B2:B4 que leen el libro externo \\[1\\]",
            "objeto incrustado (Word.Document.12)",
            "El libro pide recalcular al abrirse",
            "Título del libro tomado del nombre del archivo",
        ] {
            assert!(report.contains(expected), "{expected}\n{report}");
        }
        assert!(md["documento.md"].contains("# libro\\_con\\_perdidas"));
    }
}
