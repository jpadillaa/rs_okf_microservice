//! Recorrido del cuerpo de WordprocessingML. El cuerpo del documento se recorre en
//! streaming y solo se materializa un bloque de primer nivel cada vez, para acotar la
//! memoria.
pub(crate) use crate::xml::{Child, Node};
use crate::xml::{element, subtree};
use anyhow::{Result, bail};
use quick_xml::{NsReader, events::Event};

/// Contenedores transparentes del cuerpo: su contenido cuenta como bloques propios.
fn transparent(n: &Node) -> bool {
    n.is(b"sdt") || n.is(b"sdtContent") || n.is(b"customXml")
}

/// Galería de un control de contenido de bloque (`docPartGallery`), p. ej. «Table of
/// Contents».
fn gallery(sdt: &Node) -> Option<String> {
    sdt.child(b"sdtPr")?
        .child(b"docPartObj")?
        .val(b"docPartGallery")
        .map(str::to_string)
}

type BodyFn<'a> = dyn FnMut(Node, Option<&str>) -> Result<()> + 'a;

fn expand(n: Node, inherited: Option<&str>, f: &mut BodyFn) -> Result<()> {
    if transparent(&n) {
        let own = if n.is(b"sdt") { gallery(&n) } else { None };
        let current = own.as_deref().or(inherited);
        for c in n.children {
            if let Child::Node(c) = c
                && !c.is(b"sdtPr")
                && !c.is(b"sdtEndPr")
                && !c.is(b"customXmlPr")
            {
                expand(c, current, f)?;
            }
        }
        Ok(())
    } else {
        f(n, inherited)
    }
}

/// Recorre los elementos de primer nivel de `w:body` en orden, abriendo los
/// contenedores transparentes (controles de contenido, XML personalizado); cada
/// elemento llega con la galería del control que lo contiene, si la hay.
pub(crate) fn body_elements(
    xml: &str,
    mut f: impl FnMut(Node, Option<&str>) -> Result<()>,
) -> Result<()> {
    let mut reader = NsReader::from_str(xml);
    let mut in_body = false;
    loop {
        match reader.read_event()? {
            Event::Start(e) if !in_body => in_body = e.local_name().as_ref() == b"body",
            Event::Start(e) => {
                let root = element(&reader, &e);
                let node = subtree(&mut reader, root)?;
                expand(node, None, &mut f)?;
            }
            Event::Empty(e) if in_body => expand(element(&reader, &e), None, &mut f)?,
            Event::End(e) if in_body && e.local_name().as_ref() == b"body" => return Ok(()),
            Event::Eof => bail!("document.xml no contiene w:body"),
            _ => {}
        }
    }
}

/// Bloques del cuerpo en orden estructural: cada párrafo y cada tabla de primer nivel.
/// Es la numeración de los localizadores `docx_blocks`.
pub(crate) fn is_block(n: &Node) -> bool {
    n.is(b"p") || n.is(b"tbl")
}
