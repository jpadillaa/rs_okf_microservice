//! Esquema del frontmatter del perfil (plan 3, I9). `docs/especificacion/` guarda un
//! JSON Schema por versión del perfil; estas pruebas exigen que exista el de la versión
//! actual y que el frontmatter de los bundles de referencia lo cumpla, de modo que un
//! cambio de perfil que añade, quita o cambia un campo actualiza su esquema en el mismo
//! commit.
use crate::bundle::frontmatter;
use serde_json::{Value, json};
use std::fs;

/// Esquema de la versión actual del perfil.
fn schema() -> Value {
    let path = format!(
        "docs/especificacion/perfil-{}.schema.json",
        crate::version::PROFILE
    );
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("falta el esquema del perfil actual en {path}: {e}"));
    serde_json::from_str(&text).unwrap()
}

/// Validador del frontmatter.
fn validator() -> jsonschema::Validator {
    jsonschema::validator_for(&schema()).expect("esquema válido")
}

/// Validador de los archivos de localizadores de bloque (`<concepto>.spans.json`).
fn block_spans_validator() -> jsonschema::Validator {
    let mut schema = schema();
    schema["oneOf"] = json!([{"$ref": "#/$defs/blockSpansFile"}]);
    jsonschema::validator_for(&schema).expect("esquema válido")
}

/// Frontmatter de un archivo Markdown, como JSON.
fn as_json(md: &str) -> Value {
    serde_json::to_value(frontmatter(md).unwrap()).unwrap()
}

fn errors(v: &jsonschema::Validator, fm: &Value) -> Vec<String> {
    v.iter_errors(fm)
        .map(|e| format!("{} en {}", e, e.instance_path()))
        .collect()
}

#[test]
fn the_frontmatter_of_every_reference_bundle_follows_the_schema() {
    let v = validator();
    let spans = block_spans_validator();
    let mut checked = 0;
    let mut failures = Vec::new();
    for doc in crate::golden::documents() {
        for (name, text) in crate::golden::bundle_files(&doc) {
            let base = name.rsplit('/').next().unwrap();
            if name.ends_with(".spans.json") {
                let file: Value = serde_json::from_str(&text).unwrap();
                for e in errors(&spans, &file) {
                    failures.push(format!("{}: {name}: {e}", doc.display()));
                }
                continue;
            }
            if !name.ends_with(".md") || base == "index.md" {
                continue;
            }
            checked += 1;
            for e in errors(&v, &as_json(&text)) {
                failures.push(format!("{}: {name}: {e}", doc.display()));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(checked > 200, "solo {checked} archivos con frontmatter");
}

#[test]
fn the_schema_accepts_optional_fields_and_rejects_changes() {
    let v = validator();
    let docs = crate::golden::documents();
    let pdf = docs.iter().find(|d| d.ends_with("breve.pdf")).unwrap();
    let files = crate::golden::bundle_files(pdf);
    let document = as_json(&files["documento.md"]);
    assert!(errors(&v, &document).is_empty());
    // Campos que ningún ejemplo activa: OCR, conversión no reproducible, clave y permisos.
    let mut full = document.clone();
    full["conversion_settings"]["ocr"] = json!({
        "enabled": true, "max_pages": 40, "timeout_s": 300,
        "det_sha256": "a".repeat(64), "rec_sha256": "ausente", "dict_sha256": "b".repeat(64)
    });
    full["reproducible"] = json!(false);
    full["document_key"] = json!("sp:sitios/42");
    full["access"] = json!(r#"{"grupos": ["finanzas"]}"#);
    assert_eq!(errors(&v, &full), Vec::<String>::new());
    // Un campo desconocido, otra versión del perfil, un campo que cambia de tipo o un
    // archivo de localizadores fuera de contenido/ no cumplen el esquema.
    let concept = as_json(&files["contenido/texto.md"]);
    assert_eq!(concept["block_spans"], json!("/contenido/texto.spans.json"));
    type Change<'a> = (&'a Value, &'a dyn Fn(&mut Value));
    let changes: [Change; 5] = [
        (&document, &|f| f["nuevo_campo"] = json!(1)),
        (&document, &|f| f["profile_version"] = json!("1.9.0")),
        (&document, &|f| f["conversion_id"] = json!(12)),
        (&document, &|f| f["reproducible"] = json!(true)),
        (&concept, &|f| {
            f["block_spans"] = json!("/assets/texto.spans.json")
        }),
    ];
    for (k, (base, change)) in changes.iter().enumerate() {
        let mut altered = (*base).clone();
        change(&mut altered);
        assert!(!errors(&v, &altered).is_empty(), "alteración {k} aceptada");
    }
    // En el archivo de localizadores, una caja con coordenadas negativas o un campo
    // desconocido no cumplen el esquema.
    let spans = block_spans_validator();
    let file: Value = serde_json::from_str(&files["contenido/texto.spans.json"]).unwrap();
    assert!(errors(&spans, &file).is_empty());
    for change in [
        &(|f: &mut Value| f["source_spans"][0]["boxes"][0]["x0"] = json!(-3.5))
            as &dyn Fn(&mut Value),
        &|f: &mut Value| f["extra"] = json!(1),
    ] {
        let mut altered = file.clone();
        change(&mut altered);
        assert!(!errors(&spans, &altered).is_empty());
    }
}
