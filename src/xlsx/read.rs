//! Lectura de SpreadsheetML: libro, estilos numéricos, cadenas compartidas, hojas en
//! streaming y partes asociadas (tablas, tablas dinámicas, dibujos, comentarios).
use super::a1::{Rect, cell_name, col_name, parse_cell, shift_formula};
use crate::bundle::Comment;
use crate::ooxml::*;
use crate::xml::Node;
use anyhow::{Context, Result, bail};
use quick_xml::events::Event;
use std::{collections::HashMap, io::Cursor};
use zip::ZipArchive;

pub(crate) type Zip<'a> = ZipArchive<Cursor<&'a [u8]>>;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SheetKind {
    Work,
    Chart,
    /// Hoja de diálogo o de macros de Excel 4, fuera del perfil.
    Other(&'static str),
}

#[derive(Clone, Debug)]
pub(crate) struct SheetInfo {
    pub(crate) name: String,
    /// `hidden` o `veryHidden`; `None` si es visible.
    pub(crate) state: Option<String>,
    pub(crate) part: Option<String>,
    pub(crate) kind: SheetKind,
}

#[derive(Clone, Debug)]
pub(crate) struct DefinedName {
    pub(crate) name: String,
    pub(crate) value: String,
    /// Hoja a la que se limita el nombre, si no es global.
    pub(crate) scope: Option<usize>,
}

#[derive(Debug, Default)]
pub(crate) struct Workbook {
    pub(crate) sheets: Vec<SheetInfo>,
    pub(crate) names: Vec<DefinedName>,
    pub(crate) date1904: bool,
    /// El libro pide recalcular al abrirse (`fullCalcOnLoad`).
    pub(crate) recalc_on_load: bool,
    /// Destino de cada vínculo externo, en el orden de `[1]`, `[2]`…
    pub(crate) external: Vec<String>,
    pub(crate) title: Option<String>,
    pub(crate) persons: HashMap<String, String>,
    pub(crate) strings: Vec<String>,
    pub(crate) styles: Styles,
}

pub(crate) fn workbook(z: &mut Zip) -> Result<Workbook> {
    let part = main_part(z).unwrap_or_else(|| "xl/workbook.xml".into());
    let Some(root) = parse_entry(z, &part) else {
        bail!("el paquete no contiene un libro de Excel legible ({part})")
    };
    let rels = relationships(z, &part);
    let by_id: HashMap<&str, &Rel> = rels.iter().map(|r| (r.id.as_str(), r)).collect();
    let mut wb = Workbook::default();
    if let Some(sheets) = root.child(b"sheets") {
        for s in sheets.nodes().filter(|n| n.is(b"sheet")) {
            let rel = s.rel(b"id").and_then(|id| by_id.get(id));
            let kind = match rel.map(|r| r.kind.as_str()) {
                Some(k) if k.ends_with("/worksheet") => SheetKind::Work,
                Some(k) if k.ends_with("/chartsheet") => SheetKind::Chart,
                Some(k) if k.ends_with("/dialogsheet") => SheetKind::Other("hoja de diálogo"),
                Some(k) if k.to_ascii_lowercase().ends_with("macrosheet") => {
                    SheetKind::Other("hoja de macros de Excel 4")
                }
                _ => SheetKind::Other("hoja de tipo desconocido"),
            };
            wb.sheets.push(SheetInfo {
                name: s.attr(b"name").unwrap_or_default().to_string(),
                state: s
                    .attr(b"state")
                    .filter(|v| *v != "visible")
                    .map(str::to_string),
                part: rel
                    .filter(|r| !r.external)
                    .map(|r| resolve(&part, &r.target)),
                kind,
            });
        }
    }
    if let Some(pr) = root.child(b"workbookPr") {
        wb.date1904 = matches!(pr.attr(b"date1904"), Some("1") | Some("true"));
    }
    if let Some(calc) = root.child(b"calcPr") {
        wb.recalc_on_load = matches!(calc.attr(b"fullCalcOnLoad"), Some("1") | Some("true"));
    }
    if let Some(names) = root.child(b"definedNames") {
        for n in names.nodes().filter(|n| n.is(b"definedName")) {
            let name = n.attr(b"name").unwrap_or_default();
            let hidden = matches!(n.attr(b"hidden"), Some("1") | Some("true"));
            if hidden || name.starts_with("_xlnm.") || name.is_empty() {
                continue;
            }
            wb.names.push(DefinedName {
                name: name.to_string(),
                value: n.text(),
                scope: n.attr(b"localSheetId").and_then(|v| v.parse().ok()),
            });
        }
    }
    if let Some(refs) = root.child(b"externalReferences") {
        for r in refs.nodes().filter(|n| n.is(b"externalReference")) {
            let Some(link) = r
                .rel(b"id")
                .and_then(|id| by_id.get(id))
                .map(|r| resolve(&part, &r.target))
            else {
                continue;
            };
            let target = relationships(z, &link)
                .into_iter()
                .find(|r| r.external)
                .map(|r| r.target)
                .unwrap_or_else(|| "destino no declarado".into());
            wb.external.push(target);
        }
    }
    wb.title = parse_entry(z, "docProps/core.xml")
        .and_then(|c| c.find(b"title").map(Node::text))
        .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|t| !t.is_empty());
    for p in related(&rels, &part, "/person") {
        if let Some(list) = parse_entry(z, &p) {
            for person in list.nodes().filter(|n| n.is(b"person")) {
                if let (Some(id), Some(name)) = (person.attr(b"id"), person.attr(b"displayName")) {
                    wb.persons.insert(id.to_string(), name.to_string());
                }
            }
        }
    }
    if let Some(p) = related(&rels, &part, "/sharedStrings").first() {
        wb.strings = shared_strings(&String::from_utf8_lossy(&read_entry(z, p)?))?;
    }
    if let Some(p) = related(&rels, &part, "/styles").first()
        && let Some(root) = parse_entry(z, p)
    {
        wb.styles = Styles::parse(&root);
    }
    Ok(wb)
}

