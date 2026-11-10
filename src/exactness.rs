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

/// Referencias exactas: el documento de cada una y sus páginas en palabras.
fn references() -> Vec<(PathBuf, BTreeMap<usize, Words>)> {
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
            out.push((PathBuf::from(data["document"].as_str().unwrap()), pages));
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
    for (doc, reference) in references {
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
