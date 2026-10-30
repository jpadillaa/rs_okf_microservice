//! El worker real convierte en un proceso hijo vigilado: memoria y plazo.
use rusqlite::{Connection, params};
use std::{path::Path, process::Command};

const BIN: &str = env!("CARGO_BIN_EXE_okf-microservice");

fn worker_once(root: &Path, env: &[(&str, &str)]) {
    let status = Command::new(BIN)
        .args(["worker", "--once"])
        .env("OKF_DATA_DIR", root)
        .envs(env.iter().copied())
        .status()
        .unwrap();
    assert!(status.success());
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
    worker_once(root, &[]);
    assert_eq!(job(root, "ok"), ("complete".into(), None));
    assert!(root.join("published/ok/bundle.zip").exists());

    enqueue(root, "memory");
    worker_once(root, &[("OKF_MAX_MEMORY_MB", "1")]);
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
