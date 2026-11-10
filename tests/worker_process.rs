//! El worker real convierte en un proceso hijo vigilado: memoria y plazo.
mod support;
use sqlx::PgPool;
use std::{path::Path, process::Command};

const BIN: &str = env!("CARGO_BIN_EXE_okf-microservice");

/// Ejecuta el worker hasta vaciar la cola y devuelve su registro.
fn worker_once(root: &Path, env: &[(&str, &str)]) -> String {
    let out = Command::new(BIN)
        .args(["worker", "--once"])
        .env("OKF_DATA_DIR", root)
        .env("OKF_OCR_ENABLED", "false")
        .env("OKF_DATABASE_URL", support::database_url(root))
        .env("RUST_LOG", "info")
        .env("NO_COLOR", "1")
        .envs(env.iter().copied())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stderr).unwrap()
}

async fn enqueue(db: &PgPool, root: &Path, id: &str) {
    let original = format!("originals/{id}.pptx");
    std::fs::copy("examples/pptx/simple.pptx", root.join(&original)).unwrap();
    sqlx::query("INSERT INTO jobs(id,filename,original,sha256,status,created_at) VALUES($1,'simple.pptx',$2,'-','queued',0)")
        .bind(id).bind(original).execute(db).await.unwrap();
}

async fn job(db: &PgPool, id: &str) -> (String, Option<String>) {
    sqlx::query_as("SELECT status,error FROM jobs WHERE id=$1")
        .bind(id)
        .fetch_one(db)
        .await
        .unwrap()
}

#[tokio::test]
async fn child_process_is_converted_or_stopped_at_its_limits() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let db = support::pool(root).await;
    // La primera ejecución crea la base y termina con la cola vacía.
    worker_once(root, &[]);
    enqueue(&db, root, "ok").await;
    let log = worker_once(root, &[]);
    assert_eq!(job(&db, "ok").await, ("complete".into(), None));
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

    // La clave del documento llega al hijo como argumento, aunque empiece por `-`, y al
    // bundle; no aparece en el registro (plan 3, I2).
    let key = "-clave:sp/42";
    std::fs::copy("examples/pptx/simple.pptx", root.join("originals/key.pptx")).unwrap();
    sqlx::query("INSERT INTO jobs(id,filename,original,sha256,status,created_at,document_key) VALUES('key','simple.pptx','originals/key.pptx','-','queued',0,$1)")
        .bind(key).execute(&db).await.unwrap();
    let log = worker_once(root, &[]);
    assert_eq!(job(&db, "key").await, ("complete".into(), None));
    let mut z =
        zip::ZipArchive::new(std::fs::File::open(root.join("published/key/bundle.zip")).unwrap())
            .unwrap();
    let mut document = String::new();
    std::io::Read::read_to_string(&mut z.by_name("documento.md").unwrap(), &mut document).unwrap();
    assert!(
        document.contains(&format!("document_key: \"{key}\"")),
        "{document}"
    );
    assert!(!log.contains(key), "{log}");

    // Los permisos llegan al hijo en un archivo del intento, no en sus argumentos, y al
    // bundle sin cambios (plan 3, I7).
    let access = r#"{"grupos": ["finanzas"]}"#;
    std::fs::copy("examples/pptx/simple.pptx", root.join("originals/acc.pptx")).unwrap();
    sqlx::query("INSERT INTO jobs(id,filename,original,sha256,status,created_at,access) VALUES('acc','simple.pptx','originals/acc.pptx','-','queued',0,$1)")
        .bind(access).execute(&db).await.unwrap();
    let log = worker_once(root, &[]);
    assert_eq!(job(&db, "acc").await, ("complete".into(), None));
    let mut z =
        zip::ZipArchive::new(std::fs::File::open(root.join("published/acc/bundle.zip")).unwrap())
            .unwrap();
    let mut document = String::new();
    std::io::Read::read_to_string(&mut z.by_name("documento.md").unwrap(), &mut document).unwrap();
    assert!(
        document.contains(r#"access: "{\"grupos\": [\"finanzas\"]}""#),
        "{document}"
    );
    assert!(!log.contains("finanzas"), "{log}");

    // Los cuatro formatos deben poder leer sus recursos con el aislamiento activo.
    for (format, source) in [
        ("docx", "examples/docx/breve.docx"),
        ("xlsx", "examples/xlsx/muestras/ventas_trimestrales.xlsx"),
        ("pdf", "examples/pdf/muestras/breve.pdf"),
    ] {
        let original = format!("originals/{format}.{format}");
        std::fs::copy(source, root.join(&original)).unwrap();
        sqlx::query("INSERT INTO jobs(id,filename,original,sha256,status,created_at) VALUES($1,$2,$3,'-','queued',0)")
            .bind(format).bind(format!("sample.{format}")).bind(original).execute(&db).await.unwrap();
        worker_once(root, &[]);
        assert_eq!(job(&db, format).await, ("complete".into(), None));
    }

    enqueue(&db, root, "memory").await;
    let log = worker_once(root, &[("OKF_MAX_MEMORY_MB", "1")]);
    assert!(log.contains("status=\"failed\""), "{log}");
    let (state, error) = job(&db, "memory").await;
    assert_eq!(state, "failed");
    assert!(error.unwrap().contains("límite de memoria de 1 MB"));

    enqueue(&db, root, "slow").await;
    worker_once(root, &[("OKF_CONVERSION_TIMEOUT_S", "0")]);
    let (state, error) = job(&db, "slow").await;
    assert_eq!(state, "failed");
    assert!(error.unwrap().contains("tiempo máximo de 0 s"));
    assert!(!root.join("published/memory").exists() && !root.join("published/slow").exists());
}

#[tokio::test]
async fn pdf_oxide_fuzz_panic_is_permanent_and_does_not_stop_the_worker() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let db = support::pool(root).await;
    worker_once(root, &[]);
    std::fs::write(
        root.join("originals/fuzz.pdf"),
        include_bytes!("../fuzz/regressions/pdf/utf8-object-header.pdf"),
    )
    .unwrap();
    sqlx::query("INSERT INTO jobs(id,filename,original,sha256,status,created_at) VALUES('fuzz','fuzz.pdf','originals/fuzz.pdf','-','queued',0)")
        .execute(&db).await.unwrap();
    worker_once(root, &[]);
    let (state, error) = job(&db, "fuzz").await;
    assert_eq!(state, "failed");
    let error = error.unwrap();
    assert!(error.contains("defecto del conversor (pánico)"), "{error}");
    assert!(error.contains("char boundary"), "{error}");
    let attempt: i64 = sqlx::query_scalar("SELECT attempt FROM jobs WHERE id='fuzz'")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(attempt, 1);
    assert!(!root.join("published/fuzz").exists());
    // La contención deja al worker disponible para el siguiente documento válido.
    enqueue(&db, root, "after_fuzz").await;
    worker_once(root, &[]);
    assert_eq!(job(&db, "after_fuzz").await, ("complete".into(), None));
}
