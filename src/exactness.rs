//! Exactitud del texto PDF frente a referencias exactas (`examples/pdf/exactitud/`): el
//! mismo cálculo que `tools/exactitud.py` con una referencia exacta. La cobertura dice qué
//! palabras llegan; esto, además, si llegan en su orden y sin mezclarse. Regenerar los
//! golden no cambia los mínimos de `examples/exactitud-thresholds.json`, que se actualizan
//! a mano.
use crate::coverage::{markdown_text, without_generated, words};
use regex::Regex;
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Path, PathBuf},
    sync::LazyLock,
};
use yaml_serde::Value as YamlValue;

static MARKER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<!-- okf-source-span: ([^ ]+) -->").unwrap());
static IMAGES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"!\[[^\]]*\]\([^)]*\)").unwrap());

type Words = Vec<String>;

/// Palabras del original en un bloque: sin notas ni rótulos del conversor.
fn block_words(segment: &str) -> Words {
    let text = without_generated(segment);
    let text = IMAGES
        .replace_all(&text, " ")
        .replace("_Texto girado:_", " ");
    words(&markdown_text(&text))
}

/// Bloques del bundle: páginas que ocupa cada uno y sus palabras, en el orden del Markdown.
fn blocks(files: &BTreeMap<String, String>) -> Vec<(usize, usize, Words)> {
    let mut out = Vec::new();
    for (name, text) in files {
        let base = name.rsplit('/').next().unwrap();
        if !name.ends_with(".md") || matches!(base, "index.md" | "conversion-report.md") {
            continue;
        }
        let meta = crate::bundle::frontmatter(text).unwrap();
        let body = &text[4..][text[4..].find("\n---\n").unwrap() + 5..];
        let mut spans: Vec<YamlValue> = meta
            .get("source_spans")
            .and_then(YamlValue::as_sequence)
            .cloned()
            .unwrap_or_default();
        if let Some(file) = meta.get("block_spans").and_then(YamlValue::as_str) {
            let json: serde_json::Value =
                serde_json::from_str(&files[file.trim_start_matches('/')]).unwrap();
            for span in json["source_spans"].as_array().unwrap() {
                spans.push(yaml_serde::to_value(span).unwrap());
            }
        }
        let ranges: HashMap<String, (usize, usize)> = spans
            .iter()
            .filter(|s| s.get("kind").and_then(YamlValue::as_str) == Some("pdf_pages"))
            .filter_map(|s| {
                let page = |k| s.get(k).and_then(YamlValue::as_u64).map(|n| n as usize);
                Some((
                    s.get("id")?.as_str()?.to_string(),
                    (page("from")?, page("to")?),
                ))
            })
            .collect();
        let markers: Vec<_> = MARKER.captures_iter(body).collect();
        let head = &body[..markers
            .first()
            .map_or(body.len(), |m| m.get(0).unwrap().start())];
        if name != "documento.md"
            && let Some(&(first, _)) = ranges.get("seccion")
        {
            // El título del concepto está en la página donde empieza.
            out.push((first, first, block_words(head)));
        }
        for (k, m) in markers.iter().enumerate() {
            let start = m.get(0).unwrap().end();
            let end = markers
                .get(k + 1)
                .map_or(body.len(), |n| n.get(0).unwrap().start());
            let mut segment = &body[start..end];
            let id = &m[1];
            if name == "documento.md" {
                // El marcador cubre solo el título; lo demás no es texto de una página.
                if id != "titulo" {
                    continue;
                }
                let trimmed = segment.trim_start_matches('\n');
                segment = trimmed.split("\n\n").next().unwrap_or_default();
            }
            if let Some(&(first, last)) = ranges.get(id) {
                out.push((first, last, block_words(segment)));
            }
        }
    }
    out
}

fn matched(reference: &[String], produced: &[String]) -> usize {
    let mut have: HashMap<&str, usize> = HashMap::new();
    for w in produced {
        *have.entry(w).or_default() += 1;
    }
    let mut found = 0;
    for w in reference {
        if let Some(n) = have.get_mut(w.as_str()).filter(|n| **n > 0) {
            *n -= 1;
            found += 1;
        }
    }
    found
}

