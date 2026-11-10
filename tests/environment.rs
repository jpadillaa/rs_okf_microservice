//! Configuración del binario desde archivo, sin modificar el entorno global de las pruebas.
mod support;
use std::{fs, process::Command};
const BIN: &str = env!("CARGO_BIN_EXE_okf-microservice");

#[tokio::test]
async fn environment_file_configures_postgres_and_process_environment_takes_precedence() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("datos con espacios");
    fs::create_dir(&root).unwrap();
    let db = support::pool(&root).await;
    let config = tmp.path().join("conexion.env");
    fs::write(
        &config,
        format!(
            "OKF_DATABASE_URL='{}'\nOKF_DATA_DIR='{}'\n",
            support::database_url(&root),
            root.display()
        ),
    )
    .unwrap();
    let run = |url: Option<&str>| {
        let mut command = Command::new(BIN);
        command
            .args(["--env-file", config.to_str().unwrap(), "worker", "--once"])
            .env_remove("OKF_DATABASE_URL")
            .env_remove("OKF_DATA_DIR")
            // El OCR está activo por defecto y estas pruebas no tienen sus modelos.
            .env("OKF_OCR_ENABLED", "false");
        if let Some(url) = url {
            command.env("OKF_DATABASE_URL", url);
        }
        command.output().unwrap()
    };
    let output = run(None);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(root.join("originals").exists());
    fs::write(
        &config,
        format!(
            "OKF_DATABASE_URL=invalid\nOKF_DATA_DIR='{}'\n",
            root.display()
        ),
    )
    .unwrap();
    assert!(!run(None).status.success());
    assert!(run(Some(&support::database_url(&root))).status.success());
    fs::remove_file(&config).unwrap();
    assert!(!run(Some(&support::database_url(&root))).status.success());
    db.close().await;
}

#[tokio::test]
async fn worker_refuses_to_start_with_ocr_enabled_and_no_models() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("datos");
    fs::create_dir(&root).unwrap();
    let db = support::pool(&root).await;
    let run = |enabled: &str| {
        Command::new(BIN)
            .args(["worker", "--once"])
            .env("OKF_DATABASE_URL", support::database_url(&root))
            .env("OKF_DATA_DIR", &root)
            .env("OKF_OCR_ENABLED", enabled)
            .env("OKF_OCR_MODELS_DIR", tmp.path().join("sin-modelos"))
            .output()
            .unwrap()
    };
    let output = run("true");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(
        stderr.contains("det.onnx") && stderr.contains("OKF_OCR_MODELS_DIR"),
        "{stderr}"
    );
    let output = run("quizá");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("OKF_OCR_ENABLED debe ser true o false")
    );
    assert!(run("false").status.success());
    db.close().await;
}

#[test]
fn conversion_threads_and_cpu_limit_are_validated() {
    let run = |name: &str, value: &str| {
        Command::new(BIN)
            .args(["validate", "inexistente.zip"])
            .env(name, value)
            .output()
            .unwrap()
    };
    for (name, value, message) in [
        (
            "OKF_CONVERSION_THREADS",
            "0",
            "OKF_CONVERSION_THREADS debe estar entre 1 y 64",
        ),
        (
            "OKF_CONVERSION_THREADS",
            "65",
            "OKF_CONVERSION_THREADS debe estar entre 1 y 64",
        ),
        (
            "OKF_CONVERSION_THREADS",
            "dos",
            "OKF_CONVERSION_THREADS no es un número válido",
        ),
        ("OKF_MAX_CPU_S", "0", "OKF_MAX_CPU_S debe ser mayor que 0"),
    ] {
        let output = run(name, value);
        assert!(!output.status.success(), "{name}={value}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(message), "{name}={value}: {stderr}");
    }
}
