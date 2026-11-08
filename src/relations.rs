//! Relaciones entre los conceptos de un bundle (plan 3, I4).
//!
//! OKF v0.2 expresa las relaciones con enlaces en el cuerpo y la jerarquía con los
//! directorios; no tiene campos para ellas. El perfil añade tres extensiones del
//! frontmatter a cada concepto de `contenido/`: `parent` (el concepto que lo contiene:
//! `documento.md` o, para una región grande de XLSX, el concepto de su hoja), y
//! `previous` y `next` (sus hermanos en el orden de lectura, si existen). Los valores son
//! rutas relativas a la raíz del bundle. `src/output.rs` los escribe; aquí se comprueban.
use anyhow::{Result, bail};
use std::collections::{HashMap, HashSet};
use yaml_serde::Value as YamlValue;

/// Relaciones declaradas por un concepto.
#[derive(Debug)]
pub(crate) struct Node {
    name: String,
    parent: Option<String>,
    previous: Option<String>,
    next: Option<String>,
}

/// Lee las relaciones de un concepto; deben ser rutas de texto si aparecen.
pub(crate) fn node(name: &str, fm: &YamlValue) -> Result<Node> {
    let field = |k: &str| -> Result<Option<String>> {
        match fm.get(k) {
            None => Ok(None),
            Some(v) => match v.as_str() {
                Some(s) => Ok(Some(s.to_string())),
                None => bail!("{name}: {k} no es una ruta"),
            },
        }
    };
    Ok(Node {
        name: name.to_string(),
        parent: field("parent")?,
        previous: field("previous")?,
        next: field("next")?,
    })
}

fn content(name: &str) -> bool {
    name.starts_with("contenido/") && name != "contenido/index.md"
}

