//! Ecuaciones OMML (`m:oMath`) en notación lineal, comunes a PPTX y DOCX.
use crate::ooxml::{attr, resolve_ref};
use quick_xml::events::Event;

#[derive(Default)]
pub(crate) struct MathNode {
    pub(crate) name: Vec<u8>,
    pub(crate) val: Option<String>,
    pub(crate) text: String,
    pub(crate) children: Vec<MathNode>,
}

impl MathNode {
    fn child(&self, name: &[u8]) -> Option<&MathNode> {
        self.children.iter().find(|c| c.name == name)
    }
    /// Valor `m:val` de una propiedad, p. ej. `m:begChr` dentro de `m:dPr`.
    fn prop(&self, pr: &[u8], name: &[u8]) -> Option<String> {
        self.child(pr)?.child(name)?.val.clone()
    }
}

/// Notación lineal aproximada a UnicodeMath: `c^2`, `a/b`, `√(x)`, `∑_(i=1)^n`.
pub(crate) fn linear(n: &MathNode) -> String {
    let part = |name: &[u8]| n.child(name).map(linear).unwrap_or_default();
    let wrap = |name: &[u8]| {
        let s = part(name);
        if s.chars().count() <= 1 || s.chars().all(|c| c.is_ascii_digit()) {
            s
        } else {
            format!("({s})")
        }
    };
    match n.name.as_slice() {
        b"t" => n.text.clone(),
        b"sSup" => format!("{}^{}", wrap(b"e"), wrap(b"sup")),
        b"sSub" => format!("{}_{}", wrap(b"e"), wrap(b"sub")),
        b"sSubSup" => format!("{}_{}^{}", wrap(b"e"), wrap(b"sub"), wrap(b"sup")),
        b"sPre" => format!("_{}^{}{}", wrap(b"sub"), wrap(b"sup"), wrap(b"e")),
        b"f" => format!("{}/{}", wrap(b"num"), wrap(b"den")),
        b"rad" => match part(b"deg") {
            d if d.is_empty() => format!("√{}", wrap(b"e")),
            d => format!("√({d}&{})", part(b"e")),
        },
        b"d" => {
            let open = n.prop(b"dPr", b"begChr").unwrap_or_else(|| "(".into());
            let close = n.prop(b"dPr", b"endChr").unwrap_or_else(|| ")".into());
            let sep = n.prop(b"dPr", b"sepChr").unwrap_or_else(|| "|".into());
            let items: Vec<String> = n
                .children
                .iter()
                .filter(|c| c.name == b"e")
                .map(linear)
                .collect();
            format!("{open}{}{close}", items.join(&sep))
        }
        b"nary" => {
            let op = n.prop(b"naryPr", b"chr").unwrap_or_else(|| "∫".into());
            let mut s = op;
            if n.child(b"sub").is_some_and(|c| !linear(c).is_empty()) {
                s.push('_');
                s.push_str(&wrap(b"sub"));
            }
            if n.child(b"sup").is_some_and(|c| !linear(c).is_empty()) {
                s.push('^');
                s.push_str(&wrap(b"sup"));
            }
            format!("{s} {}", part(b"e"))
        }
        b"func" => format!("{} {}", part(b"fName"), part(b"e")),
        b"limLow" => format!("{}_{}", part(b"e"), wrap(b"lim")),
        b"limUpp" => format!("{}^{}", part(b"e"), wrap(b"lim")),
        b"acc" => {
            let chr = n.prop(b"accPr", b"chr").unwrap_or_else(|| "\u{302}".into());
            format!("{}{chr}", part(b"e"))
        }
        b"m" => {
            let rows: Vec<String> = n
                .children
                .iter()
                .filter(|c| c.name == b"mr")
                .map(|r| {
                    r.children
                        .iter()
                        .filter(|c| c.name == b"e")
                        .map(linear)
                        .collect::<Vec<_>>()
                        .join("&")
                })
                .collect();
            format!("■({})", rows.join("@"))
        }
        b"eqArr" => {
            let rows: Vec<String> = n
                .children
                .iter()
                .filter(|c| c.name == b"e")
                .map(linear)
                .collect();
            format!("█({})", rows.join("@"))
        }
        name if name.ends_with(b"Pr") => String::new(),
        // Tras un subíndice o superíndice, un espacio separa lo que sigue («q_i d_i»).
        _ => {
            let mut out = String::new();
            let mut scripted = false;
            for c in &n.children {
                let s = linear(c);
                if scripted && s.starts_with(char::is_alphanumeric) {
                    out.push(' ');
                }
                if !s.is_empty() {
                    scripted = matches!(c.name.as_slice(), b"sSub" | b"sSup" | b"sSubSup");
                }
                out.push_str(&s);
            }
            out
        }
    }
}

/// Captura el subárbol de un `m:oMath` evento a evento.
#[derive(Default)]
pub(crate) struct Capture {
    stack: Vec<MathNode>,
}

impl Capture {
    /// Hay una ecuación abierta; los eventos le pertenecen.
    pub(crate) fn active(&self) -> bool {
        !self.stack.is_empty()
    }
    /// Recibe un evento dentro de la ecuación (el primero es la apertura de `m:oMath`);
    /// devuelve la notación lineal cuando la ecuación se cierra.
    pub(crate) fn feed(&mut self, event: &Event<'_>) -> anyhow::Result<Option<String>> {
        match event {
            Event::Start(e) => self.stack.push(MathNode {
                name: e.local_name().as_ref().to_vec(),
                val: attr(e, b"val"),
                ..MathNode::default()
            }),
            Event::Empty(e) => {
                if let Some(top) = self.stack.last_mut() {
                    top.children.push(MathNode {
                        name: e.local_name().as_ref().to_vec(),
                        val: attr(e, b"val"),
                        ..MathNode::default()
                    });
                }
            }
            Event::Text(t) => {
                if let Some(top) = self.stack.last_mut().filter(|n| n.name == b"t") {
                    top.text.push_str(&t.decode()?);
                }
            }
            Event::GeneralRef(r) => {
                if let Some(top) = self.stack.last_mut().filter(|n| n.name == b"t") {
                    top.text.push_str(&resolve_ref(r)?);
                }
            }
            Event::End(_) => {
                if let Some(node) = self.stack.pop() {
                    match self.stack.last_mut() {
                        Some(parent) => parent.children.push(node),
                        None => return Ok(Some(linear(&node))),
                    }
                }
            }
            Event::Eof => self.stack.clear(),
            _ => {}
        }
        Ok(None)
    }
}
