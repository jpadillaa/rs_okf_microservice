//! Árbol XML ligero para las partes OOXML. Las partes pequeñas (estilos, numeración,
//! notas, dibujos, definiciones de tablas) se leen enteras; las grandes se recorren en
//! streaming y solo se materializa un elemento cada vez.
use crate::ooxml::resolve_ref;
use anyhow::{Result, bail};
use quick_xml::{
    NsReader, XmlVersion,
    events::{BytesStart, Event, attributes::Attribute},
    name::{Namespace, ResolveResult},
};

/// Valor de un atributo con las entidades resueltas y los espacios normalizados como
/// exige XML 1.0; `None` si no puede decodificarse.
pub(crate) fn attr_value(a: &Attribute<'_>, e: &BytesStart<'_>) -> Option<String> {
    a.decoded_and_normalized_value(XmlVersion::Implicit1_0, e.decoder())
        .ok()
        .map(|v| v.into_owned())
}

/// Espacios de nombres de las relaciones del paquete (transicional y estricto).
const RELATIONSHIPS: &[&[u8]] = &[
    b"http://schemas.openxmlformats.org/officeDocument/2006/relationships",
    b"http://purl.oclc.org/ooxml/officeDocument/relationships",
];

#[derive(Clone, Debug)]
pub(crate) struct Attr {
    local: Vec<u8>,
    /// El atributo pertenece al espacio de relaciones (`r:id`, `r:embed`).
    rel: bool,
    value: String,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Node {
    /// Nombre local, sin prefijo.
    pub(crate) name: Vec<u8>,
    attrs: Vec<Attr>,
    pub(crate) children: Vec<Child>,
}

#[derive(Clone, Debug)]
pub(crate) enum Child {
    Node(Node),
    Text(String),
}

impl Node {
    pub(crate) fn is(&self, name: &[u8]) -> bool {
        self.name == name
    }
    /// Atributo por nombre local, fuera del espacio de relaciones (`w:val`, `w:id`).
    pub(crate) fn attr(&self, local: &[u8]) -> Option<&str> {
        self.attrs
            .iter()
            .find(|a| !a.rel && a.local == local)
            .map(|a| a.value.as_str())
    }
    /// Atributo del espacio de relaciones (`r:id`, `r:embed`, `r:link`).
    pub(crate) fn rel(&self, local: &[u8]) -> Option<&str> {
        self.attrs
            .iter()
            .find(|a| a.rel && a.local == local)
            .map(|a| a.value.as_str())
            .filter(|v| !v.is_empty())
    }
    pub(crate) fn nodes(&self) -> impl Iterator<Item = &Node> {
        self.children.iter().filter_map(|c| match c {
            Child::Node(n) => Some(n),
            Child::Text(_) => None,
        })
    }
    pub(crate) fn child(&self, name: &[u8]) -> Option<&Node> {
        self.nodes().find(|n| n.is(name))
    }
    /// Valor `w:val` de un hijo, p. ej. `w:pStyle` dentro de `w:pPr`.
    pub(crate) fn val(&self, name: &[u8]) -> Option<&str> {
        self.child(name)?.attr(b"val")
    }
    /// Texto directo del elemento.
    pub(crate) fn text(&self) -> String {
        self.children
            .iter()
            .filter_map(|c| match c {
                Child::Text(t) => Some(t.as_str()),
                Child::Node(_) => None,
            })
            .collect()
    }
    /// Descendientes con ese nombre, en orden de documento, sin entrar en los
    /// subárboles `stop` (p. ej. el contenido de un cuadro de texto).
    pub(crate) fn find_all<'a>(&'a self, name: &[u8], stop: &[u8], out: &mut Vec<&'a Node>) {
        for n in self.nodes() {
            if n.is(name) {
                out.push(n);
            } else if !n.is(stop) {
                n.find_all(name, stop, out);
            }
        }
    }
    pub(crate) fn find(&self, name: &[u8]) -> Option<&Node> {
        let mut out = Vec::new();
        self.find_all(name, b"", &mut out);
        out.into_iter().next()
    }
}

pub(crate) fn element(reader: &NsReader<&[u8]>, e: &BytesStart<'_>) -> Node {
    let attrs = e
        .attributes()
        .with_checks(false)
        .flatten()
        .map(|a| {
            let (ns, local) = reader.resolver().resolve_attribute(a.key);
            let rel =
                matches!(ns, ResolveResult::Bound(Namespace(n)) if RELATIONSHIPS.contains(&n));
            Attr {
                local: local.as_ref().to_vec(),
                rel,
                value: attr_value(&a, e).unwrap_or_default(),
            }
        })
        .collect();
    Node {
        name: e.local_name().as_ref().to_vec(),
        attrs,
        children: Vec::new(),
    }
}

/// Lee el subárbol de un elemento ya abierto hasta su cierre.
pub(crate) fn subtree(reader: &mut NsReader<&[u8]>, root: Node) -> Result<Node> {
    let mut stack = vec![root];
    loop {
        match reader.read_event()? {
            Event::Start(e) => {
                let n = element(reader, &e);
                stack.push(n);
            }
            Event::Empty(e) => {
                let n = element(reader, &e);
                if let Some(top) = stack.last_mut() {
                    top.children.push(Child::Node(n));
                }
            }
            Event::Text(t) => push_text(&mut stack, &t.decode()?),
            Event::CData(t) => push_text(&mut stack, &t.decode()?),
            Event::GeneralRef(r) => push_text(&mut stack, &resolve_ref(&r)?),
            Event::End(_) => {
                let done = stack.pop().unwrap_or_default();
                match stack.last_mut() {
                    Some(parent) => parent.children.push(Child::Node(done)),
                    None => return Ok(done),
                }
            }
            Event::Eof => bail!("XML truncado: falta el cierre de un elemento"),
            _ => {}
        }
    }
}

fn push_text(stack: &mut [Node], text: &str) {
    let Some(top) = stack.last_mut() else { return };
    match top.children.last_mut() {
        Some(Child::Text(t)) => t.push_str(text),
        _ => top.children.push(Child::Text(text.to_string())),
    }
}

/// Elemento raíz de una parte completa.
pub(crate) fn parse(xml: &str) -> Result<Node> {
    let mut reader = NsReader::from_str(xml);
    loop {
        match reader.read_event()? {
            Event::Start(e) => {
                let root = element(&reader, &e);
                return subtree(&mut reader, root);
            }
            Event::Empty(e) => return Ok(element(&reader, &e)),
            Event::Eof => bail!("parte XML sin elemento raíz"),
            _ => {}
        }
    }
}
