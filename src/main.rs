use anyhow::Result;
use axum::{
    Json, Router,
    extract::{Multipart, Path, State},
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use clap::{Parser, Subcommand};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path as FsPath, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;
use tracing::info;
use uuid::Uuid;

mod bundle;
mod docx;
mod format;
#[cfg(test)]
mod golden;
mod omml;
mod ooxml;
mod output;
mod pdf;
mod pptx;
mod sections;
mod worker;
mod xlsx;
mod xml;

#[derive(Parser)]
#[command(name = "okf-microservice")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Api {
        #[arg(long, default_value = "127.0.0.1:8080")]
        bind: String,
    },
    Worker {
        #[arg(long, default_value_t = 1000)]
        poll_ms: u64,
        /// Procesa los trabajos disponibles y termina cuando la cola queda vacía.
        #[arg(long)]
        once: bool,
    },
    /// Uso interno del worker: convierte un original en el directorio de un intento.
    #[command(hide = true)]
    Convert {
        original: PathBuf,
        attempt: PathBuf,
        filename: String,
    },
    /// Render every PPTX slide with the selected native Rust backend.
    BenchRender { pptx: PathBuf, output: PathBuf },
}

#[derive(Clone)]
struct App {
    root: PathBuf,
    db: Arc<Mutex<Connection>>,
    cfg: Arc<worker::Config>,
    /// Ejecutable para convertir en un proceso hijo vigilado; `None` convierte en el
    /// propio proceso (pruebas unitarias).
    converter: Option<PathBuf>,
}
#[derive(Deserialize, Serialize)]
struct Accepted {
    job_id: String,
    status: String,
}
#[derive(Serialize)]
struct JobStatus {
    job_id: String,
    status: String,
    attempt: u32,
    error: Option<String>,
}

