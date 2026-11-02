//! El worker real convierte en un proceso hijo vigilado: memoria y plazo.
use rusqlite::{Connection, params};
use std::{path::Path, process::Command};

const BIN: &str = env!("CARGO_BIN_EXE_okf-microservice");

/// Ejecuta el worker hasta vaciar la cola y devuelve su registro.
fn worker_once(root: &Path, env: &[(&str, &str)]) -> String {
    let out = Command::new(BIN)
        .args(["worker", "--once"])
        .env("OKF_DATA_DIR", root)
        .env("RUST_LOG", "info")
        .env("NO_COLOR", "1")
        .envs(env.iter().copied())
        .output()
        .unwrap();
    assert!(out.status.success());
    String::from_utf8(out.stderr).unwrap()
}

fn enqueue(root: &Path, id: &str) {
    let original = format!("originals/{id}.pptx");
    std::fs::copy("examples/pptx/simple.pptx", root.join(&original)).unwrap();
    Connection::open(root.join("jobs.sqlite3"))
        .unwrap()
        .execute(
            "INSERT INTO jobs(id,filename,original,sha256,status,created_at) VALUES(?1,'simple.pptx',?2,'-','queued',0)",
            params![id, original],
        )
        .unwrap();
}

fn job(root: &Path, id: &str) -> (String, Option<String>) {
    Connection::open(root.join("jobs.sqlite3"))
        .unwrap()
        .query_row("SELECT status,error FROM jobs WHERE id=?1", [id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap()
}

#[test]
fn child_process_is_converted_or_stopped_at_its_limits() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    // La primera ejecución crea la base y termina con la cola vacía.
    worker_once(root, &[]);
    enqueue(root, "ok");
    let log = worker_once(root, &[]);
    assert_eq!(job(root, "ok"), ("complete".into(), None));
    assert!(root.join("published/ok/bundle.zip").exists());
    // Una línea por intento con formato, estado, etapas, duración total y memoria
    // máxima, sin texto del documento.
    let line = log
        .lines()
        .find(|l| l.contains("intento terminado"))
        .unwrap_or_else(|| panic!("sin línea del intento en:\n{log}"));
    for field in [
        "job_id=ok",
        "attempt=1",
        "format=\"pptx\"",
        "status=\"complete\"",
        "read_ms=",
        "assemble_ms=",
        "validate_ms=",
        "total_ms=",
        "peak_mb=",
    ] {
        assert!(line.contains(field), "{field} en {line}");
    }
    // El máximo del hijo cubre también conversiones más cortas que el muestreo.
    assert!(!line.ends_with("peak_mb=0"), "{line}");
    assert!(!log.contains("A simple OKF test deck"));

    // Los cuatro formatos deben poder leer sus recursos con el aislamiento activo.
    for (format, source) in [
        ("docx", "examples/docx/breve.docx"),
        ("xlsx", "examples/xlsx/muestras/ventas_trimestrales.xlsx"),
        ("pdf", "examples/pdf/muestras/breve.pdf"),
    ] {
        let original = format!("originals/{format}.{format}");
        std::fs::copy(source, root.join(&original)).unwrap();
        Connection::open(root.join("jobs.sqlite3")).unwrap().execute(
            "INSERT INTO jobs(id,filename,original,sha256,status,created_at) VALUES(?1,?2,?3,'-','queued',0)",
            params![format, format!("sample.{format}"), original],
        ).unwrap();
        worker_once(root, &[]);
        assert_eq!(job(root, format), ("complete".into(), None));
    }

    enqueue(root, "memory");
    let log = worker_once(root, &[("OKF_MAX_MEMORY_MB", "1")]);
    assert!(log.contains("status=\"failed\""), "{log}");
    let (state, error) = job(root, "memory");
    assert_eq!(state, "failed");
    assert!(error.unwrap().contains("límite de memoria de 1 MB"));

    enqueue(root, "slow");
    worker_once(root, &[("OKF_CONVERSION_TIMEOUT_S", "0")]);
    let (state, error) = job(root, "slow");
    assert_eq!(state, "failed");
    assert!(error.unwrap().contains("tiempo máximo de 0 s"));
    assert!(!root.join("published/memory").exists() && !root.join("published/slow").exists());
}