/// Longitud de la subsecuencia común más larga.
fn lcs(a: &[String], b: &[String]) -> usize {
    let mut previous = vec![0usize; b.len() + 1];
    for x in a {
        let mut current = vec![0usize; b.len() + 1];
        for (j, y) in b.iter().enumerate() {
            current[j + 1] = if x == y {
                previous[j] + 1
            } else {
                previous[j + 1].max(current[j])
            };
        }
        previous = current;
    }
    previous[b.len()]
}

/// Pares de palabras consecutivas de la referencia que el bundle conserva juntos.
fn adjacent(reference: &[String], produced: &[String]) -> (usize, usize) {
    let mut kept: HashMap<(&str, &str), usize> = HashMap::new();
    for p in produced.windows(2) {
        *kept.entry((&p[0], &p[1])).or_default() += 1;
    }
    let mut found = 0;
    for p in reference.windows(2) {
        if let Some(n) = kept
            .get_mut(&(p[0].as_str(), p[1].as_str()))
            .filter(|n| **n > 0)
        {
            *n -= 1;
            found += 1;
        }
    }
    (found, reference.len().saturating_sub(1))
}

#[derive(Debug, Default, PartialEq)]
struct Scores {
    completeness: f64,
    order: f64,
    inner_order: f64,
    contiguity: f64,
}

/// Las cuatro métricas de un bundle frente a la referencia exacta de sus páginas.
fn measure(reference: &BTreeMap<usize, Words>, files: &BTreeMap<String, String>) -> Scores {
    let blocks = blocks(files);
    let mut pages: BTreeMap<usize, Words> = BTreeMap::new();
    for (first, last, found) in &blocks {
        for p in *first..=*last {
            pages.entry(p).or_default().extend(found.iter().cloned());
        }
    }
    let (mut words, mut found, mut ordered, mut pairs, mut kept) = (0, 0, 0, 0, 0);
    for (page, refs) in reference.iter().filter(|(_, r)| !r.is_empty()) {
        let produced = pages.get(page).map(Vec::as_slice).unwrap_or_default();
        let m = matched(refs, produced);
        words += refs.len();
        found += m;
        ordered += lcs(refs, produced).min(m);
        let (k, p) = adjacent(refs, produced);
        kept += k;
        pairs += p;
    }
    // Orden interno: cada bloque frente a la referencia de sus páginas, juntas, contado
    // en su primera página si esta tiene referencia.
    let (mut inner_found, mut inner_ordered) = (0, 0);
    for (first, last, found) in &blocks {
        if reference.get(first).is_none_or(Vec::is_empty) || found.is_empty() {
            continue;
        }
        let window: Words = (*first..=*last)
            .filter_map(|p| reference.get(&p))
            .flatten()
            .cloned()
            .collect();
        inner_found += matched(&window, found);
        inner_ordered += lcs(&window, found);
    }
    let ratio = |a: usize, b: usize| {
        if b == 0 {
            100.0
        } else {
            (10_000.0 * a as f64 / b as f64).round() / 100.0
        }
    };
    Scores {
        completeness: ratio(found, words),
        order: ratio(ordered, found),
        inner_order: ratio(inner_ordered, inner_found),
        contiguity: ratio(kept, pairs),
    }
}

// --- Estructura -------------------------------------------------------------------------