/// Comprueba las relaciones de todos los conceptos frente a `contenido/index.md`.
pub(crate) fn check(nodes: &[Node], contents_index: &str) -> Result<()> {
    let by_name: HashMap<&str, &Node> = nodes.iter().map(|n| (n.name.as_str(), n)).collect();
    let mut children: HashMap<&str, Vec<&Node>> = HashMap::new();
    for n in nodes {
        if !content(&n.name) {
            if n.parent.is_some() || n.previous.is_some() || n.next.is_some() {
                bail!(
                    "{}: solo los conceptos de contenido/ declaran relaciones",
                    n.name
                )
            }
            continue;
        }
        let Some(parent) = n.parent.as_deref() else {
            bail!("{}: falta parent", n.name)
        };
        if parent != crate::output::DOCUMENT
            && !(content(parent) && by_name.contains_key(parent) && parent != n.name)
        {
            bail!(
                "{}: parent no es documento.md ni otro concepto del bundle",
                n.name
            )
        }
        for (k, sibling) in [("previous", &n.previous), ("next", &n.next)] {
            let Some(s) = sibling.as_deref() else {
                continue;
            };
            match by_name.get(s) {
                Some(o) if s != n.name && o.parent.as_deref() == Some(parent) => {}
                _ => bail!("{}: {k} no es un hermano con el mismo parent", n.name),
            }
        }
        // Reciprocidad: el siguiente me tiene como anterior y viceversa.
        if let Some(next) = n.next.as_deref()
            && by_name[next].previous.as_deref() != Some(n.name.as_str())
        {
            bail!("{}: next y previous no son recíprocos", n.name)
        }
        if let Some(previous) = n.previous.as_deref()
            && by_name[previous].next.as_deref() != Some(n.name.as_str())
        {
            bail!("{}: previous y next no son recíprocos", n.name)
        }
        children.entry(parent).or_default().push(n);
    }
    // Cada grupo de hermanos es una sola cadena, sin ciclos, que empieza por el único
    // sin `previous`.
    let mut chains: HashMap<&str, Vec<&str>> = HashMap::new();
    for (parent, group) in &children {
        let heads: Vec<&&Node> = group.iter().filter(|n| n.previous.is_none()).collect();
        let [head] = heads.as_slice() else {
            bail!("los hijos de {parent} no forman una sola cadena")
        };
        let mut chain = vec![head.name.as_str()];
        while let Some(next) = by_name[chain[chain.len() - 1]].next.as_deref() {
            if chain.len() >= group.len() {
                bail!("los hijos de {parent} forman un ciclo")
            }
            chain.push(next);
        }
        if chain.len() != group.len() {
            bail!("los hijos de {parent} no forman una sola cadena")
        }
        chains.insert(parent, chain);
    }
    // Recorrido en profundidad desde documento.md: cada concepto, y después sus hijos.
    let mut order = Vec::new();
    let mut seen = HashSet::new();
    let mut stack: Vec<&str> = chains
        .get(crate::output::DOCUMENT)
        .map(|c| c.iter().rev().copied().collect())
        .unwrap_or_default();
    while let Some(n) = stack.pop() {
        if !seen.insert(n) {
            bail!("la jerarquía de parent tiene un ciclo en {n}")
        }
        order.push(n);
        if let Some(c) = chains.get(n) {
            stack.extend(c.iter().rev());
        }
    }
    if order.len() != nodes.iter().filter(|n| content(&n.name)).count() {
        bail!("algún concepto de contenido/ no desciende de documento.md")
    }
    let listed: Vec<String> = crate::bundle::link_targets(contents_index)
        .into_iter()
        .filter_map(|t| crate::bundle::local_target("contenido/index.md", t))
        .filter(|t| by_name.contains_key(t.as_str()))
        .collect();
    if listed != order {
        bail!("el orden de parent, previous y next no coincide con contenido/index.md")
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(name: &str, parent: Option<&str>, previous: Option<&str>, next: Option<&str>) -> Node {
        Node {
            name: name.into(),
            parent: parent.map(Into::into),
            previous: previous.map(Into::into),
            next: next.map(Into::into),
        }
    }

    /// Una hoja con una región y otra hoja, como en XLSX.
    fn workbook() -> Vec<Node> {
        let d = Some("documento.md");
        vec![
            n("documento.md", None, None, None),
            n("conversion-report.md", None, None, None),
            n(
                "contenido/hoja-001.md",
                d,
                None,
                Some("contenido/hoja-002.md"),
            ),
            n(
                "contenido/hoja-001-region-01.md",
                Some("contenido/hoja-001.md"),
                None,
                None,
            ),
            n(
                "contenido/hoja-002.md",
                d,
                Some("contenido/hoja-001.md"),
                None,
            ),
        ]
    }
    const INDEX: &str =
        "# Contenido\n\n- [A](hoja-001.md)\n  - [R](hoja-001-region-01.md)\n- [B](hoja-002.md)\n";

    #[test]
    fn a_consistent_hierarchy_matches_the_index() {
        check(&workbook(), INDEX).unwrap();
    }

    #[test]
    fn broken_relations_are_rejected() {
        type Edit = Box<dyn Fn(&mut Vec<Node>)>;
        let cases: Vec<(&str, Edit, &str)> = vec![
            ("falta parent", Box::new(|v| v[2].parent = None), INDEX),
            (
                "no es documento.md ni otro concepto",
                Box::new(|v| v[3].parent = Some("otro/hoja.md".into())),
                INDEX,
            ),
            (
                "no son recíprocos",
                Box::new(|v| v[4].previous = None),
                INDEX,
            ),
            (
                "no es un hermano",
                Box::new(|v| v[2].next = Some("contenido/hoja-001-region-01.md".into())),
                INDEX,
            ),
            (
                "solo los conceptos de contenido/",
                Box::new(|v| v[0].next = Some("contenido/hoja-001.md".into())),
                INDEX,
            ),
            (
                "no coincide con contenido/index.md",
                Box::new(|_| {}),
                "- [B](hoja-002.md)\n- [A](hoja-001.md)\n  - [R](hoja-001-region-01.md)\n",
            ),
            (
                "no desciende de documento.md",
                Box::new(|v| {
                    v[2].parent = Some("contenido/hoja-001-region-01.md".into());
                    v[2].next = None;
                    v[4].previous = None;
                    v[3].parent = Some("contenido/hoja-001.md".into());
                }),
                INDEX,
            ),
        ];
        for (expected, edit, index) in cases {
            let mut v = workbook();
            edit(&mut v);
            let error = check(&v, index).unwrap_err().to_string();
            assert!(error.contains(expected), "{expected}: {error}");
        }
    }
}
