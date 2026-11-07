//! Control independiente de C1: regenerar los golden no modifica estos umbrales.
//! Mismo cálculo de palabras que tools/coverage.py (sin la métrica de reagrupación).
use regex::Regex;
use std::{collections::BTreeMap, fs, path::Path, sync::LazyLock};
use unicode_normalization::UnicodeNormalization;

type Counts = BTreeMap<String, usize>;
static WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\p{L}\p{N}]+").unwrap());
static FIELDS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?m)^\s*(?:kicker|section|display_label): "(.*)"$"#).unwrap());
static COMMENTS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<!--.*?-->").unwrap());
static TARGETS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\]\([^)]*\)").unwrap());

fn words(text: &str) -> Vec<String> {
    let normalized = text.nfkc().filter(|&c| c != '\u{ad}').collect::<String>();
    let folded = caseless::default_case_fold_str(&normalized);
    WORD.find_iter(&folded)
        .map(|m| m.as_str().to_owned())
        .collect()
}

fn markdown_text(md: &str) -> String {
    let mut text = String::new();
    let mut body = md;
    if md.starts_with("---\n")
        && let Some(end) = md[4..].find("\n---\n").map(|i| i + 4)
    {
        for c in FIELDS.captures_iter(&md[..end]) {
            text.push_str(&c[1]);
            text.push('\n');
        }
        body = &md[end + 5..];
    }
    let body = COMMENTS.replace_all(body, " ");
    text.push_str(&TARGETS.replace_all(&body, "] "));
    text
}

fn bundle_words(files: &BTreeMap<String, String>) -> Counts {
    let mut counts = Counts::new();
    for (name, data) in files {
        let base = name.rsplit('/').next().unwrap();
        if matches!(base, "index.md" | "conversion-report.md") {
            continue;
        }
        let text = if name.ends_with(".md") {
            markdown_text(data)
        } else if name.ends_with(".csv") {
            data.clone()
        } else {
            continue;
        };
        for word in words(&text) {
            *counts.entry(word).or_default() += 1;
        }
    }
    counts
}

fn reference(doc: &Path) -> Counts {
    let dir = crate::golden::expected_dir(doc);
    let path = Path::new(&dir.to_string_lossy().replace("/esperado/", "/cobertura/"))
        .with_extension("tsv");
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut counts = Counts::new();
    for line in text.lines() {
        let (word, n) = line.split_once('\t').expect("referencia TSV inválida");
        let n: usize = n.parse().expect("frecuencia inválida");
        assert!(n > 0 && words(word) == [word], "palabra inválida: {line}");
        assert!(
            counts.insert(word.to_owned(), n).is_none(),
            "palabra repetida: {word}"
        );
    }
    counts
}

fn measure(reference: &Counts, produced: &Counts) -> (f64, Vec<String>) {
    let total: usize = reference.values().sum();
    if total == 0 {
        return (100.0, Vec::new());
    }
    let mut missing = 0;
    let mut details = Vec::new();
    for (word, n) in reference {
        let lost = n.saturating_sub(*produced.get(word).unwrap_or(&0));
        if lost > 0 {
            missing += lost;
            details.push(format!("{word}×{lost}"));
        }
    }
    (100.0 * (1.0 - missing as f64 / total as f64), details)
}

#[derive(serde::Deserialize)]
struct Threshold {
    measured: f64,
    minimum: f64,
    /// Documento sin capa de texto (un escaneo): su referencia está vacía a propósito.
    /// Cualquier otra lista vacía es un error.
    #[serde(default)]
    empty_reference: bool,
}

#[test]
fn coverage_does_not_regress() {
    let thresholds: BTreeMap<String, Threshold> =
        serde_json::from_str(include_str!("../examples/coverage-thresholds.json")).unwrap();
    let docs = crate::golden::documents();
    assert_eq!(thresholds.len(), docs.len(), "sobran o faltan umbrales");
    let mut failures = Vec::new();
    for doc in docs {
        let key = doc.to_str().unwrap();
        let t = thresholds
            .get(key)
            .unwrap_or_else(|| panic!("falta umbral: {key}"));
        assert!((t.minimum - (t.measured - 0.5)).abs() < 0.001);
        let reference = reference(&doc);
        assert_eq!(
            reference.is_empty(),
            t.empty_reference,
            "{key}: referencia vacía sin declarar, o declarada vacía y con palabras"
        );
        let (value, missing) = measure(
            &reference,
            &bundle_words(&crate::golden::bundle_files(&doc)),
        );
        if value + 1e-9 < t.minimum {
            failures.push(format!(
                "{key}: {value:.2}% < {:.2}% (medido {:.2}%); faltan: {}",
                t.minimum,
                t.measured,
                missing.join(", ")
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "Pérdida de cobertura independiente de los golden:\n{}",
        failures.join("\n")
    );
}

#[test]
fn coverage_matches_python_and_detects_lost_text() {
    // Casos calculados con tools/coverage.py: casefold completo, NFKC, números,
    // letras combinantes, guion blando y campos que conservan texto original.
    assert_eq!(
        words("Straße Σς İ Ａ ﬁ ² Ⅷ café cafe\u{301} co\u{ad}sa a_b"),
        [
            "strasse", "σσ", "i", "a", "fi", "2", "viii", "café", "café", "cosa", "a", "b"
        ]
    );
    let md = "---\ntitle: \"excluir\"\nsection: \"Original\"\n---\nTexto <!-- oculto\nen comentario --> [visible](destino.pdf)";
    assert_eq!(words(&markdown_text(md)), ["original", "texto", "visible"]);
    let mut files = BTreeMap::from([
        ("contenido/a.md".into(), md.into()),
        ("documento.md".into(), "origen".into()),
        ("assets/a.csv".into(), "celda,10".into()),
        ("contenido/index.md".into(), "excluir".into()),
        ("conversion-report.md".into(), "excluir".into()),
        ("assets/a.jsonl".into(), "excluir".into()),
    ]);
    let reference = Counts::from_iter(
        ["original", "texto", "visible", "origen", "celda", "10"].map(|w| (w.into(), 1)),
    );
    assert_eq!(measure(&reference, &bundle_words(&files)).0, 100.0);
    files.insert("contenido/a.md".into(), md.replace("Texto ", ""));
    // Incluso si este árbol se acepta como nuevo golden, el control falla.
    assert!(measure(&reference, &bundle_words(&files)).0 < 99.5);
    let reference = Counts::from([("repetida".into(), 2)]);
    assert_eq!(
        measure(&reference, &Counts::from([("repetida".into(), 1)])).0,
        50.0
    );
}