/// Texto de cada `si`, sin la guía fonética (`rPh`).
fn shared_strings(xml: &str) -> Result<Vec<String>> {
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut out = Vec::new();
    let mut current = String::new();
    let (mut in_t, mut in_phonetic) = (false, false);
    loop {
        match reader.read_event()? {
            Event::Start(e) => match e.local_name().as_ref() {
                b"si" => current.clear(),
                b"t" => in_t = true,
                b"rPh" => in_phonetic = true,
                _ => {}
            },
            Event::Empty(e) if e.local_name().as_ref() == b"si" => out.push(String::new()),
            Event::Text(t) if in_t && !in_phonetic => current.push_str(&t.decode()?),
            Event::GeneralRef(r) if in_t && !in_phonetic => current.push_str(&resolve_ref(&r)?),
            Event::End(e) => match e.local_name().as_ref() {
                b"si" => out.push(std::mem::take(&mut current)),
                b"t" => in_t = false,
                b"rPh" => in_phonetic = false,
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(out.into_iter().map(|s| unescape_x(&s)).collect())
}

/// Excel codifica los caracteres no permitidos en XML como `_x000D_`.
fn unescape_x(s: &str) -> String {
    if !s.contains("_x") {
        return s.to_string();
    }
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("_x") {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let code = tail
            .get(2..6)
            .filter(|h| h.chars().all(|c| c.is_ascii_hexdigit()));
        match (code, tail.get(6..7)) {
            (Some(h), Some("_")) => {
                if let Some(c) = u32::from_str_radix(h, 16).ok().and_then(char::from_u32) {
                    out.push(c);
                }
                rest = &tail[7..];
            }
            _ => {
                out.push_str("_x");
                rest = &tail[2..];
            }
        }
    }
    out.push_str(rest);
    out.replace("\r\n", "\n").replace('\r', "\n")
}

/// Significado del formato numérico de una celda.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Fmt {
    General,
    /// Fecha, hora o ambas: el número de serie se convierte a ISO 8601.
    Date {
        date: bool,
        time: bool,
    },
    Percent,
    /// Formato que solo cambia la presentación del número (decimales, miles, notación
    /// científica, fracciones): se conserva el valor.
    Plain,
    /// Formato con significado que el valor mostrado no reproduce (moneda, texto
    /// literal, duraciones); se declara con su código.
    Meaningful(String),
}

#[derive(Debug, Default)]
pub(crate) struct Styles {
    /// Significado y código de formato de cada estilo de celda (`cellXfs`).
    xf: Vec<(Fmt, String)>,
}

impl Styles {
    fn parse(root: &Node) -> Self {
        let custom: HashMap<u32, String> = root
            .child(b"numFmts")
            .map(|n| {
                n.nodes()
                    .filter(|f| f.is(b"numFmt"))
                    .filter_map(|f| {
                        Some((
                            f.attr(b"numFmtId")?.parse().ok()?,
                            f.attr(b"formatCode")?.to_string(),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let xf = root
            .child(b"cellXfs")
            .map(|x| {
                x.nodes()
                    .filter(|n| n.is(b"xf"))
                    .map(|n| {
                        let id: u32 = n
                            .attr(b"numFmtId")
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(0);
                        match custom.get(&id) {
                            Some(code) => (classify(code), code.clone()),
                            None => (builtin(id), format!("formato integrado {id}")),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        Styles { xf }
    }
    pub(crate) fn fmt(&self, style: u32) -> &Fmt {
        self.xf.get(style as usize).map_or(&Fmt::General, |x| &x.0)
    }
    /// Código del formato de un estilo que no es General.
    pub(crate) fn code(&self, style: u32) -> Option<&str> {
        self.xf
            .get(style as usize)
            .filter(|x| x.0 != Fmt::General)
            .map(|x| x.1.as_str())
    }
}

/// Formatos integrados de SpreadsheetML (ECMA-376, 18.8.30).
fn builtin(id: u32) -> Fmt {
    match id {
        0 | 49 => Fmt::General,
        1..=4 | 11 | 12 | 13 | 37..=40 | 48 => Fmt::Plain,
        5..=8 | 41..=44 => Fmt::Meaningful(format!("moneda (formato integrado {id})")),
        9 | 10 => Fmt::Percent,
        14..=17 | 27..=31 | 34..=36 | 50..=58 => Fmt::Date {
            date: true,
            time: false,
        },
        18..=21 | 32 | 33 | 45 | 47 => Fmt::Date {
            date: false,
            time: true,
        },
        22 => Fmt::Date {
            date: true,
            time: true,
        },
        46 => Fmt::Meaningful("duración [h]:mm:ss (formato integrado 46)".into()),
        _ => Fmt::General,
    }
}

/// Clasifica un código de formato propio por su primera sección.
fn classify(code: &str) -> Fmt {
    let section = code.split(';').next().unwrap_or_default();
    let mut plain = String::new();
    let mut literal = false;
    let mut currency = false;
    let mut elapsed = false;
    let mut chars = section.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                let text: String = chars.by_ref().take_while(|&c| c != '"').collect();
                literal |= !text.trim().is_empty();
            }
            '\\' => {
                if let Some(n) = chars.next() {
                    literal |=
                        !n.is_whitespace() && !matches!(n, '-' | '(' | ')' | ':' | '/' | ',' | '.');
                    currency |= matches!(n, '€' | '$' | '£' | '¥');
                }
            }
            '[' => {
                let inner: String = chars.by_ref().take_while(|&c| c != ']').collect();
                let lower = inner.to_ascii_lowercase();
                if lower.starts_with('h') || lower.starts_with('m') || lower.starts_with('s') {
                    elapsed |= lower.chars().all(|c| matches!(c, 'h' | 'm' | 's'));
                }
                if let Some(sym) = inner.strip_prefix('$') {
                    currency |= !sym.split('-').next().unwrap_or_default().is_empty();
                }
            }
            '_' | '*' => {
                chars.next();
            }
            '€' | '$' | '£' | '¥' => currency = true,
            c => plain.push(c.to_ascii_lowercase()),
        }
    }
    if elapsed {
        return Fmt::Meaningful(code.to_string());
    }
    if plain.contains('%') {
        return Fmt::Percent;
    }
    let date = plain.contains('y') || plain.contains('d');
    let time = plain.contains('h') || plain.contains('s');
    if date || time || (plain.contains('m') && !plain.contains('0') && !plain.contains('#')) {
        return Fmt::Date {
            date: date || !time,
            time,
        };
    }
    if plain == "general" || plain == "@" || plain.is_empty() && !literal && !currency {
        return Fmt::General;
    }
    if currency || literal {
        return Fmt::Meaningful(code.to_string());
    }
    Fmt::Plain
}

/// Tipo del valor almacenado.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Kind {
    Number,
    Text,
    Bool,
    Error,
    /// Fecha ISO almacenada como tal (`t="d"`).
    Date,
    /// Fórmula sin valor almacenado.
    NoValue,
}

impl Kind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Kind::Number => "número",
            Kind::Text => "texto",
            Kind::Bool => "booleano",
            Kind::Error => "error",
            Kind::Date => "fecha",
            Kind::NoValue => "sin valor",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Cell {
    pub(crate) row: u32,
    pub(crate) col: u32,
    pub(crate) kind: Kind,
    /// Valor tal como está almacenado en el archivo.
    pub(crate) raw: String,
    /// Valor para leer, si difiere del almacenado: fechas en ISO 8601, porcentajes,
    /// números con la precisión de Excel (15 cifras), booleanos `TRUE`/`FALSE`.
    pub(crate) display: Option<String>,
    /// Fórmula con `=`, ya desplazada si es una celda dependiente de una compartida.
    pub(crate) formula: Option<String>,
    /// Grupo de fórmula compartida al que pertenece.
    pub(crate) shared: Option<u32>,
    /// Rango de una fórmula matricial, en su celda maestra.
    pub(crate) array: Option<Rect>,
    pub(crate) style: u32,
}

impl Cell {
    pub(crate) fn name(&self) -> String {
        cell_name(self.row, self.col)
    }
    /// Valor para leer.
    pub(crate) fn shown(&self) -> &str {
        self.display.as_deref().unwrap_or(&self.raw)
    }
    /// La fecha se reconoce por su formato o por su tipo almacenado.
    pub(crate) fn is_date(&self, styles: &Styles) -> bool {
        self.kind == Kind::Date
            || (self.kind == Kind::Number && matches!(styles.fmt(self.style), Fmt::Date { .. }))
    }
}

/// Grupo de fórmula compartida: celda maestra y fórmula.
#[derive(Clone, Debug)]
pub(crate) struct Shared {
    pub(crate) anchor: (u32, u32),
    pub(crate) formula: String,
}

#[derive(Clone, Debug)]
pub(crate) enum Link {
    External(String),
    /// Ubicación dentro del libro, p. ej. `'Detalle'!A1`.
    Internal(String),
}

#[derive(Debug, Default)]
pub(crate) struct Sheet {
    pub(crate) cells: Vec<Cell>,
    pub(crate) merges: Vec<Rect>,
    pub(crate) hidden_rows: Vec<u32>,
    pub(crate) hidden_cols: Vec<u32>,
    pub(crate) links: Vec<(Rect, Link)>,
    /// Rango y descripción de cada regla de formato condicional.
    pub(crate) conditional: Vec<(String, String)>,
    pub(crate) shared: HashMap<u32, Shared>,
    pub(crate) drawing: Option<String>,
    pub(crate) tables: Vec<String>,
    /// ProgID de cada objeto incrustado (OLE).
    pub(crate) objects: Vec<String>,
}

impl Sheet {
    /// Rango que contiene todas las celdas con contenido.
    pub(crate) fn used(&self) -> Option<Rect> {
        self.cells
            .iter()
            .map(|c| Rect::cell(c.row, c.col))
            .reduce(|a, b| a.union(&b))
    }
}

/// Número con la precisión que muestra Excel (15 cifras significativas), sin ruido de
/// coma flotante: `0.30000000000000004` → `0.3`.
pub(crate) fn number(raw: &str) -> String {
    match raw.trim().parse::<f64>() {
        Ok(x) if x.is_finite() => {
            let rounded: f64 = format!("{x:.14e}").parse().unwrap_or(x);
            let s = format!("{rounded}");
            if s == "-0" { "0".into() } else { s }
        }
        _ => raw.to_string(),
    }
}

/// Fecha u hora ISO 8601 de un número de serie de Excel.
pub(crate) fn serial_date(raw: &str, date: bool, time: bool, date1904: bool) -> Option<String> {
    let x: f64 = raw.trim().parse().ok()?;
    if !x.is_finite() || x < 0.0 || x > 2_958_466.0 {
        return None;
    }
    let mut days = x.floor() as i64;
    let mut secs = ((x - x.floor()) * 86_400.0).round() as i64;
    if secs >= 86_400 {
        days += 1;
        secs -= 86_400;
    }
    let base = if date1904 {
        chrono::NaiveDate::from_ymd_opt(1904, 1, 1)?
    } else {
        // El sistema 1900 cuenta el inexistente 29 de febrero de 1900 (serie 60).
        if days < 61 {
            days += 1;
        }
        chrono::NaiveDate::from_ymd_opt(1899, 12, 30)?
    };
    let d = base.checked_add_signed(chrono::TimeDelta::days(days))?;
    let t = format!(
        "{:02}:{:02}:{:02}",
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    );
    Some(match (date, time) {
        (true, true) => format!("{} {t}", d.format("%Y-%m-%d")),
        (false, true) => t,
        _ => d.format("%Y-%m-%d").to_string(),
    })
}

fn display(kind: Kind, raw: &str, fmt: &Fmt, date1904: bool) -> String {
    match kind {
        Kind::Bool => if raw.trim() == "1" { "TRUE" } else { "FALSE" }.into(),
        Kind::Date => raw
            .strip_suffix("T00:00:00")
            .map(str::to_string)
            .unwrap_or_else(|| raw.replace('T', " ")),
        Kind::Number => match fmt {
            Fmt::Date { date, time } => {
                serial_date(raw, *date, *time, date1904).unwrap_or_else(|| number(raw))
            }
            Fmt::Percent => match raw.trim().parse::<f64>() {
                Ok(x) => format!("{}%", number(&format!("{}", x * 100.0))),
                Err(_) => raw.to_string(),
            },
            _ => number(raw),
        },
        Kind::Text | Kind::Error | Kind::NoValue => raw.to_string(),
    }
}

/// Lee una hoja directamente desde su entrada del paquete, sin cargar el XML entero.
pub(crate) fn sheet_part(
    z: &mut Zip,
    part: &str,
    wb: &Workbook,
    budget: &mut usize,
) -> Result<Sheet> {
    let entry = z
        .by_name(part)
        .with_context(|| format!("falta la parte {part} del paquete"))?;
    if entry.size() > limits().entry_bytes() {
        bail!("una parte del paquete excede el límite de expansión (OKF_MAX_ZIP_ENTRY_MB)");
    }
    sheet(std::io::BufReader::new(entry), wb, budget)
}

/// Lee una hoja en streaming. `budget` es el número de celdas con contenido que aún
/// se admiten en el libro.
pub(crate) fn sheet(
    src: impl std::io::BufRead,
    wb: &Workbook,
    budget: &mut usize,
) -> Result<Sheet> {
    let mut reader = quick_xml::Reader::from_reader(src);
    let mut buf = Vec::new();
    let mut sh = Sheet::default();
    let mut row = 0u32;
    let mut col = 0u32;
    // Estado de la celda abierta.
    let mut cur: Option<(u32, u32, Option<String>, u32)> = None;
    let mut value = String::new();
    let mut inline = String::new();
    let mut formula: Option<OpenFormula> = None;
    let mut text_target: u8 = 0; // 1 = v, 2 = f, 3 = is/t
    let mut in_phonetic = false;
    let mut cf: Option<(String, Vec<String>)> = None;
    let mut rule: Option<(String, Option<String>, Vec<String>)> = None;
    let mut ole_seen = std::collections::HashSet::new();
    let mut has_value = false;
    let mut has_inline = false;
    loop {
        buf.clear();
        let ev = reader.read_event_into(&mut buf)?;
        let (start, empty) = match &ev {
            Event::Start(_) => (true, false),
            Event::Empty(_) => (true, true),
            _ => (false, false),
        };
        match ev {
            Event::Start(ref e) | Event::Empty(ref e) if start => {
                match e.local_name().as_ref() {
                    b"row" => {
                        row = attr(e, b"r")
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(row + 1);
                        col = 0;
                        if matches!(attr(e, b"hidden").as_deref(), Some("1") | Some("true")) {
                            sh.hidden_rows.push(row);
                        }
                    }
                    b"c" => {
                        let (r, c) = attr(e, b"r")
                            .and_then(|v| parse_cell(&v))
                            .unwrap_or((row.max(1), col + 1));
                        row = r;
                        col = c;
                        let style = attr(e, b"s").and_then(|v| v.parse().ok()).unwrap_or(0);
                        cur = Some((r, c, attr(e, b"t"), style));
                        value.clear();
                        inline.clear();
                        formula = None;
                        has_value = false;
                        has_inline = false;
                        if empty {
                            cur = None;
                        }
                    }
                    b"v" if cur.is_some() => {
                        text_target = 1;
                        has_value = true;
                    }
                    b"f" if cur.is_some() => {
                        formula = Some(OpenFormula {
                            text: String::new(),
                            kind: attr(e, b"t"),
                            range: attr(e, b"ref"),
                            si: attr(e, b"si").and_then(|v| v.parse().ok()),
                        });
                        text_target = if empty { 0 } else { 2 };
                    }
                    b"is" if cur.is_some() => has_inline = true,
                    b"t" if has_inline => text_target = 3,
                    b"rPh" => in_phonetic = true,
                    b"mergeCell" => {
                        if let Some(r) = attr(e, b"ref").and_then(|v| Rect::parse(&v)) {
                            sh.merges.push(r);
                        }
                    }
                    b"col" => {
                        if matches!(attr(e, b"hidden").as_deref(), Some("1") | Some("true")) {
                            let min: u32 =
                                attr(e, b"min").and_then(|v| v.parse().ok()).unwrap_or(0);
                            let max: u32 =
                                attr(e, b"max").and_then(|v| v.parse().ok()).unwrap_or(min);
                            // Un intervalo enorme suele ser «ocultar el resto de columnas».
                            sh.hidden_cols.extend(min..=max.min(min + 255));
                        }
                    }
                    b"hyperlink" => {
                        if let Some(r) = attr(e, b"ref").and_then(|v| Rect::parse(&v)) {
                            let link = match (attr(e, b"id"), attr(e, b"location")) {
                                (Some(id), _) => Some(Link::External(id)),
                                (None, Some(loc)) => Some(Link::Internal(loc)),
                                _ => None,
                            };
                            if let Some(l) = link {
                                sh.links.push((r, l));
                            }
                        }
                    }
                    b"conditionalFormatting" => {
                        cf = Some((attr(e, b"sqref").unwrap_or_default(), Vec::new()));
                    }
                    b"cfRule" => {
                        rule = Some((
                            attr(e, b"type").unwrap_or_default(),
                            attr(e, b"operator"),
                            Vec::new(),
                        ));
                        if empty && let (Some(r), Some((_, rules))) = (rule.take(), cf.as_mut()) {
                            rules.push(rule_text(&r.0, r.1.as_deref(), &r.2));
                        }
                    }
                    b"formula" if rule.is_some() => text_target = 4,
                    b"drawing" => sh.drawing = attr(e, b"id"),
                    b"tablePart" => sh.tables.extend(attr(e, b"id")),
                    b"oleObject" => {
                        let key = attr(e, b"shapeId")
                            .or_else(|| attr(e, b"id"))
                            .unwrap_or_default();
                        if ole_seen.insert(key) {
                            sh.objects.push(
                                attr(e, b"progId").unwrap_or_else(|| "objeto sin ProgID".into()),
                            );
                        }
                    }
                    _ => {}
                }
                if empty && e.local_name().as_ref() == b"f" {
                    text_target = 0;
                }
            }
            Event::Text(t) => {
                if in_phonetic {
                    continue;
                }
                let s = t.decode()?;
                push_target(
                    text_target,
                    &s,
                    &mut value,
                    &mut formula,
                    &mut inline,
                    &mut rule,
                );
            }
            Event::GeneralRef(r) => {
                if in_phonetic {
                    continue;
                }
                let s = resolve_ref(&r)?;
                push_target(
                    text_target,
                    &s,
                    &mut value,
                    &mut formula,
                    &mut inline,
                    &mut rule,
                );
            }
            Event::End(e) => match e.local_name().as_ref() {
                b"v" | b"f" | b"t" | b"formula" => text_target = 0,
                b"rPh" => in_phonetic = false,
                b"cfRule" => {
                    if let (Some(r), Some((_, rules))) = (rule.take(), cf.as_mut()) {
                        rules.push(rule_text(&r.0, r.1.as_deref(), &r.2));
                    }
                }
                b"conditionalFormatting" => {
                    if let Some((sqref, rules)) = cf.take() {
                        for r in rules {
                            sh.conditional.push((sqref.replace(' ', ", "), r));
                        }
                    }
                }
                b"c" => {
                    let Some((r, c, t, style)) = cur.take() else {
                        continue;
                    };
                    if let Some(cell) = finish_cell(
                        r,
                        c,
                        t.as_deref(),
                        style,
                        has_value.then_some(value.as_str()),
                        has_inline.then_some(inline.as_str()),
                        formula.take(),
                        wb,
                        &mut sh,
                    ) {
                        if *budget == 0 {
                            bail!("el libro supera el máximo de celdas con contenido admitido");
                        }
                        *budget -= 1;
                        sh.cells.push(cell);
                    }
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    // Las celdas dependientes de una fórmula compartida reciben su fórmula desplazada.
    for cell in sh.cells.iter_mut() {
        if cell.formula.is_none()
            && let Some(g) = cell.shared.and_then(|si| sh.shared.get(&si))
        {
            let dr = i64::from(cell.row) - i64::from(g.anchor.0);
            let dc = i64::from(cell.col) - i64::from(g.anchor.1);
            cell.formula = shift_formula(&g.formula[1..], dr, dc).map(|f| format!("={f}"));
        }
    }
    sh.hidden_cols.sort_unstable();
    sh.hidden_cols.dedup();
    Ok(sh)
}

/// Fórmula de la celda abierta: texto y atributos `t`, `ref` y `si`.
struct OpenFormula {
    text: String,
    kind: Option<String>,
    range: Option<String>,
    si: Option<u32>,
}

fn push_target(
    target: u8,
    s: &str,
    value: &mut String,
    formula: &mut Option<OpenFormula>,
    inline: &mut String,
    rule: &mut Option<(String, Option<String>, Vec<String>)>,
) {
    match target {
        1 => value.push_str(s),
        2 => {
            if let Some(f) = formula.as_mut() {
                f.text.push_str(s)
            }
        }
        3 => inline.push_str(s),
        4 => {
            if let Some(r) = rule.as_mut() {
                match r.2.last_mut() {
                    Some(last) if !s.is_empty() => last.push_str(s),
                    _ => r.2.push(s.to_string()),
                }
            }
        }
        _ => {}
    }
}

/// Descripción legible de una regla de formato condicional.
fn rule_text(kind: &str, operator: Option<&str>, formulas: &[String]) -> String {
    let op = match operator.unwrap_or_default() {
        "lessThan" => "<",
        "lessThanOrEqual" => "≤",
        "greaterThan" => ">",
        "greaterThanOrEqual" => "≥",
        "equal" => "=",
        "notEqual" => "≠",
        "between" => "entre",
        "notBetween" => "fuera de",
        other => other,
    };
    let args = formulas.join(" y ");
    match kind {
        "cellIs" => format!("valor de celda {op} {args}"),
        "expression" => format!("fórmula {args}"),
        "colorScale" => "escala de color".into(),
        "dataBar" => "barras de datos".into(),
        "iconSet" => "conjunto de iconos".into(),
        "top10" => "valores superiores o inferiores".into(),
        "duplicateValues" => "valores duplicados".into(),
        "uniqueValues" => "valores únicos".into(),
        "containsText" | "beginsWith" | "endsWith" | "notContainsText" => {
            format!("texto ({kind}) {args}")
        }
        other => format!("regla {other} {args}").trim_end().to_string(),
    }
}

#[allow(clippy::too_many_arguments)]
fn finish_cell(
    r: u32,
    c: u32,
    t: Option<&str>,
    style: u32,
    value: Option<&str>,
    inline: Option<&str>,
    formula: Option<OpenFormula>,
    wb: &Workbook,
    sh: &mut Sheet,
) -> Option<Cell> {
    let mut shared = None;
    let mut array = None;
    let mut text = None;
    if let Some(OpenFormula {
        text: f,
        kind,
        range,
        si,
    }) = formula
    {
        match kind.as_deref() {
            Some("shared") => {
                shared = si;
                if !f.trim().is_empty()
                    && let Some(si) = si
                {
                    sh.shared.insert(
                        si,
                        Shared {
                            anchor: (r, c),
                            formula: format!("={f}"),
                        },
                    );
                }
            }
            Some("array") => array = range.as_deref().and_then(Rect::parse),
            _ => {}
        }
        if !f.trim().is_empty() {
            text = Some(format!("={f}"));
        }
    }
    let has_formula = text.is_some() || shared.is_some();
    let (kind, raw) = match (t, value, inline) {
        (Some("inlineStr"), _, Some(s)) => (Kind::Text, unescape_x(s)),
        (Some("s"), Some(v), _) => (
            Kind::Text,
            v.trim()
                .parse::<usize>()
                .ok()
                .and_then(|i| wb.strings.get(i))
                .cloned()
                .unwrap_or_default(),
        ),
        (Some("str"), Some(v), _) => (Kind::Text, unescape_x(v)),
        (Some("b"), Some(v), _) => (Kind::Bool, v.trim().to_string()),
        (Some("e"), Some(v), _) => (Kind::Error, v.trim().to_string()),
        (Some("d"), Some(v), _) => (Kind::Date, v.trim().to_string()),
        (_, Some(v), _) if !v.trim().is_empty() => (Kind::Number, v.trim().to_string()),
        _ if has_formula => (Kind::NoValue, String::new()),
        _ => return None,
    };
    if kind == Kind::Text && raw.is_empty() && !has_formula {
        return None;
    }
    let display =
        Some(display(kind, &raw, wb.styles.fmt(style), wb.date1904)).filter(|d| *d != raw);
    Some(Cell {
        row: r,
        col: c,
        kind,
        raw,
        display,
        formula: text,
        shared,
        array,
        style,
    })
}

/// Tabla de Excel (ListObject).
#[derive(Clone, Debug)]
pub(crate) struct TableDef {
    pub(crate) name: String,
    pub(crate) range: Rect,
    pub(crate) header_rows: u32,
    pub(crate) totals_rows: u32,
}

pub(crate) fn table(z: &mut Zip, part: &str) -> Option<TableDef> {
    let root = parse_entry(z, part)?;
    Some(TableDef {
        name: root
            .attr(b"displayName")
            .or_else(|| root.attr(b"name"))
            .unwrap_or("Tabla")
            .to_string(),
        range: Rect::parse(root.attr(b"ref")?)?,
        header_rows: root
            .attr(b"headerRowCount")
            .and_then(|v| v.parse().ok())
            .unwrap_or(1),
        totals_rows: root
            .attr(b"totalsRowCount")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
    })
}

#[derive(Clone, Debug)]
pub(crate) struct PivotDef {
    pub(crate) name: String,
    pub(crate) range: Rect,
    /// Origen de los datos: `Hoja!A1:D9` o el nombre de una tabla.
    pub(crate) source: Option<String>,
}

pub(crate) fn pivot(z: &mut Zip, part: &str) -> Option<PivotDef> {
    let root = parse_entry(z, part)?;
    let range = Rect::parse(root.child(b"location")?.attr(b"ref")?)?;
    let source = related(&relationships(z, part), part, "/pivotCacheDefinition")
        .first()
        .and_then(|p| parse_entry(z, p))
        .and_then(|cache| {
            let ws = cache.find(b"worksheetSource")?;
            match (ws.attr(b"sheet"), ws.attr(b"ref"), ws.attr(b"name")) {
                (Some(s), Some(r), _) => Some(format!("{s}!{r}")),
                (_, _, Some(n)) => Some(n.to_string()),
                (None, Some(r), _) => Some(r.to_string()),
                _ => None,
            }
        });
    Some(PivotDef {
        name: root.attr(b"name").unwrap_or("Tabla dinámica").to_string(),
        range,
        source,
    })
}

/// Objeto de un dibujo, con la celda donde está anclado.
#[derive(Clone, Debug)]
pub(crate) enum Drawn {
    Chart {
        anchor: Option<(u32, u32)>,
        name: String,
        part: Option<String>,
    },
    Image {
        anchor: Option<(u32, u32)>,
        target: Option<String>,
        linked: bool,
        alt: Option<String>,
    },
    Shape {
        anchor: Option<(u32, u32)>,
        text: String,
    },
}

/// Ramas de `mc:AlternateContent` que el lector conoce.
const SUPPORTED_CHOICES: &[&str] = &["a14", "x14", "sle15", "a16"];

pub(crate) fn drawing(z: &mut Zip, part: &str) -> Vec<Drawn> {
    let Some(root) = parse_entry(z, part) else {
        return Vec::new();
    };
    let rels = relationships(z, part);
    let mut out = Vec::new();
    for anchor in root.nodes() {
        let cell = anchor.child(b"from").and_then(|f| {
            let col: u32 = f.child(b"col")?.text().trim().parse().ok()?;
            let row: u32 = f.child(b"row")?.text().trim().parse().ok()?;
            Some((row + 1, col + 1))
        });
        collect(anchor, cell, &rels, part, &mut out);
    }
    out
}

fn collect(n: &Node, anchor: Option<(u32, u32)>, rels: &[Rel], part: &str, out: &mut Vec<Drawn>) {
    for c in n.nodes() {
        if c.is(b"AlternateContent") {
            let choice = c.nodes().find(|b| {
                b.is(b"Choice")
                    && b.attr(b"Requires")
                        .unwrap_or_default()
                        .split_whitespace()
                        .all(|r| SUPPORTED_CHOICES.contains(&r))
            });
            let branch = choice.or_else(|| c.child(b"Fallback"));
            if let Some(b) = branch {
                collect(b, anchor, rels, part, out);
            }
        } else if c.is(b"graphicFrame") {
            let name = c
                .find(b"cNvPr")
                .and_then(|p| p.attr(b"name"))
                .unwrap_or("Gráfico")
                .to_string();
            if let Some(chart) = c.find(b"chart") {
                let part = chart
                    .rel(b"id")
                    .and_then(|id| rels.iter().find(|r| r.id == id))
                    .map(|r| resolve(part, &r.target));
                out.push(Drawn::Chart { anchor, name, part });
            }
        } else if c.is(b"pic") {
            let blip = c.find(b"blip");
            let (rid, linked) = match blip {
                Some(b) => match (b.rel(b"embed"), b.rel(b"link")) {
                    (Some(id), _) => (Some(id), false),
                    (None, Some(id)) => (Some(id), true),
                    _ => (None, false),
                },
                None => (None, false),
            };
            let target = rid
                .and_then(|id| rels.iter().find(|r| r.id == id))
                .map(|r| {
                    if r.external {
                        r.target.clone()
                    } else {
                        resolve(part, &r.target)
                    }
                });
            out.push(Drawn::Image {
                anchor,
                target,
                linked: linked
                    || rid.is_some_and(|id| rels.iter().any(|r| r.id == id && r.external)),
                alt: meaningful_alt(c.find(b"cNvPr").and_then(|p| p.attr(b"descr"))),
            });
        } else if c.is(b"sp") {
            let mut paras = Vec::new();
            if let Some(body) = c.find(b"txBody") {
                for p in body.nodes().filter(|p| p.is(b"p")) {
                    let mut ts = Vec::new();
                    p.find_all(b"t", b"", &mut ts);
                    let line: String = ts.iter().map(|t| t.text()).collect();
                    if !line.trim().is_empty() {
                        paras.push(line.trim().to_string());
                    }
                }
            }
            if !paras.is_empty() {
                out.push(Drawn::Shape {
                    anchor,
                    text: paras.join("\n"),
                });
            }
        } else if c.is(b"grpSp") {
            collect(c, anchor, rels, part, out);
        }
    }
}

/// Referencias de datos de un gráfico (`c:f` de sus series), sin repetir.
pub(crate) fn chart_sources(xml: &str) -> Vec<String> {
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut out: Vec<String> = Vec::new();
    let mut in_f = false;
    let mut text = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) if e.local_name().as_ref() == b"f" => {
                in_f = true;
                text.clear();
            }
            Ok(Event::Text(t)) if in_f => text.push_str(&t.decode().unwrap_or_default()),
            Ok(Event::GeneralRef(r)) if in_f => text.push_str(&resolve_ref(&r).unwrap_or_default()),
            Ok(Event::End(e)) if e.local_name().as_ref() == b"f" => {
                in_f = false;
                let f = text.trim().to_string();
                if !f.is_empty() && !out.contains(&f) {
                    out.push(f);
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}

/// Comentarios de una hoja: los encadenados (con respuestas) y los clásicos que no
/// duplican un hilo. Cada comentario lleva su celda.
pub(crate) fn comments(
    z: &mut Zip,
    rels: &[Rel],
    part: &str,
    wb: &Workbook,
) -> Vec<(String, Comment)> {
    let mut out: Vec<(String, Comment)> = Vec::new();
    let mut threaded_refs = std::collections::HashSet::new();
    for p in related(rels, part, "/threadedComment") {
        let Some(root) = parse_entry(z, &p) else {
            continue;
        };
        let mut index: HashMap<String, usize> = HashMap::new();
        for c in root.nodes().filter(|n| n.is(b"threadedComment")) {
            let cell = c.attr(b"ref").unwrap_or_default().to_string();
            let comment = Comment {
                author: c
                    .attr(b"personId")
                    .and_then(|id| wb.persons.get(id))
                    .cloned(),
                date: c.attr(b"dT").map(|d| d.chars().take(10).collect()),
                text: c.child(b"text").map(Node::text).unwrap_or_default(),
                replies: Vec::new(),
            };
            match c.attr(b"parentId").and_then(|p| index.get(p)).copied() {
                Some(i) => out[i].1.replies.push(comment),
                None => {
                    if let Some(id) = c.attr(b"id") {
                        index.insert(id.to_string(), out.len());
                    }
                    threaded_refs.insert(cell.clone());
                    out.push((cell, comment));
                }
            }
        }
    }
    for p in related(rels, part, "/comments") {
        let Some(root) = parse_entry(z, &p) else {
            continue;
        };
        let authors: Vec<String> = root
            .child(b"authors")
            .map(|a| a.nodes().map(Node::text).collect())
            .unwrap_or_default();
        let Some(list) = root.child(b"commentList") else {
            continue;
        };
        for c in list.nodes().filter(|n| n.is(b"comment")) {
            let cell = c.attr(b"ref").unwrap_or_default().to_string();
            let author = c
                .attr(b"authorId")
                .and_then(|i| i.parse::<usize>().ok())
                .and_then(|i| authors.get(i))
                .cloned();
            if threaded_refs.contains(&cell)
                || author.as_deref().is_some_and(|a| a.starts_with("tc="))
            {
                continue;
            }
            let mut ts = Vec::new();
            if let Some(t) = c.child(b"text") {
                t.find_all(b"t", b"rPh", &mut ts);
            }
            let mut text: String = unescape_x(&ts.iter().map(|t| t.text()).collect::<String>());
            // Excel antepone el autor en negrita: «Autor:\n».
            if let Some(a) = &author
                && let Some(rest) = text.strip_prefix(&format!("{a}:"))
            {
                text = rest.trim_start().to_string();
            }
            out.push((
                cell,
                Comment {
                    author,
                    date: None,
                    text: text.trim().to_string(),
                    replies: Vec::new(),
                },
            ));
        }
    }
    out.sort_by_key(|(cell, _)| parse_cell(cell).unwrap_or((0, 0)));
    out
}

/// Nombre de hoja de una ubicación interna (`'Mi hoja'!A1`, `Detalle!B2`).
pub(crate) fn location_sheet(loc: &str) -> Option<String> {
    let (sheet, _) = loc.rsplit_once('!')?;
    let s = sheet.trim_start_matches('#');
    Some(
        match s.strip_prefix('\'').and_then(|x| x.strip_suffix('\'')) {
            Some(q) => q.replace("''", "'"),
            None => s.to_string(),
        },
    )
}

/// «columna F» o «columnas C, F y H».
pub(crate) fn columns_phrase(cols: &[u32]) -> String {
    let names: Vec<String> = cols.iter().map(|&c| col_name(c)).collect();
    match names.as_slice() {
        [one] => format!("columna {one}"),
        [rest @ .., last] => format!("columnas {} y {last}", rest.join(", ")),
        [] => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_formats_are_classified_by_meaning() {
        assert_eq!(
            classify("dd/mm/yyyy\\ hh:mm"),
            Fmt::Date {
                date: true,
                time: true
            }
        );
        assert_eq!(
            classify("mmmm\\ yyyy"),
            Fmt::Date {
                date: true,
                time: false
            }
        );
        assert_eq!(
            classify("h:mm:ss"),
            Fmt::Date {
                date: false,
                time: true
            }
        );
        assert_eq!(classify("0.0%"), Fmt::Percent);
        assert_eq!(classify("#,##0.00"), Fmt::Plain);
        assert_eq!(classify("[Red]#,##0;[Blue]-#,##0"), Fmt::Plain);
        assert!(matches!(classify("#,##0.00\\ \"€\""), Fmt::Meaningful(_)));
        assert!(matches!(
            classify("[$€-es-ES] #,##0.00"),
            Fmt::Meaningful(_)
        ));
        assert_eq!(
            classify("[$-es-ES]mmmm yyyy"),
            Fmt::Date {
                date: true,
                time: false
            }
        );
        assert!(matches!(classify("[h]:mm"), Fmt::Meaningful(_)));
        assert_eq!(classify("General"), Fmt::General);
    }

    #[test]
    fn values_use_excel_precision_and_iso_dates() {
        assert_eq!(number("0.30000000000000004"), "0.3");
        assert_eq!(number("1234.5600000000001"), "1234.56");
        assert_eq!(number("120500"), "120500");
        assert_eq!(
            serial_date("46096", true, false, false).unwrap(),
            "2026-03-15"
        );
        assert_eq!(
            serial_date("46096.604166666664", true, true, false).unwrap(),
            "2026-03-15 14:30:00"
        );
        assert_eq!(
            serial_date("0.6041666666666666", false, true, false).unwrap(),
            "14:30:00"
        );
        assert_eq!(serial_date("59", true, false, false).unwrap(), "1900-02-28");
        assert_eq!(serial_date("61", true, false, false).unwrap(), "1900-03-01");
        assert_eq!(serial_date("0", true, false, true).unwrap(), "1904-01-01");
        assert_eq!(display(Kind::Number, "0.035", &Fmt::Percent, false), "3.5%");
        assert_eq!(unescape_x("a_x000D_\nb_x0041_"), "a\nbA");
    }
}
