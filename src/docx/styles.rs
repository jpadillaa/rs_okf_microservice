//! Estilos y numeración de WordprocessingML: el formato efectivo de cada ejecución,
//! el nivel de esquema de cada párrafo y las etiquetas de las listas.
use super::xml::Node;
use std::collections::HashMap;

/// Propiedades de ejecución que importan al conversor; `None` hereda.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct RunProps {
    pub(crate) bold: Option<bool>,
    pub(crate) italic: Option<bool>,
    pub(crate) vanish: Option<bool>,
    /// Medios puntos (`w:sz`).
    pub(crate) size: Option<u32>,
}

/// Interruptor OOXML: presente sin valor o con un valor distinto de falso.
fn on(n: &Node) -> bool {
    !matches!(n.attr(b"val"), Some("0" | "false" | "off" | "none"))
}

impl RunProps {
    pub(crate) fn from_rpr(rpr: Option<&Node>) -> Self {
        let mut p = RunProps::default();
        let Some(rpr) = rpr else { return p };
        for n in rpr.nodes() {
            match n.name.as_slice() {
                b"b" => p.bold = Some(on(n)),
                b"i" => p.italic = Some(on(n)),
                b"vanish" => p.vanish = Some(on(n)),
                b"sz" => p.size = n.attr(b"val").and_then(|v| v.parse().ok()),
                _ => {}
            }
        }
        p
    }
    /// Estas propiedades con `top` encima: gana lo que `top` define.
    pub(crate) fn over(&self, top: &RunProps) -> RunProps {
        RunProps {
            bold: top.bold.or(self.bold),
            italic: top.italic.or(self.italic),
            vanish: top.vanish.or(self.vanish),
            size: top.size.or(self.size),
        }
    }
}

#[derive(Clone, Debug, Default)]
struct Style {
    name: String,
    based_on: Option<String>,
    outline: Option<u8>,
    num: Option<(String, u8)>,
    run: RunProps,
}

#[derive(Debug, Default)]
pub(crate) struct Styles {
    styles: HashMap<String, Style>,
    defaults: RunProps,
    default_para: Option<String>,
}

/// `numPr` de un párrafo o estilo: identificador de numeración y nivel.
pub(crate) fn num_pr(ppr: Option<&Node>) -> Option<(String, u8)> {
    let n = ppr?.child(b"numPr")?;
    let id = n.val(b"numId")?.to_string();
    let lvl = n.val(b"ilvl").and_then(|v| v.parse().ok()).unwrap_or(0);
    Some((id, lvl))
}