static HEADING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(#{1,6})\s+(.*)$").unwrap());
static LIST_ITEM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*([-*+]|\d+[.)])\s+(.*)$").unwrap());
static FENCE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^ {0,3}(`{3,}|~{3,})").unwrap());
static SEPARATOR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^:?-{3,}:?$").unwrap());

/// Texto comparable de un encabezado, una celda o un elemento: sus palabras.
fn norm(text: &str) -> String {
    words(&markdown_text(text)).join(" ")
}

/// Celdas de una fila de tabla GFM, sin las barras escapadas.
fn cells(line: &str) -> Vec<String> {
    let line = line.trim();
    let mut out = vec![String::new()];
    let mut escaped = false;
    for c in line.chars() {
        if c == '|' && !escaped {
            out.push(String::new());
        } else {
            out.last_mut().unwrap().push(c);
        }
        escaped = c == '\\' && !escaped;
    }
    // Sin lo que hay antes de la primera barra y después de la última.
    out.remove(0);
    out.pop();
    out.into_iter().map(|c| c.trim().to_string()).collect()
}

#[derive(Default)]
struct Structure {
    headings: Vec<(usize, String)>,
    items: Vec<(String, String)>,
    tables: Vec<Vec<Vec<String>>>,
    code: Vec<Vec<String>>,
}

/// Encabezados, elementos de lista, tablas y código de un cuerpo Markdown, en orden,
/// sin marcadores ni notas del conversor.
fn markdown_structure(body: &str, out: &mut Structure) {
    let body = MARKER
        .replace_all(&without_generated(body), "")
        .into_owned();
    let (mut in_table, mut code): (bool, Option<Vec<String>>) = (false, None);
    for line in body.split('\n') {
        if let Some(block) = code.as_mut() {
            if FENCE.is_match(line) {
                out.code.push(code.take().unwrap());
            } else {
                block.push(line.to_string());
            }
            continue;
        }
        if FENCE.is_match(line) {
            code = Some(Vec::new());
            continue;
        }
        if line.starts_with('|') {
            let row = cells(line);
            if !in_table {
                out.tables.push(Vec::new());
                in_table = true;
            }
            if !row.iter().all(|c| SEPARATOR.is_match(c)) {
                out.tables.last_mut().unwrap().push(row);
            }
            continue;
        }
        in_table = false;
        if let Some(m) = HEADING.captures(line) {
            out.headings.push((m[1].len(), m[2].to_string()));
        } else if let Some(m) = LIST_ITEM.captures(line) {
            out.items.push((m[1].to_string(), m[2].to_string()));
        }
    }
}

/// Estructura del bundle en el orden del documento: el título de `documento.md`
/// (nivel 0) y la de cada concepto. El `#` de un concepto que repite el título del
/// documento no es otro encabezado.
fn bundle_structure(files: &BTreeMap<String, String>) -> Structure {
    let document = &files["documento.md"];
    let body = &document[4..][document[4..].find("\n---\n").unwrap() + 5..];
    let title = body
        .split_once("<!-- okf-source-span: titulo -->")
        .and_then(|(_, rest)| HEADING.captures(rest.trim().lines().next().unwrap_or_default()))
        .map(|m| norm(&m[2]));
    let mut total = Structure::default();
    for (name, text) in files {
        if !name.starts_with("contenido/") || !name.ends_with(".md") || name.ends_with("index.md") {
            continue;
        }
        let body = &text[4..][text[4..].find("\n---\n").unwrap() + 5..];
        markdown_structure(body, &mut total);
    }
    total.headings = total
        .headings
        .into_iter()
        .map(|(level, t)| (level, norm(&t)))
        .filter(|(_, t)| Some(t) != title.as_ref())
        .collect();
    if let Some(title) = title {
        total.headings.insert(0, (0, title));
    }
    total
}

/// Parejas (i, j) de elementos de `reference` y `found` que coinciden, en orden.
fn in_order<A, B>(
    reference: &[A],
    found: &[B],
    same: impl Fn(&A, &B) -> bool,
) -> Vec<(usize, usize)> {
    let (mut pairs, mut start) = (Vec::new(), 0);
    for (i, r) in reference.iter().enumerate() {
        if let Some(j) = (start..found.len()).find(|&j| same(r, &found[j])) {
            pairs.push((i, j));
            start = j + 1;
        }
    }
    pairs
}

fn percent(a: usize, b: usize) -> f64 {
    (10_000.0 * a as f64 / b as f64).round() / 100.0
}

/// Métricas de estructura frente a los bloques de una referencia exacta; el mismo
/// cálculo que `tools/exactitud.py`. Solo las que la referencia permite calcular.
fn measure_structure(
    blocks: &[serde_json::Value],
    files: &BTreeMap<String, String>,
) -> BTreeMap<&'static str, f64> {
    let got = bundle_structure(files);
    let mut out = BTreeMap::new();
    let text = |v: &serde_json::Value| norm(v.as_str().unwrap());
    let kind = |b: &serde_json::Value| b["type"].as_str().unwrap().to_string();
    // Encabezados, en orden y con el mismo nivel relativo entre consecutivos.
    let refs: Vec<(i64, String)> = blocks
        .iter()
        .filter(|b| matches!(kind(b).as_str(), "title" | "heading"))
        .map(|b| {
            (
                if kind(b) == "title" {
                    0
                } else {
                    b["level"].as_i64().unwrap()
                },
                text(&b["text"]),
            )
        })
        .collect();
    if !refs.is_empty() {
        let pairs = in_order(&refs, &got.headings, |r, f| r.1 == f.1);
        let steps: Vec<_> = pairs.windows(2).collect();
        let same = steps
            .iter()
            .filter(|w| {
                let (a, b) = (w[0], w[1]);
                (refs[b.0].0 - refs[a.0].0).signum()
                    == (got.headings[b.1].0 as i64 - got.headings[a.1].0 as i64).signum()
            })
            .count();
        out.insert("headings", percent(pairs.len(), refs.len()));
        if !got.headings.is_empty() {
            out.insert(
                "heading_precision",
                percent(pairs.len(), got.headings.len()),
            );
        }
        out.insert(
            "heading_levels",
            if steps.is_empty() {
                100.0
            } else {
                percent(same, steps.len())
            },
        );
    }
    // Listas: cada elemento como elemento de lista y, si es ordenada, con su número.
    let items: Vec<(bool, String, String)> = blocks
        .iter()
        .filter(|b| kind(b) == "list")
        .flat_map(|b| {
            let ordered = b["ordered"].as_bool().unwrap();
            b["items"].as_array().unwrap().iter().map(move |i| {
                (
                    ordered,
                    i["label"].as_str().unwrap().to_string(),
                    norm(i["text"].as_str().unwrap()),
                )
            })
        })
        .collect();
    if !items.is_empty() {
        let found: Vec<(String, String)> = got
            .items
            .iter()
            .map(|(l, t)| (l.clone(), norm(t)))
            .collect();
        let pairs = in_order(&items, &found, |r, f| r.2 == f.1);
        let digits = |s: &str| s.chars().filter(char::is_ascii_digit).collect::<String>();
        let numbered: Vec<_> = pairs.iter().filter(|(i, _)| items[*i].0).collect();
        let right = numbered
            .iter()
            .filter(|(i, j)| digits(&items[*i].1) == digits(&found[*j].0))
            .count();
        out.insert("list_items", percent(pairs.len(), items.len()));
        out.insert(
            "list_numbers",
            if numbered.is_empty() {
                100.0
            } else {
                percent(right, numbered.len())
            },
        );
    }
    // Tablas: celdas en su fila y columna de la tabla del bundle con más palabras en
    // común; las del bundle que no corresponden a ninguna son falsas.
    type Table = Vec<Vec<String>>;
    let table_words = |t: &Table| {
        t.iter()
            .flatten()
            .flat_map(|c| c.split(' '))
            .filter(|w| !w.is_empty())
            .map(str::to_string)
            .collect::<Words>()
    };
    let ref_tables: Vec<Table> = blocks
        .iter()
        .filter(|b| kind(b) == "table")
        .map(|b| {
            b["rows"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r.as_array().unwrap().iter().map(text).collect())
                .collect()
        })
        .collect();
    let found_tables: Vec<Table> = got
        .tables
        .iter()
        .map(|t| {
            t.iter()
                .map(|r| r.iter().map(|c| norm(c)).collect())
                .collect()
        })
        .collect();
    let (mut used, mut all_cells, mut correct) = (std::collections::BTreeSet::new(), 0, 0);
    for reference in &ref_tables {
        all_cells += reference.iter().map(Vec::len).sum::<usize>();
        let wanted = table_words(reference);
        let mut best: Option<(usize, usize)> = None;
        for (j, t) in found_tables.iter().enumerate() {
            let m = matched(&wanted, &table_words(t));
            if best.is_none_or(|(_, b)| m > b) {
                best = Some((j, m));
            }
        }
        let Some((j, m)) = best.filter(|(_, m)| m * 2 >= wanted.len()) else {
            continue;
        };
        let _ = m;
        used.insert(j);
        let t = &found_tables[j];
        correct += reference
            .iter()
            .enumerate()
            .flat_map(|(r, row)| row.iter().enumerate().map(move |(c, cell)| (r, c, cell)))
            .filter(|(r, c, cell)| t.get(*r).and_then(|row| row.get(*c)) == Some(*cell))
            .count();
    }
    if !ref_tables.is_empty() {
        out.insert("table_cells", percent(correct, all_cells));
    }
    out.insert("false_tables", (found_tables.len() - used.len()) as f64);
    // Código: líneas de cada bloque, en orden, y su sangría relativa al bloque.
    let ref_code: Vec<Vec<String>> = blocks
        .iter()
        .filter(|b| kind(b) == "code")
        .map(|b| {
            b["lines"]
                .as_array()
                .unwrap()
                .iter()
                .map(|l| l.as_str().unwrap().to_string())
                .collect()
        })
        .collect();
    if !ref_code.is_empty() {
        let indent = |l: &str| l.len() - l.trim_start().len();
        let margin = |block: &[String]| {
            block
                .iter()
                .filter(|l| !l.trim().is_empty())
                .map(|l| indent(l))
                .min()
                .unwrap_or(0)
        };
        let stripped = |block: &[String]| {
            block
                .iter()
                .map(|l| l.trim().to_string())
                .collect::<Words>()
        };
        let (mut lines, mut kept, mut indented) = (0, 0, 0);
        for reference in &ref_code {
            lines += reference.iter().filter(|l| !l.trim().is_empty()).count();
            let mut best: Option<(&Vec<String>, usize)> = None;
            for c in &got.code {
                let m = matched(&stripped(reference), &stripped(c));
                if best.is_none_or(|(_, b)| m > b) {
                    best = Some((c, m));
                }
            }
            let empty = Vec::new();
            let found = best.map_or(&empty, |(c, _)| c);
            let pairs = in_order(&stripped(reference), &stripped(found), |r, f| {
                r == f && !r.is_empty()
            });
            kept += pairs.len();
            let (rm, fm) = (margin(reference), margin(found));
            indented += pairs
                .iter()
                .filter(|(i, j)| indent(&reference[*i]) - rm == indent(&found[*j]) - fm)
                .count();
        }
        out.insert("code_lines", percent(kept, lines));
        if kept > 0 {
            out.insert("code_indent", percent(indented, kept));
        }
    }
    out
}