fn app(root: PathBuf) -> Result<App> {
    for p in ["originals", "attempts", "published"] {
        fs::create_dir_all(root.join(p))?;
    }
    let db = Connection::open(root.join("jobs.sqlite3"))?;
    db.pragma_update(None, "journal_mode", "WAL")?;
    db.busy_timeout(Duration::from_secs(5))?;
    db.execute_batch("CREATE TABLE IF NOT EXISTS jobs(id TEXT PRIMARY KEY, filename TEXT NOT NULL, original TEXT NOT NULL, sha256 TEXT NOT NULL, status TEXT NOT NULL, attempt INTEGER NOT NULL DEFAULT 0, lease_until INTEGER, bundle TEXT, error TEXT, created_at INTEGER NOT NULL, retry_at INTEGER); CREATE INDEX IF NOT EXISTS jobs_status ON jobs(status, created_at);")?;
    // Bases creadas antes de los reintentos con espera.
    let has_retry = db
        .prepare("SELECT 1 FROM pragma_table_info('jobs') WHERE name='retry_at'")?
        .exists([])?;
    if !has_retry
        && let Err(e) = db.execute_batch("ALTER TABLE jobs ADD COLUMN retry_at INTEGER")
        && !e.to_string().contains("duplicate column")
    {
        return Err(e.into());
    }
    Ok(App {
        root,
        db: Arc::new(Mutex::new(db)),
        cfg: Arc::new(worker::Config::from_env()?),
        converter: None,
    })
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
fn valid_name(s: &str) -> bool {
    !s.is_empty() && !s.contains('/') && !s.contains('\\') && !s.contains("..")
}

/// Control de carga en la admisión, antes de leer el archivo: cola llena (`429`) o
/// volumen de datos sin el espacio libre mínimo (`507`).
async fn admit(a: &App) -> Result<(), (StatusCode, String)> {
    if a.cfg.max_queued > 0 {
        let pending: u64 =
            a.db.lock()
                .await
                .query_row(
                    "SELECT COUNT(*) FROM jobs WHERE status IN ('queued','processing')",
                    [],
                    |r| r.get(0),
                )
                .map_err(internal)?;
        if pending >= a.cfg.max_queued {
            return Err((
                StatusCode::TOO_MANY_REQUESTS,
                format!("hay {pending} trabajos pendientes; inténtalo más tarde"),
            ));
        }
    }
    if a.cfg.min_free_mb > 0 {
        let free_mb = free_space_mb(&a.root).map_err(internal)?;
        if free_mb < a.cfg.min_free_mb {
            return Err((
                StatusCode::INSUFFICIENT_STORAGE,
                format!("el volumen de datos solo tiene {free_mb} MiB libres"),
            ));
        }
    }
    Ok(())
}
/// Espacio disponible para el proceso en el sistema de archivos de `path`, en MiB.
fn free_space_mb(path: &FsPath) -> std::io::Result<u64> {
    let s = rustix::fs::statvfs(path)?;
    Ok(s.f_bavail.saturating_mul(s.f_frsize) / (1024 * 1024))
}

async fn submit(
    State(a): State<App>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    admit(&a).await?;
    let mut filename = None;
    let mut bytes = None;
    // Un cuerpo por encima del límite llega como error del multipart, con su `413`.
    let rejected = |e: axum::extract::multipart::MultipartError| (e.status(), e.body_text());
    while let Some(field) = multipart.next_field().await.map_err(rejected)? {
        if let Some(n) = field.file_name() {
            filename = Some(n.to_string());
            bytes = Some(field.bytes().await.map_err(rejected)?.to_vec());
            break;
        }
    }
    let filename = filename.ok_or((
        StatusCode::BAD_REQUEST,
        "multipart requiere un archivo".into(),
    ))?;
    if !valid_name(&filename) {
        return Err((
            StatusCode::BAD_REQUEST,
            "nombre de archivo no válido".into(),
        ));
    }
    let Some(format) = format::Format::from_filename(&filename) else {
        let ext = FsPath::new(&filename)
            .extension()
            .and_then(|x| x.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        return Err((
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            format!(
                "formato .{ext} aún no implementado; {}",
                format::supported()
            ),
        ));
    };
    let bytes = bytes.unwrap_or_default();
    if bytes.is_empty() || bytes.len() > a.cfg.max_upload_bytes() {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            format!(
                "el archivo está vacío o supera {} MiB (OKF_MAX_UPLOAD_MB)",
                a.cfg.max_upload_mb
            ),
        ));
    }
    let id = Uuid::new_v4().to_string();
    let hash = bundle::hex(&Sha256::digest(&bytes));
    let original_rel = format!("originals/{id}.{}", format.extension());
    let path = a.root.join(&original_rel);
    fs::write(&path, &bytes).map_err(internal)?;
    let tx = a.db.lock().await;
    if let Err(e) = tx.execute("INSERT INTO jobs(id,filename,original,sha256,status,created_at) VALUES(?1,?2,?3,?4,'queued',?5)", params![id, filename, original_rel, hash, now()]) { let _ = fs::remove_file(path); return Err(internal(e)); }
    Ok((
        StatusCode::ACCEPTED,
        Json(Accepted {
            job_id: id,
            status: "queued".into(),
        }),
    ))
}
fn internal(e: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}
async fn status(
    State(a): State<App>,
    Path(id): Path<String>,
) -> Result<Json<JobStatus>, (StatusCode, String)> {
    let db = a.db.lock().await;
    let row = db
        .query_row(
            "SELECT status,attempt,error FROM jobs WHERE id=?1",
            [&id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, u32>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(internal)?;
    row.map(|(status, attempt, error)| {
        Json(JobStatus {
            job_id: id,
            status,
            attempt,
            error,
        })
    })
    .ok_or((StatusCode::NOT_FOUND, "trabajo inexistente".into()))
}
async fn download(
    State(a): State<App>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let db = a.db.lock().await;
    let path: Option<String> = db
        .query_row(
            "SELECT bundle FROM jobs WHERE id=?1 AND status IN ('complete','partial')",
            [&id],
            |r| r.get(0),
        )
        .optional()
        .map_err(internal)?
        .flatten();
    drop(db);
    let Some(path) = path else {
        return Err((
            StatusCode::NOT_FOUND,
            "el bundle aún no está publicado".into(),
        ));
    };
    let bytes = fs::read(a.root.join(path)).map_err(internal)?;
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/zip".parse().unwrap());
    headers.insert(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"{id}.zip\"")
            .parse()
            .unwrap(),
    );
    Ok((headers, bytes))
}
async fn serve(root: PathBuf, bind: &str) -> Result<()> {
    let a = app(root)?;
    let router = router(a);
    let listener = tokio::net::TcpListener::bind(bind).await?;
    info!(%bind,"API escuchando");
    axum::serve(listener, router).await?;
    Ok(())
}
fn router(a: App) -> Router {
    let body_limit = a.cfg.max_upload_bytes().saturating_add(64 * 1024);
    Router::new()
        .route("/jobs", post(submit))
        .route("/jobs/{id}", get(status))
        .route("/jobs/{id}/bundle", get(download))
        .layer(axum::extract::DefaultBodyLimit::max(body_limit))
        .with_state(a)
}

#[tokio::main]
async fn main() -> Result<()> {
    // Los registros van a la salida de error: la estándar del subcomando `convert`
    // transporta su resultado al worker.
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();
    ooxml::set_limits(worker::Config::from_env()?.zip);
    let root = PathBuf::from(std::env::var("OKF_DATA_DIR").unwrap_or_else(|_| "data".into()));
    match cli.command {
        Command::Api { bind } => serve(root, &bind).await,
        Command::Worker { poll_ms, once } => worker::worker(root, poll_ms, once).await,
        Command::Convert {
            original,
            attempt,
            filename,
        } => worker::convert_command(&original, &attempt, &filename),
        Command::BenchRender { pptx, output } => pptx::bench_render(&pptx, &output),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bundle::{hex, validate_bundle, write_zip_file};
    use pptx::{build_bundle, extract};
    use std::io::Read;
    use worker::{FailureKind, renew, work_one};
    use zip::{ZipArchive, ZipWriter};
    /// Aplicación de prueba: conversión en el propio proceso y reintentos sin espera.
    fn test_app(root: &FsPath) -> App {
        let mut a = app(root.to_path_buf()).unwrap();
        Arc::make_mut(&mut a.cfg).retry_base_s = 0;
        a
    }
    async fn job(a: &App, id: &str) -> (String, u32, Option<String>, Option<String>) {
        a.db.lock()
            .await
            .query_row(
                "SELECT status,attempt,error,bundle FROM jobs WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap()
    }
    use tower::ServiceExt;
    async fn enqueue(
        a: &App,
        id: &str,
        filename: &str,
        bytes: &[u8],
        status: &str,
        attempt: i64,
        lease: i64,
    ) {
        let rel = format!("originals/{id}.pptx");
        fs::write(a.root.join(&rel), bytes).unwrap();
        a.db.lock().await.execute(
            "INSERT INTO jobs(id,filename,original,sha256,status,attempt,lease_until,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![id,filename,rel,hex(&Sha256::digest(bytes)),status,attempt,lease,now()],
        ).unwrap();
    }
    fn example(name: &str) -> Vec<u8> {
        fs::read(format!("examples/pptx/{name}.pptx")).unwrap()
    }
    #[tokio::test]
    async fn pptx_end_to_end_complete_partial_and_durable_recovery() {
        let tmp = tempfile::tempdir().unwrap();
        let a = test_app(tmp.path());
        let simple = example("simple");
        let boundary = "pptx-test-boundary";
        let mut body = format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"simple.pptx\"\r\nContent-Type: application/vnd.openxmlformats-officedocument.presentationml.presentation\r\n\r\n").into_bytes();
        body.extend_from_slice(&simple);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let response = router(a.clone())
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/jobs")
                    .header(
                        "content-type",
                        format!("multipart/form-data; boundary={boundary}"),
                    )
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let response_body = http_body_util::BodyExt::collect(response.into_body())
            .await
            .unwrap()
            .to_bytes();
        let accepted: Accepted = serde_json::from_slice(&response_body).unwrap();
        assert_eq!(accepted.status, "queued");
        let unsupported = b"--pptx-test-boundary\r\nContent-Disposition: form-data; name=\"file\"; filename=\"later.odt\"\r\nContent-Type: application/vnd.oasis.opendocument.text\r\n\r\nplaceholder\r\n--pptx-test-boundary--\r\n";
        let rejected = router(a.clone())
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/jobs")
                    .header(
                        "content-type",
                        "multipart/form-data; boundary=pptx-test-boundary",
                    )
                    .body(axum::body::Body::from(unsupported.as_slice()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(rejected.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
        assert!(work_one(&a).await.unwrap());
        let completed: String =
            a.db.lock()
                .await
                .query_row(
                    "SELECT status FROM jobs WHERE id=?1",
                    [&accepted.job_id],
                    |r| r.get(0),
                )
                .unwrap();
        assert_eq!(completed, "complete");
        let status_response = router(a.clone())
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!("/jobs/{}", accepted.job_id))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(status_response.status(), StatusCode::OK);
        let download_response = router(a.clone())
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!("/jobs/{}/bundle", accepted.job_id))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(download_response.status(), StatusCode::OK);
        let zip_path = a
            .root
            .join("published")
            .join(&accepted.job_id)
            .join("bundle.zip");
        validate_bundle(&zip_path).unwrap();
        let mut z = ZipArchive::new(fs::File::open(zip_path).unwrap()).unwrap();
        let mut idx = String::new();
        z.by_name("index.md")
            .unwrap()
            .read_to_string(&mut idx)
            .unwrap();
        assert!(idx.contains("contenido/slide-001.md"));
        assert!(idx.contains("contenido/index.md"));
        let mut concept = String::new();
        z.by_name("contenido/slide-001.md")
            .unwrap()
            .read_to_string(&mut concept)
            .unwrap();
        assert!(concept.contains("display_label: \"1\""));
        let mut content_index = String::new();
        z.by_name("contenido/index.md")
            .unwrap()
            .read_to_string(&mut content_index)
            .unwrap();
        assert!(content_index.contains("[Diapositiva 1: A simple OKF test deck](slide-001.md)"));
        assert!(concept.contains(
            "description: \"Diapositiva 1 de simple.pptx: A simple OKF test deck. One short paragraph of slide content\""
        ));
        let multi = example("multiple");
        enqueue(&a, "multi", "multiple.pptx", &multi, "queued", 0, 0).await;
        assert!(work_one(&a).await.unwrap());
        let (state, attempt): (String, u32) =
            a.db.lock()
                .await
                .query_row(
                    "SELECT status,attempt FROM jobs WHERE id='multi'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
        // Una diapositiva sin título es una observación, no una pérdida.
        assert_eq!(state, "complete");
        assert_eq!(attempt, 1);
        let mut z =
            ZipArchive::new(fs::File::open(a.root.join("published/multi/bundle.zip")).unwrap())
                .unwrap();
        let mut slide = String::new();
        z.by_name("contenido/slide-003.md")
            .unwrap()
            .read_to_string(&mut slide)
            .unwrap();
        assert!(slide.contains("slide_index: 3"));
        assert!(slide.contains("slide_id: 258"));
        assert!(slide.contains("oculta"));
        assert!(slide.contains("## Contenido"));
        assert!(slide.contains("| Name | Value |"));
        assert!(slide.contains("| alpha | 1 |"));
        let mut noted = String::new();
        z.by_name("contenido/slide-001.md")
            .unwrap()
            .read_to_string(&mut noted)
            .unwrap();
        assert!(noted.contains("Notas del presentador"));
        assert!(noted.contains("Presenter notes kept separately"));
        let mut untitled = String::new();
        z.by_name("contenido/slide-002.md")
            .unwrap()
            .read_to_string(&mut untitled)
            .unwrap();
        assert!(untitled.contains("status: \"draft\""));
        assert!(untitled.contains("sin título explícito"));
        assert!(slide.contains("../assets/slide-003.png"));
        assert!(
            slide.contains("../assets/slide-003-image-01.png"),
            "{slide}"
        );
        let mut report = String::new();
        z.by_name("conversion-report.md")
            .unwrap()
            .read_to_string(&mut report)
            .unwrap();
        assert!(report.contains("Diapositivas inventariadas: 3"));
        assert!(report.contains("Diapositivas representadas con concepto y vista: 3"));
        let observations = report.split("## Observaciones").nth(1).unwrap();
        assert!(observations.contains("Diapositiva 2 sin título explícito"));
        assert!(report.contains("Sin pérdidas ni incertidumbres sustantivas."));
        let mut index = String::new();
        z.by_name("index.md")
            .unwrap()
            .read_to_string(&mut index)
            .unwrap();
        assert!(index.contains("(contenido/slide-003.md) — oculta"));
        let visual = example("visual");
        enqueue(&a, "visual", "visual.pptx", &visual, "queued", 0, 0).await;
        assert!(work_one(&a).await.unwrap());
        let state: String =
            a.db.lock()
                .await
                .query_row("SELECT status FROM jobs WHERE id='visual'", [], |r| {
                    r.get(0)
                })
                .unwrap();
        assert_eq!(state, "partial");
        let mut z =
            ZipArchive::new(fs::File::open(a.root.join("published/visual/bundle.zip")).unwrap())
                .unwrap();
        let mut slide = String::new();
        z.by_name("contenido/slide-001.md")
            .unwrap()
            .read_to_string(&mut slide)
            .unwrap();
        assert!(slide.contains("visual_only: true"));
        assert!(slide.contains("status: \"draft\""));
        assert!(slide.contains("Contenido solo visual"));
        assert!(slide.contains("../assets/slide-001-image-01.png"));
        let mut visual_report = String::new();
        z.by_name("conversion-report.md")
            .unwrap()
            .read_to_string(&mut visual_report)
            .unwrap();
        let losses = visual_report.split("## Observaciones").next().unwrap();
        assert!(losses.contains("Diapositiva 1: contenido solo visual"));
        let expected_hash = hex(&Sha256::digest(&multi));
        assert!(report.contains(&expected_hash));
        let interrupted = example("simple");
        enqueue(
            &a,
            "recovery",
            "simple.pptx",
            &interrupted,
            "processing",
            1,
            now() - 1,
        )
        .await;
        let stale = a.root.join("attempts/recovery/1/stale.txt");
        fs::create_dir_all(stale.parent().unwrap()).unwrap();
        fs::write(&stale, "abandoned attempt").unwrap();
        assert!(work_one(&a).await.unwrap());
        let (state, attempt): (String, u32) =
            a.db.lock()
                .await
                .query_row(
                    "SELECT status,attempt FROM jobs WHERE id='recovery'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
        assert_eq!(state, "complete");
        assert_eq!(attempt, 2);
        // Publicado el bundle, los intentos (incluido el abandonado) se eliminan.
        assert!(!a.root.join("attempts/recovery").exists());
        assert!(a.root.join("published/recovery/bundle.zip").exists());
    }
    #[tokio::test]
    async fn malformed_pptx_fails_without_publishing_and_missing_view_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let a = test_app(tmp.path());
        enqueue(&a, "broken", "broken.pptx", b"not a pptx", "queued", 0, 0).await;
        assert!(work_one(&a).await.unwrap());
        let (state, bundle): (String, Option<String>) =
            a.db.lock()
                .await
                .query_row(
                    "SELECT status,bundle FROM jobs WHERE id='broken'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
        assert_eq!(state, "failed");
        assert!(bundle.is_none());
        let simple = example("simple");
        let attempt = tmp.path().join("validator");
        fs::create_dir_all(&attempt).unwrap();
        let extraction = extract(&simple, &attempt).unwrap();
        let good = build_bundle(
            &simple,
            "simple.pptx",
            &hex(&Sha256::digest(&simple)),
            &extraction,
            &attempt,
        )
        .unwrap()
        .0;
        let mut original = ZipArchive::new(fs::File::open(good).unwrap()).unwrap();
        let bad = tmp.path().join("missing-view.zip");
        let mut z = ZipWriter::new(fs::File::create(&bad).unwrap());
        for i in 0..original.len() {
            let mut entry = original.by_index(i).unwrap();
            let name = entry.name().to_string();
            if name == "assets/slide-001.png" {
                continue;
            }
            let mut content = Vec::new();
            entry.read_to_end(&mut content).unwrap();
            write_zip_file(&mut z, &name, &content).unwrap();
        }
        z.finish().unwrap();
        assert!(validate_bundle(&bad).is_err());
    }
    #[tokio::test]
    async fn failures_are_classified_and_transient_ones_retried_up_to_the_limit() {
        let tmp = tempfile::tempdir().unwrap();
        let mut a = test_app(tmp.path());
        // Original ausente: error del sistema de archivos, transitorio.
        enqueue(&a, "missing", "missing.pptx", b"x", "queued", 0, 0).await;
        fs::remove_file(a.root.join("originals/missing.pptx")).unwrap();
        assert!(work_one(&a).await.unwrap());
        let (state, attempt, error, _) = job(&a, "missing").await;
        assert_eq!((state.as_str(), attempt), ("queued", 1));
        assert!(error.unwrap().contains("se reintentará"));
        assert!(work_one(&a).await.unwrap());
        assert!(work_one(&a).await.unwrap());
        let (state, attempt, error, _) = job(&a, "missing").await;
        assert_eq!((state.as_str(), attempt), ("failed", 3));
        assert!(error.unwrap().starts_with("falló tras 3 intentos"));
        assert!(!work_one(&a).await.unwrap());
        assert!(!a.root.join("attempts/missing").exists());
        // Archivo inválido: permanente, sin reintentos.
        enqueue(&a, "broken", "broken.pptx", b"not a pptx", "queued", 0, 0).await;
        assert!(work_one(&a).await.unwrap());
        let (state, attempt, error, _) = job(&a, "broken").await;
        assert_eq!((state.as_str(), attempt), ("failed", 1));
        assert!(!error.unwrap().contains("reintent"));
        // Formato sin adaptador: permanente.
        enqueue(&a, "later", "notes.odt", b"PK", "queued", 0, 0).await;
        assert!(work_one(&a).await.unwrap());
        let (state, attempt, error, _) = job(&a, "later").await;
        assert_eq!((state.as_str(), attempt), ("failed", 1));
        assert!(error.unwrap().contains("formato no admitido: notes.odt"));
        // Límite de diapositivas: permanente.
        Arc::make_mut(&mut a.cfg).max_slides = 2;
        enqueue(
            &a,
            "long",
            "multiple.pptx",
            &example("multiple"),
            "queued",
            0,
            0,
        )
        .await;
        assert!(work_one(&a).await.unwrap());
        let (state, attempt, error, bundle) = job(&a, "long").await;
        assert_eq!((state.as_str(), attempt, bundle), ("failed", 1, None));
        assert!(
            error
                .unwrap()
                .contains("3 diapositivas; el máximo admitido es 2")
        );
        assert_eq!(
            worker::classify(&anyhow::Error::new(std::io::Error::from(
                std::io::ErrorKind::StorageFull
            ))),
            FailureKind::Transient
        );
    }
    #[tokio::test]
    async fn leases_are_renewed_by_their_owner_and_expired_ones_bounded() {
        let tmp = tempfile::tempdir().unwrap();
        let a = test_app(tmp.path());
        enqueue(
            &a,
            "owned",
            "simple.pptx",
            &example("simple"),
            "processing",
            1,
            now() + 5,
        )
        .await;
        assert!(renew(&a, "owned", 1).await.unwrap());
        let lease: i64 =
            a.db.lock()
                .await
                .query_row("SELECT lease_until FROM jobs WHERE id='owned'", [], |r| {
                    r.get(0)
                })
                .unwrap();
        assert!(lease >= now() + a.cfg.lease_s - 1);
        // Otro worker reclamó el trabajo (intento 2): el intento 1 ya no puede renovarla.
        assert!(!renew(&a, "owned", 2).await.unwrap());
        // Un trabajo que interrumpe al worker en cada intento se detiene en el máximo.
        enqueue(
            &a,
            "crashy",
            "simple.pptx",
            &example("simple"),
            "processing",
            3,
            now() - 1,
        )
        .await;
        a.db.lock()
            .await
            .execute(
                "UPDATE jobs SET status='done-elsewhere' WHERE id='owned'",
                [],
            )
            .unwrap();
        assert!(!work_one(&a).await.unwrap());
        let (state, _, error, _) = job(&a, "crashy").await;
        assert_eq!(state, "failed");
        assert!(error.unwrap().contains("se interrumpió en 3 intentos"));
    }
    /// Bundle válido de un ejemplo, construido como lo haría el worker.
    fn bundle_of(name: &str, dir: &FsPath) -> PathBuf {
        let bytes = example(name);
        fs::create_dir_all(dir).unwrap();
        let extraction = extract(&bytes, dir).unwrap();
        build_bundle(
            &bytes,
            &format!("{name}.pptx"),
            &hex(&Sha256::digest(&bytes)),
            &extraction,
            dir,
        )
        .unwrap()
        .0
    }
    #[tokio::test]
    async fn startup_reconciliation_repairs_interrupted_publications() {
        let tmp = tempfile::tempdir().unwrap();
        let a = test_app(tmp.path());
        // El worker murió después de publicar y antes de registrar el estado.
        enqueue(
            &a,
            "published",
            "simple.pptx",
            &example("simple"),
            "processing",
            1,
            now() - 1,
        )
        .await;
        let pubdir = a.root.join("published/published");
        fs::create_dir_all(&pubdir).unwrap();
        fs::copy(
            bundle_of("simple", &tmp.path().join("b1")),
            pubdir.join("bundle.zip"),
        )
        .unwrap();
        // Publicación a medio hacer, bundle registrado que desapareció e intentos huérfanos.
        fs::create_dir_all(a.root.join("published/.half.tmp")).unwrap();
        enqueue(
            &a,
            "lost",
            "simple.pptx",
            &example("simple"),
            "complete",
            1,
            0,
        )
        .await;
        a.db.lock()
            .await
            .execute(
                "UPDATE jobs SET bundle='published/lost/bundle.zip' WHERE id='lost'",
                [],
            )
            .unwrap();
        enqueue(
            &a,
            "dead",
            "simple.pptx",
            &example("simple"),
            "failed",
            1,
            0,
        )
        .await;
        enqueue(
            &a,
            "waiting",
            "simple.pptx",
            &example("simple"),
            "queued",
            1,
            0,
        )
        .await;
        for id in ["dead", "ghost", "waiting"] {
            fs::create_dir_all(a.root.join(format!("attempts/{id}/1"))).unwrap();
        }
        worker::reconcile(&a).await.unwrap();
        let (state, attempt, _, bundle) = job(&a, "published").await;
        assert_eq!((state.as_str(), attempt), ("complete", 1));
        assert_eq!(bundle.as_deref(), Some("published/published/bundle.zip"));
        assert!(!a.root.join("published/.half.tmp").exists());
        let (state, _, error, bundle) = job(&a, "lost").await;
        assert_eq!((state.as_str(), bundle), ("queued", None));
        assert!(error.unwrap().contains("bundle publicado no existe"));
        assert!(!a.root.join("attempts/dead").exists());
        assert!(!a.root.join("attempts/ghost").exists());
        assert!(a.root.join("attempts/waiting/1").exists());
        // Un bundle publicado inválido se descarta y el trabajo vuelve a convertirse.
        enqueue(
            &a,
            "corrupt",
            "simple.pptx",
            &example("simple"),
            "processing",
            1,
            now() - 1,
        )
        .await;
        fs::create_dir_all(a.root.join("published/corrupt")).unwrap();
        fs::write(a.root.join("published/corrupt/bundle.zip"), b"partial").unwrap();
        worker::reconcile(&a).await.unwrap();
        assert_eq!(job(&a, "corrupt").await.0, "queued");
        assert!(!a.root.join("published/corrupt").exists());
    }
    /// Copia un bundle aplicando `edit` a cada entrada.
    fn rewrite(src: &FsPath, dst: &FsPath, edit: impl Fn(&str, String) -> String) {
        let mut original = ZipArchive::new(fs::File::open(src).unwrap()).unwrap();
        let mut z = ZipWriter::new(fs::File::create(dst).unwrap());
        for i in 0..original.len() {
            let mut entry = original.by_index(i).unwrap();
            let name = entry.name().to_string();
            let mut content = Vec::new();
            entry.read_to_end(&mut content).unwrap();
            if name.ends_with(".md") || name.ends_with(".csv") {
                content = edit(&name, String::from_utf8(content).unwrap()).into_bytes();
            }
            write_zip_file(&mut z, &name, &content).unwrap();
        }
        z.finish().unwrap();
    }
    #[test]
    fn validator_checks_hash_inventory_spans_and_reserved_indexes() {
        let tmp = tempfile::tempdir().unwrap();
        let good = bundle_of("multiple", &tmp.path().join("good"));
        validate_bundle(&good).unwrap();
        let bad = tmp.path().join("bad.zip");
        type Edit<'a> = &'a dyn Fn(&str, String) -> String;
        let cases: [(&str, Edit); 5] = [
            ("SHA-256 del informe", &|n, md| {
                if n == "conversion-report.md" {
                    md.replacen("sha256: \"", "sha256: \"0", 1)
                } else {
                    md
                }
            }),
            ("SHA-256 de documento.md", &|n, md| {
                if n == "documento.md" {
                    md.replacen("sha256: \"", "sha256: \"0", 1)
                } else {
                    md
                }
            }),
            ("fuente no declarada", &|n, md| {
                if n == "contenido/slide-002.md" {
                    md.replace("source_id: \"original\"", "source_id: \"otra\"")
                } else {
                    md
                }
            }),
            ("no corresponde a la diapositiva 2", &|n, md| {
                if n == "contenido/slide-002.md" {
                    md.replace("slide_index: 2", "slide_index: 3")
                } else {
                    md
                }
            }),
            ("no admite frontmatter", &|n, md| {
                if n == "contenido/index.md" {
                    format!("---\ntype: Index\n---\n{md}")
                } else {
                    md
                }
            }),
        ];
        for (expected, edit) in cases {
            rewrite(&good, &bad, edit);
            let error = validate_bundle(&bad).unwrap_err().to_string();
            assert!(error.contains(expected), "{expected}: {error}");
        }
    }
    #[tokio::test]
    async fn docx_is_converted_by_the_worker_and_checked_against_its_blocks() {
        let tmp = tempfile::tempdir().unwrap();
        let a = test_app(tmp.path());
        let bytes = fs::read("examples/docx/estructura.docx").unwrap();
        enqueue(&a, "doc", "estructura.docx", &bytes, "queued", 0, 0).await;
        assert!(work_one(&a).await.unwrap());
        let (state, _, error, bundle) = job(&a, "doc").await;
        assert_eq!((state.as_str(), error), ("complete", None));
        let good = a.root.join(bundle.unwrap());
        validate_bundle(&good).unwrap();
        let bad = tmp.path().join("bad.zip");
        type Edit<'a> = &'a dyn Fn(&str, String) -> String;
        let cases: [(&str, Edit); 6] = [
            (
                "empieza en el bloque 10 y debía empezar en el 9",
                &|n, md| {
                    if n == "contenido/seccion-002.md" {
                        md.replace(
                            "id: \"seccion\"\n    source_id: \"original\"\n    kind: \"docx_blocks\"\n    from: 9\n",
                            "id: \"seccion\"\n    source_id: \"original\"\n    kind: \"docx_blocks\"\n    from: 10\n",
                        )
                    } else {
                        md
                    }
                },
            ),
            ("localizador de bloque fuera de su sección", &|n, md| {
                if n == "contenido/seccion-002.md" {
                    md.replace("from: 10\n    to: 10\n", "from: 3\n    to: 3\n")
                } else {
                    md
                }
            }),
            ("marcador de pasaje sin localizador", &|n, md| {
                if n == "contenido/seccion-001.md" {
                    md.replace(
                        "<!-- okf-source-span: b-3 -->",
                        "<!-- okf-source-span: b-99 -->",
                    )
                } else {
                    md
                }
            }),
            ("no está enlazado desde ningún concepto", &|n, md| {
                if n == "contenido/seccion-002.md" {
                    md.replace(
                        "![Diagrama del flujo](../assets/imagen-001.png)",
                        "Figura retirada.",
                    )
                } else {
                    md
                }
            }),
            ("marcador de pasaje sin bloque a continuación", &|n, md| {
                if n == "contenido/seccion-002.md" {
                    md.replace("![Diagrama del flujo](../assets/imagen-001.png)", "")
                } else {
                    md
                }
            }),
            ("documento.md no localiza los 13 bloques", &|n, md| {
                if n == "documento.md" {
                    md.replace("from: 1\n    to: 13\n", "from: 1\n    to: 12\n")
                } else {
                    md
                }
            }),
        ];
        for (expected, edit) in cases {
            rewrite(&good, &bad, edit);
            let error = validate_bundle(&bad).unwrap_err().to_string();
            assert!(error.contains(expected), "{expected}: {error}");
        }
    }

    #[tokio::test]
    async fn xlsx_is_converted_by_the_worker_and_checked_against_its_cells() {
        let tmp = tempfile::tempdir().unwrap();
        let a = test_app(tmp.path());
        let bytes = fs::read("examples/xlsx/muestras/ventas_trimestrales.xlsx").unwrap();
        enqueue(&a, "book", "ventas.xlsx", &bytes, "queued", 0, 0).await;
        assert!(work_one(&a).await.unwrap());
        let (state, _, error, bundle) = job(&a, "book").await;
        assert_eq!((state.as_str(), error), ("complete", None));
        let good = a.root.join(bundle.unwrap());
        validate_bundle(&good).unwrap();
        let bad = tmp.path().join("bad.zip");
        let only = |name: &'static str, from: &'static str, to: &'static str| {
            move |n: &str, md: String| if n == name { md.replace(from, to) } else { md }
        };
        type Edit = Box<dyn Fn(&str, String) -> String>;
        let cases: Vec<(&str, Edit)> = vec![
            (
                "la celda Resumen!A8 del original no está en ninguna región",
                Box::new(only(
                    "contenido/hoja-001.md",
                    "range: \"A1:F8\"",
                    "range: \"A1:F7\"",
                )),
            ),
            (
                "tiene un rango A10:A13 fuera del rango de la hoja",
                Box::new(only(
                    "contenido/hoja-001.md",
                    "range: \"A10:A12\"",
                    "range: \"A10:A13\"",
                )),
            ),
            (
                "no localiza el rango con contenido de la hoja «Resumen» (A1:F12)",
                Box::new(only(
                    "contenido/hoja-001.md",
                    "range: \"A1:F12\"",
                    "range: \"A1:G12\"",
                )),
            ),
            (
                "no lleva el nombre exacto de la hoja «Supuestos»",
                Box::new(only(
                    "contenido/hoja-003.md",
                    "title: \"Supuestos\"",
                    "title: \"Supuestos generales\"",
                )),
            ),
            (
                "localiza la hoja «Notas» y pertenece a «Supuestos»",
                Box::new(only(
                    "contenido/hoja-003.md",
                    "sheet: \"Supuestos\"\n    range: \"A1:C4\"\n---",
                    "sheet: \"Notas\"\n    range: \"A1:C4\"\n---",
                )),
            ),
            (
                "el CSV de contenido/hoja-002-region-01.md tiene 300 filas y la región A1:G301 tiene 301",
                Box::new(|n: &str, data: String| {
                    if n == "assets/hoja-002-region-01.csv" {
                        let cut = data.trim_end().rfind("\r\n").unwrap();
                        format!("{}\r\n", &data[..cut])
                    } else {
                        data
                    }
                }),
            ),
            (
                "un índice no enumera los conceptos en el orden del libro",
                Box::new(only(
                    "contenido/index.md",
                    "- [Supuestos](hoja-003.md)\n- [Notas](hoja-004.md)",
                    "- [Notas](hoja-004.md)\n- [Supuestos](hoja-003.md)",
                )),
            ),
        ];
        for (expected, edit) in cases {
            rewrite(&good, &bad, edit);
            let error = validate_bundle(&bad).unwrap_err().to_string();
            assert!(error.contains(expected), "{expected}: {error}");
        }
    }

    #[tokio::test]
    async fn pdf_is_converted_by_the_worker_and_checked_against_its_pages() {
        let tmp = tempfile::tempdir().unwrap();
        let a = test_app(tmp.path());
        let bytes = fs::read("examples/pdf/muestras/articulo_columnas.pdf").unwrap();
        enqueue(&a, "article", "articulo.pdf", &bytes, "queued", 0, 0).await;
        assert!(work_one(&a).await.unwrap());
        let (state, _, error, bundle) = job(&a, "article").await;
        assert_eq!((state.as_str(), error), ("complete", None));
        let good = a.root.join(bundle.unwrap());
        validate_bundle(&good).unwrap();
        let scanned = fs::read("examples/pdf/muestras/escaneado_mixto.pdf").unwrap();
        enqueue(&a, "scan", "expediente.pdf", &scanned, "queued", 0, 0).await;
        assert!(work_one(&a).await.unwrap());
        let (state, _, error, _) = job(&a, "scan").await;
        assert_eq!((state.as_str(), error), ("partial", None));
        let bad = tmp.path().join("bad.zip");
        let only = |name: &'static str, from: &'static str, to: &'static str| {
            move |n: &str, md: String| if n == name { md.replace(from, to) } else { md }
        };
        type Edit = Box<dyn Fn(&str, String) -> String>;
        let cases: Vec<(&str, Edit)> = vec![
            (
                "contenido/seccion-004.md tiene un bloque sin marcador de página",
                Box::new(only(
                    "contenido/seccion-004.md",
                    "<!-- okf-source-span: p-1 -->\n",
                    "",
                )),
            ),
            (
                "tiene un localizador de páginas fuera de su sección",
                Box::new(only(
                    "contenido/seccion-004.md",
                    "id: \"p-1\"\n    source_id: \"original\"\n    kind: \"pdf_pages\"\n    from: 1\n    to: 1\n",
                    "id: \"p-1\"\n    source_id: \"original\"\n    kind: \"pdf_pages\"\n    from: 3\n    to: 3\n",
                )),
            ),
            (
                "seccion-006.md empieza en la página 4 y el concepto anterior cubre 2–2",
                Box::new(only(
                    "contenido/seccion-006.md",
                    "kind: \"pdf_pages\"\n    from: 3\n    to: 3\n",
                    "kind: \"pdf_pages\"\n    from: 4\n    to: 4\n",
                )),
            ),
            (
                "documento.md no localiza las 4 páginas del original",
                Box::new(only(
                    "documento.md",
                    "kind: \"pdf_pages\"\n    from: 1\n    to: 4\n",
                    "kind: \"pdf_pages\"\n    from: 1\n    to: 5\n",
                )),
            ),
            (
                "page_labels tiene una entrada inválida para la página 9",
                Box::new(only("documento.md", "\"4\": \"3\"", "\"9\": \"3\"")),
            ),
            (
                "marcador de pasaje sin localizador en contenido/seccion-004.md: p-7",
                Box::new(only(
                    "contenido/seccion-004.md",
                    "okf-source-span: p-1-2 -->",
                    "okf-source-span: p-7 -->",
                )),
            ),
            (
                "un índice no enumera los conceptos en el orden del documento",
                Box::new(only(
                    "contenido/index.md",
                    "- [3. Resultados](seccion-005.md)\n- [4. Conclusiones](seccion-006.md)",
                    "- [4. Conclusiones](seccion-006.md)\n- [3. Resultados](seccion-005.md)",
                )),
            ),
        ];
        for (expected, edit) in cases {
            rewrite(&good, &bad, edit);
            let error = validate_bundle(&bad).unwrap_err().to_string();
            assert!(error.contains(expected), "{expected}: {error}");
        }
    }

    /// Envía `bytes` como `name` a `POST /jobs` y devuelve el código de estado.
    async fn post(a: &App, name: &str, bytes: &[u8]) -> StatusCode {
        let boundary = "admission-boundary";
        let mut body = format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\n\r\n").into_bytes();
        body.extend_from_slice(bytes);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        router(a.clone())
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/jobs")
                    .header(
                        "content-type",
                        format!("multipart/form-data; boundary={boundary}"),
                    )
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap()
            .status()
    }

    #[tokio::test]
    async fn admission_limits_upload_size_queue_and_free_space() {
        let tmp = tempfile::tempdir().unwrap();
        let mut a = test_app(tmp.path());
        let simple = example("simple");
        // Valores por omisión: sin límite de cola ni de espacio libre.
        assert_eq!(post(&a, "simple.pptx", &simple).await, StatusCode::ACCEPTED);
        assert_eq!(post(&a, "simple.pptx", &simple).await, StatusCode::ACCEPTED);
        Arc::make_mut(&mut a.cfg).max_upload_mb = 1;
        let big = vec![0u8; 1024 * 1024 + 1];
        assert_eq!(
            post(&a, "big.pptx", &big).await,
            StatusCode::PAYLOAD_TOO_LARGE
        );
        let over_body_limit = vec![0u8; 2 * 1024 * 1024];
        assert_eq!(
            post(&a, "big.pptx", &over_body_limit).await,
            StatusCode::PAYLOAD_TOO_LARGE
        );
        Arc::make_mut(&mut a.cfg).max_queued = 2;
        assert_eq!(
            post(&a, "simple.pptx", &simple).await,
            StatusCode::TOO_MANY_REQUESTS
        );
        Arc::make_mut(&mut a.cfg).max_queued = 3;
        assert_eq!(post(&a, "simple.pptx", &simple).await, StatusCode::ACCEPTED);
        Arc::make_mut(&mut a.cfg).max_queued = 0;
        Arc::make_mut(&mut a.cfg).min_free_mb = u64::MAX;
        assert_eq!(
            post(&a, "simple.pptx", &simple).await,
            StatusCode::INSUFFICIENT_STORAGE
        );
        let pending: u64 =
            a.db.lock()
                .await
                .query_row("SELECT COUNT(*) FROM jobs", [], |r| r.get(0))
                .unwrap();
        assert_eq!(pending, 3);
    }
}