impl Styles {
    pub(crate) fn parse(root: &Node) -> Self {
        let mut out = Styles::default();
        if let Some(rpr) = root
            .child(b"docDefaults")
            .and_then(|d| d.child(b"rPrDefault"))
            .and_then(|d| d.child(b"rPr"))
        {
            out.defaults = RunProps::from_rpr(Some(rpr));
        }
        for s in root.nodes().filter(|n| n.is(b"style")) {
            let Some(id) = s.attr(b"styleId") else {
                continue;
            };
            let ppr = s.child(b"pPr");
            if s.attr(b"type") == Some("paragraph") && s.attr(b"default").is_some_and(|d| d == "1")
            {
                out.default_para = Some(id.to_string());
            }
            out.styles.insert(
                id.to_string(),
                Style {
                    name: s.val(b"name").unwrap_or_default().to_string(),
                    based_on: s.val(b"basedOn").map(str::to_string),
                    outline: ppr
                        .and_then(|p| p.val(b"outlineLvl"))
                        .and_then(|v| v.parse().ok()),
                    num: num_pr(ppr),
                    run: RunProps::from_rpr(s.child(b"rPr")),
                },
            );
        }
        out
    }
    /// El estilo y sus bases, del más derivado al más general.
    fn chain(&self, id: Option<&str>) -> Vec<&Style> {
        let mut out = Vec::new();
        let mut next = id.or(self.default_para.as_deref());
        while let Some(s) = next.and_then(|i| self.styles.get(i)) {
            if out.len() >= 16 {
                break;
            }
            out.push(s);
            next = s.based_on.as_deref();
        }
        out
    }
    /// Formato heredado por las ejecuciones de un párrafo con ese estilo.
    pub(crate) fn para_run(&self, id: Option<&str>) -> RunProps {
        self.chain(id)
            .iter()
            .rev()
            .fold(self.defaults.clone(), |acc, s| acc.over(&s.run))
    }
    /// Formato de un estilo de carácter.
    pub(crate) fn char_run(&self, id: &str) -> RunProps {
        self.chain(Some(id))
            .iter()
            .rev()
            .fold(RunProps::default(), |acc, s| acc.over(&s.run))
    }
    /// Nivel de esquema declarado o heredado (0 = encabezado de nivel 1, 9 = texto
    /// corriente); los estilos integrados «heading N» lo tienen aunque no lo declaren.
    pub(crate) fn outline(&self, id: Option<&str>) -> Option<u8> {
        for s in self.chain(id) {
            if let Some(l) = s.outline {
                return Some(l.min(9));
            }
            let name = s.name.trim().to_lowercase();
            if let Some(n) = name.strip_prefix("heading ")
                && let Ok(n) = n.trim().parse::<u8>()
                && (1..=9).contains(&n)
            {
                return Some(n - 1);
            }
        }
        None
    }
    /// Estilo de código o texto preformateado (por nombre, en cualquier punto de la cadena).
    pub(crate) fn is_code(&self, id: Option<&str>) -> bool {
        id.is_some()
            && self.chain(id).iter().any(|s| {
                let name = s.name.to_lowercase();
                [
                    "code",
                    "código",
                    "codigo",
                    "preformat",
                    "plain text",
                    "source",
                ]
                .iter()
                .any(|k| name.contains(k))
            })
    }
    /// El estilo es (o deriva de) el estilo integrado «Title».
    pub(crate) fn is_title(&self, id: Option<&str>) -> bool {
        id.is_some()
            && self
                .chain(id)
                .iter()
                .any(|s| s.name.trim().eq_ignore_ascii_case("title"))
    }
    pub(crate) fn numbering(&self, id: Option<&str>) -> Option<(String, u8)> {
        self.chain(id).iter().find_map(|s| s.num.clone())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Level {
    pub(crate) fmt: String,
    pub(crate) text: String,
    pub(crate) start: u32,
}

#[derive(Debug, Default)]
struct Num {
    abstract_id: String,
    starts: HashMap<u8, u32>,
    levels: HashMap<u8, Level>,
}

#[derive(Debug, Default)]
pub(crate) struct Numbering {
    abstracts: HashMap<String, HashMap<u8, Level>>,
    nums: HashMap<String, Num>,
}

fn level_of(l: &Node) -> Option<(u8, Level)> {
    let ilvl = l.attr(b"ilvl")?.parse().ok()?;
    Some((
        ilvl,
        Level {
            fmt: l.val(b"numFmt").unwrap_or("decimal").to_string(),
            text: l.val(b"lvlText").unwrap_or_default().to_string(),
            start: l.val(b"start").and_then(|v| v.parse().ok()).unwrap_or(1),
        },
    ))
}

impl Numbering {
    pub(crate) fn parse(root: &Node) -> Self {
        let mut out = Numbering::default();
        for n in root.nodes() {
            if n.is(b"abstractNum")
                && let Some(id) = n.attr(b"abstractNumId")
            {
                out.abstracts.insert(
                    id.to_string(),
                    n.nodes()
                        .filter(|l| l.is(b"lvl"))
                        .filter_map(level_of)
                        .collect(),
                );
            } else if n.is(b"num")
                && let Some(id) = n.attr(b"numId")
            {
                let mut num = Num {
                    abstract_id: n.val(b"abstractNumId").unwrap_or_default().to_string(),
                    ..Num::default()
                };
                for o in n.nodes().filter(|o| o.is(b"lvlOverride")) {
                    let Some(ilvl) = o.attr(b"ilvl").and_then(|v| v.parse().ok()) else {
                        continue;
                    };
                    if let Some(s) = o.val(b"startOverride").and_then(|v| v.parse().ok()) {
                        num.starts.insert(ilvl, s);
                    }
                    if let Some((_, l)) = o.child(b"lvl").and_then(level_of) {
                        num.levels.insert(ilvl, l);
                    }
                }
                out.nums.insert(id.to_string(), num);
            }
        }
        out
    }
    /// Definición efectiva de un nivel, con las sustituciones de la instancia.
    pub(crate) fn level(&self, num: &str, ilvl: u8) -> Option<Level> {
        let n = self.nums.get(num)?;
        let mut l = n
            .levels
            .get(&ilvl)
            .or_else(|| self.abstracts.get(&n.abstract_id)?.get(&ilvl))?
            .clone();
        if let Some(s) = n.starts.get(&ilvl) {
            l.start = *s;
        }
        Some(l)
    }
}

/// Contadores de las listas a lo largo del documento.
#[derive(Default)]
pub(crate) struct Counters {
    values: HashMap<String, [Option<u32>; 9]>,
}

/// Número de una lista en el formato del nivel.
pub(crate) fn format_number(n: u32, fmt: &str) -> String {
    fn letters(mut n: u32) -> String {
        let mut s = String::new();
        while n > 0 {
            n -= 1;
            s.insert(0, (b'a' + (n % 26) as u8) as char);
            n /= 26;
        }
        s
    }
    fn roman(mut n: u32) -> String {
        let table = [
            (1000, "m"),
            (900, "cm"),
            (500, "d"),
            (400, "cd"),
            (100, "c"),
            (90, "xc"),
            (50, "l"),
            (40, "xl"),
            (10, "x"),
            (9, "ix"),
            (5, "v"),
            (4, "iv"),
            (1, "i"),
        ];
        let mut s = String::new();
        for (v, r) in table {
            while n >= v {
                s.push_str(r);
                n -= v;
            }
        }
        s
    }
    match fmt {
        "lowerLetter" => letters(n),
        "upperLetter" => letters(n).to_uppercase(),
        "lowerRoman" => roman(n),
        "upperRoman" => roman(n).to_uppercase(),
        "decimalZero" => format!("{n:02}"),
        _ => n.to_string(),
    }
}

/// Elemento de lista ya numerado.
#[derive(Clone, Debug)]
pub(crate) struct ListLabel {
    pub(crate) level: u8,
    /// Número para una lista ordenada Markdown cuando la etiqueta es «N.».
    pub(crate) ordinal: Option<u32>,
    /// Etiqueta visible cuando no es una viñeta ni un «N.» simple (p. ej. «a)», «1.2.»).
    pub(crate) label: Option<String>,
}

impl Counters {
    /// Avanza el contador del nivel, reinicia los niveles inferiores y devuelve la
    /// etiqueta del elemento.
    pub(crate) fn next(&mut self, numbering: &Numbering, num: &str, ilvl: u8) -> ListLabel {
        let ilvl = ilvl.min(8);
        let level = numbering.level(num, ilvl);
        let values = self.values.entry(num.to_string()).or_default();
        let start = level.as_ref().map_or(1, |l| l.start);
        let i = ilvl as usize;
        values[i] = Some(values[i].map_or(start, |v| v + 1));
        for v in values.iter_mut().skip(i + 1) {
            *v = None;
        }
        let Some(level) = level else {
            return ListLabel {
                level: ilvl,
                ordinal: None,
                label: None,
            };
        };
        if matches!(level.fmt.as_str(), "bullet" | "none") {
            return ListLabel {
                level: ilvl,
                ordinal: None,
                label: None,
            };
        }
        let own = values[i].unwrap_or(start);
        if level.fmt == "decimal" && level.text == format!("%{}.", i + 1) {
            return ListLabel {
                level: ilvl,
                ordinal: Some(own),
                label: None,
            };
        }
        let mut text = level.text.clone();
        for k in (0..=i).rev() {
            let fmt = numbering
                .level(num, k as u8)
                .map(|l| l.fmt)
                .unwrap_or_else(|| "decimal".into());
            let v = values[k].unwrap_or(1);
            text = text.replace(&format!("%{}", k + 1), &format_number(v, &fmt));
        }
        ListLabel {
            level: ilvl,
            ordinal: None,
            label: (!text.trim().is_empty()).then(|| text.trim().to_string()),
        }
    }
}