/// Referencias exactas: el documento de cada una, sus páginas en palabras y sus bloques.
fn references() -> Vec<(PathBuf, BTreeMap<usize, Words>, Vec<serde_json::Value>)> {
    let mut out = Vec::new();
    let root = Path::new("examples/pdf/exactitud");
    for group in fs::read_dir(root).unwrap().flatten() {
        for file in fs::read_dir(group.path()).unwrap().flatten() {
            let data: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(file.path()).unwrap()).unwrap();
            let pages = data["pages"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.parse().unwrap(), words(v.as_str().unwrap())))
                .collect();
            let blocks = data["blocks"].as_array().cloned().unwrap_or_default();
            out.push((
                PathBuf::from(data["document"].as_str().unwrap()),
                pages,
                blocks,
            ));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[derive(serde::Deserialize)]
struct Bound {
    measured: f64,
    minimum: f64,
}

#[derive(serde::Deserialize)]
struct Threshold {
    /// La muestra es un escaneo: se convierte con el OCR, si están los modelos.
    #[serde(default)]
    ocr: bool,
    completeness: Bound,
    order: Bound,
    inner_order: Bound,
    contiguity: Bound,
    /// Métricas de estructura: mínimo de cada una y, para `false_tables`, máximo.
    #[serde(default)]
    structure: BTreeMap<String, StructureBound>,
}

#[derive(serde::Deserialize)]
struct StructureBound {
    measured: f64,
    #[serde(default)]
    minimum: Option<f64>,
    #[serde(default)]
    maximum: Option<f64>,
}

#[test]
fn exactness_does_not_regress() {
    let thresholds: BTreeMap<String, Threshold> =
        serde_json::from_str(include_str!("../examples/exactitud-thresholds.json")).unwrap();
    let references = references();
    assert_eq!(
        thresholds.len(),
        references.len(),
        "sobran o faltan umbrales"
    );
    let ocr = crate::pdf::ocr::test_config();
    let mut failures = Vec::new();
    for (doc, reference, blocks) in references {
        let key = doc.to_str().unwrap();
        let t = thresholds
            .get(key)
            .unwrap_or_else(|| panic!("falta umbral: {key}"));
        let files = match (&ocr, t.ocr) {
            (_, false) => crate::golden::bundle_files(&doc),
            (Some(cfg), true) => crate::golden::bundle_files_with(&doc, cfg),
            (None, true) => continue,
        };
        let s = measure(&reference, &files);
        for (name, value, bound) in [
            ("completitud", s.completeness, &t.completeness),
            ("orden", s.order, &t.order),
            ("orden interno", s.inner_order, &t.inner_order),
            ("contigüidad", s.contiguity, &t.contiguity),
        ] {
            assert!((bound.minimum - (bound.measured - 0.5).min(100.0)).abs() < 0.001);
            if value + 1e-9 < bound.minimum {
                failures.push(format!(
                    "{key}: {name} {value:.2} % < {:.2} % (medido {:.2} %)",
                    bound.minimum, bound.measured
                ));
            }
        }
        // Estructura: las mismas métricas que la referencia permite calcular.
        let structure = measure_structure(&blocks, &files);
        let measured: Vec<&str> = structure.keys().copied().collect();
        let declared: Vec<&str> = t.structure.keys().map(String::as_str).collect();
        assert_eq!(
            measured, declared,
            "{key}: métricas de estructura sin umbral o sobrantes"
        );
        for (name, value) in structure {
            let bound = &t.structure[name];
            match (bound.minimum, bound.maximum) {
                (Some(min), None) => {
                    assert!((min - (bound.measured - 0.5).clamp(0.0, 100.0)).abs() < 0.001);
                    if value + 1e-9 < min {
                        failures.push(format!(
                            "{key}: {name} {value:.2} % < {min:.2} % (medido {:.2} %)",
                            bound.measured
                        ));
                    }
                }
                (None, Some(max)) if value > max => failures.push(format!(
                    "{key}: {name} {value} > {max} (medido {})",
                    bound.measured
                )),
                (None, Some(_)) => {}
                _ => panic!("{key}: {name} necesita un mínimo o un máximo"),
            }
        }
    }
    assert!(
        failures.is_empty(),
        "Pérdida de exactitud frente a las referencias exactas (tools/exactitud.py da el detalle):\n{}",
        failures.join("\n")
    );
}

#[test]
fn exactness_detects_missing_reordered_and_interleaved_text() {
    let md = |body: &str, spans: &str| {
        BTreeMap::from([
            (
                "contenido/texto.md".to_string(),
                format!(
                    "---\ntype: \"Source Text\"\nsource_spans:\n  - id: \"seccion\"\n    kind: \"pdf_pages\"\n    from: 1\n    to: 1\nblock_spans: \"/contenido/texto.spans.json\"\n---\n\n# Título\n\n{body}"
                ),
            ),
            (
                "contenido/texto.spans.json".to_string(),
                format!("{{\"concept\": \"contenido/texto.md\", \"source_spans\": [{spans}]}}"),
            ),
        ])
    };
    let span =
        |id: &str| format!("{{\"id\": \"{id}\", \"kind\": \"pdf_pages\", \"from\": 1, \"to\": 1}}");
    let spans = [span("u-1"), span("u-2")].join(", ");
    let reference = BTreeMap::from([(1, words("Título uno dos tres. Cuatro cinco seis."))]);
    let marked = |a: &str, b: &str| {
        format!("<!-- okf-source-span: u-1 -->\n{a}\n\n<!-- okf-source-span: u-2 -->\n{b}\n")
    };
    // Exacto, aunque el conversor añada notas y rótulos que no son del original.
    let exact = md(
        &marked(
            "uno dos tres.",
            "<!-- okf-generated -->\n> Nota del conversor.\n\n![Imagen de la página 1](../assets/a.png)\n\nCuatro cinco seis.",
        ),
        &spans,
    );
    let perfect = Scores {
        completeness: 100.0,
        order: 100.0,
        inner_order: 100.0,
        contiguity: 100.0,
    };
    assert_eq!(measure(&reference, &exact), perfect);
    // Falta texto: baja la completitud.
    let lost = measure(&reference, &md(&marked("uno dos tres.", "Cuatro."), &spans));
    assert!(lost.completeness < 80.0 && lost.order == 100.0, "{lost:?}");
    // Bloques en otro orden: baja el orden, no el orden interno.
    let swapped = measure(
        &reference,
        &md(&marked("Cuatro cinco seis.", "uno dos tres."), &spans),
    );
    assert!(
        swapped.order < 80.0 && swapped.inner_order == 100.0,
        "{swapped:?}"
    );
    // Dos líneas intercaladas en un bloque: bajan el orden interno y la contigüidad.
    let mixed = measure(
        &reference,
        &md(&marked("uno cuatro dos cinco tres seis.", ""), &span("u-1")),
    );
    assert!(
        mixed.inner_order < 80.0 && mixed.contiguity < 50.0,
        "{mixed:?}"
    );
}

#[test]
fn structure_detects_levels_items_cells_false_tables_and_indentation() {
    let blocks: Vec<serde_json::Value> = serde_json::from_str(
        r#"[{"type": "title", "text": "Informe"},
            {"type": "heading", "level": 1, "text": "Uno"},
            {"type": "heading", "level": 2, "text": "Uno punto uno"},
            {"type": "heading", "level": 1, "text": "Dos"},
            {"type": "list", "ordered": true, "items": [
                {"label": "1.", "text": "Primero"}, {"label": "2.", "text": "Segundo"}]},
            {"type": "table", "rows": [["A", "B"], ["1", "2"]]},
            {"type": "code", "lines": ["si:", "  hacer"]}]"#,
    )
    .unwrap();
    let bundle = |concept: &str| {
        BTreeMap::from([
            (
                "documento.md".to_string(),
                "---\ntype: \"Source Document\"\n---\n\n<!-- okf-source-span: titulo -->\n# Informe\n\n## Estructura\n\n- [Uno](contenido/seccion-001.md)\n".to_string(),
            ),
            (
                "contenido/seccion-001.md".to_string(),
                format!("---\ntype: \"Source Section\"\n---\n\n# Informe\n\n{concept}"),
            ),
        ])
    };
    let exact = "# Uno\n\n## Uno punto uno\n\n# Dos\n\n1. Primero\n2. Segundo\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n\n```\nsi:\n  hacer\n```\n";
    let s = measure_structure(&blocks, &bundle(exact));
    assert!(
        s.iter().all(|(k, v)| if *k == "false_tables" {
            *v == 0.0
        } else {
            *v == 100.0
        }),
        "{s:?}"
    );
    // El título repetido por el concepto no es otro encabezado ni resta precisión.
    assert_eq!(s["heading_precision"], 100.0);
    let changed = [
        (
            "heading_levels",
            exact.replace("## Uno punto uno", "# Uno punto uno"),
        ),
        ("list_items", exact.replace("2. Segundo", "Segundo")),
        ("list_numbers", exact.replace("2. Segundo", "3. Segundo")),
        ("table_cells", exact.replace("| 1 | 2 |", "| 2 | 1 |")),
        ("code_indent", exact.replace("  hacer", "hacer")),
        ("headings", exact.replace("# Dos", "Dos")),
    ];
    for (metric, md) in changed {
        let s = measure_structure(&blocks, &bundle(&md));
        assert!(s[metric] < 100.0, "{metric} no detecta el cambio: {s:?}");
    }
    let s = measure_structure(
        &blocks,
        &bundle(&format!("{exact}\n| X | Y |\n| --- | --- |\n| p | q |\n")),
    );
    assert_eq!(s["false_tables"], 1.0);
}
