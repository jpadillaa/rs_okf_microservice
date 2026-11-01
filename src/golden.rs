//! Bundles de referencia (plan 2, C1). Cada documento de `examples/` se convierte y su
//! bundle se compara archivo por archivo con `examples/<formato>/esperado/<grupo>/<nombre>/`:
//! los textos completos y, en `recursos.tsv`, la huella de cada recurso binario. Las vistas
//! renderizadas solo se enumeran, porque sus píxeles dependen de las fuentes instaladas.
//!
//! La fecha de generación y las duraciones cambian en cada conversión y se sustituyen por
//! marcas fijas antes de comparar. Tras un cambio intencionado, `OKF_UPDATE_GOLDEN=1 cargo
//! test golden` regenera las referencias; su diferencia se revisa en el commit.
use crate::bundle::hex;
use crate::format::Format;
use crate::worker::Config;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
};
use zip::ZipArchive;

/// Documentos de ejemplo de todos los formatos, sin las carpetas de resultados.
fn documents() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            let name = e.file_name();
            if p.is_dir() {
                if name != "esperado" && name != "cobertura" {
                    walk(&p, out);
                }
            } else if Format::from_filename(&name.to_string_lossy()).is_some() {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    for f in Format::ALL {
        walk(&Path::new("examples").join(f.extension()), &mut out);
    }
    out.sort();
    out
}

/// Carpeta de la referencia: `corpus/s1.pptx` → `esperado/corpus/s1`; los documentos en
/// la raíz del formato forman el grupo `fixtures`.
fn expected_dir(doc: &Path) -> PathBuf {
    let rel = doc.strip_prefix("examples").unwrap();
    let mut parts: Vec<_> = rel.iter().collect();
    let format = parts.remove(0);
    let name = Path::new(parts.pop().unwrap())
        .file_stem()
        .unwrap()
        .to_owned();
    let group = parts.first().map_or("fixtures".as_ref(), |g| *g);
    Path::new("examples")
        .join(format)
        .join("esperado")
        .join(group)
        .join(name)
}

/// ¿Es la vista renderizada de una diapositiva o de una página?
fn is_view(name: &str) -> bool {
    let Some(stem) = name
        .strip_prefix("assets/")
        .and_then(|n| n.strip_suffix(".png"))
    else {
        return false;
    };
    ["slide-", "pagina-"].iter().any(|p| {
        stem.strip_prefix(p)
            .is_some_and(|n| n.len() == 3 && n.bytes().all(|b| b.is_ascii_digit()))
    })
}

/// Sustituye los valores que cambian en cada conversión.
fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches('\n');
        let newline = &line[body.len()..];
        if let Some(indent) = body
            .strip_suffix('"')
            .and_then(|b| b.find("at: \"").map(|i| &body[..i]))
            .filter(|i| i.trim().is_empty())
        {
            out.push_str(&format!("{indent}at: \"<generado>\"{newline}"));
        } else if body.starts_with("- Duración") && body.ends_with(" ms") {
            let label = body.split(':').next().unwrap();
            out.push_str(&format!("{label}: <duración> ms{newline}"));
        } else {
            out.push_str(line);
        }
    }
    out
}

/// Archivos de la referencia de un bundle: textos normalizados y `recursos.tsv`.
fn snapshot(doc: &Path) -> BTreeMap<String, String> {
    let bytes = fs::read(doc).unwrap();
    let filename = doc.file_name().unwrap().to_string_lossy().into_owned();
    let format = Format::from_filename(&filename).unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let hash = hex(&Sha256::digest(&bytes));
    let built = format
        .convert(&Config::default(), &bytes, &filename, &hash, tmp.path())
        .unwrap_or_else(|e| panic!("{}: {e:#}", doc.display()));
    let mut z = ZipArchive::new(fs::File::open(built.path).unwrap()).unwrap();
    let mut files = BTreeMap::new();
    let mut resources = Vec::new();
    for i in 0..z.len() {
        let mut e = z.by_index(i).unwrap();
        let name = e.name().to_string();
        let mut data = Vec::new();
        e.read_to_end(&mut data).unwrap();
        if [".md", ".csv", ".json", ".jsonl"]
            .iter()
            .any(|x| name.ends_with(x))
        {
            files.insert(name, normalize(&String::from_utf8(data).unwrap()));
        } else if is_view(&name) {
            resources.push(format!("{name}\tvista"));
        } else {
            resources.push(format!("{name}\t{}", hex(&Sha256::digest(&data))));
        }
    }
    resources.sort();
    files.insert("recursos.tsv".into(), resources.join("\n") + "\n");
    files
}

fn read_expected(dir: &Path) -> BTreeMap<String, String> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
        for e in fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(root, &p, out);
            } else {
                let rel = p
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(rel, fs::read_to_string(&p).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

fn write_expected(dir: &Path, files: &BTreeMap<String, String>) {
    let _ = fs::remove_dir_all(dir);
    for (name, text) in files {
        let path = dir.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
}

/// Primera diferencia legible entre dos textos, con dos líneas de contexto.
fn first_difference(expected: &str, actual: &str) -> String {
    let (e, a): (Vec<_>, Vec<_>) = (expected.lines().collect(), actual.lines().collect());
    let i = e.iter().zip(&a).take_while(|(x, y)| x == y).count();
    let from = i.saturating_sub(2);
    let show = |v: &[&str], sign: char| {
        v.iter()
            .skip(from)
            .take(i + 3 - from)
            .enumerate()
            .map(|(k, l)| {
                let mark = if from + k == i { sign } else { ' ' };
                format!("{mark} {:>5} | {l}", from + k + 1)
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!("esperado:\n{}\nobtenido:\n{}", show(&e, '-'), show(&a, '+'))
}

#[test]
fn golden_bundles_match_their_reference() {
    let update = std::env::var_os("OKF_UPDATE_GOLDEN").is_some();
    let docs = documents();
    assert!(!docs.is_empty());
    let mut failures = Vec::new();
    for doc in &docs {
        let dir = expected_dir(doc);
        let actual = snapshot(doc);
        if update {
            write_expected(&dir, &actual);
            continue;
        }
        let expected = read_expected(&dir);
        if expected.is_empty() {
            failures.push(format!(
                "{}: sin referencia en {}",
                doc.display(),
                dir.display()
            ));
            continue;
        }
        for name in expected
            .keys()
            .chain(actual.keys())
            .collect::<std::collections::BTreeSet<_>>()
        {
            match (expected.get(name), actual.get(name)) {
                (Some(e), Some(a)) if e == a => {}
                (Some(e), Some(a)) => {
                    failures.push(format!(
                        "{}: {name} cambió\n{}",
                        doc.display(),
                        first_difference(e, a)
                    ));
                    break;
                }
                (Some(_), None) => failures.push(format!("{}: falta {name}", doc.display())),
                (None, _) => failures.push(format!("{}: sobra {name}", doc.display())),
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} diferencia(s) con los bundles de referencia (OKF_UPDATE_GOLDEN=1 los regenera):\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

#[test]
fn conversion_is_deterministic() {
    for doc in documents() {
        assert!(
            snapshot(&doc) == snapshot(&doc),
            "{}: dos conversiones dan bundles distintos",
            doc.display()
        );
    }
}
