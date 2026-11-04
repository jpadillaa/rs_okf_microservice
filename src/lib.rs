use anyhow::Result;
use axum::{
    Json, Router,
    extract::{Multipart, Path, State},
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use std::{
    fs,
    path::{Path as FsPath, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use tracing::info;
use uuid::Uuid;

mod api_docs;
mod bundle;
#[cfg(test)]
mod coverage;
#[cfg(not(test))]
mod db;
mod docx;
mod format;
#[cfg(test)]
mod golden;
mod isolation;
mod observability;
mod omml;
mod ooxml;
mod output;
mod pdf;
mod pptx;
mod retention;
mod sections;
mod shutdown;
#[cfg(test)]
#[path = "../tests/support/mod.rs"]
mod test_support;
mod validate;
mod version;
mod worker;
mod xlsx;
mod xml;

#[derive(Parser)]
#[command(name = "okf-microservice")]
struct Cli {
    /// Archivo de variables; por omisión se lee .env del directorio actual.
    #[arg(long, global = true)]
    env_file: Option<PathBuf>,
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
    /// Valida conformidad OKF e integridad editorial frente al original incluido.
    Validate { bundle: PathBuf },
    /// Render every PPTX slide with the selected native Rust backend.
    BenchRender { pptx: PathBuf, output: PathBuf },
}

#[derive(Clone)]
struct App {
    root: PathBuf,
    db: PgPool,
    cfg: Arc<worker::Config>,
    /// Ejecutable para convertir en un proceso hijo vigilado; `None` convierte en el
    /// propio proceso (pruebas unitarias).
    converter: Option<PathBuf>,
    shutdown: Arc<shutdown::Shutdown>,
}
#[derive(Deserialize, Serialize, utoipa::ToSchema)]
struct Accepted {
    /// Identificador que se usa para consultar el estado y descargar el bundle.
    #[schema(format = "uuid")]
    job_id: String,
    /// Siempre `queued`: la conversión se ejecuta de forma asíncrona.
    #[schema(examples("queued"))]
    status: String,
}
#[derive(Serialize, utoipa::ToSchema)]
struct JobStatus {
    #[schema(format = "uuid")]
    job_id: String,
    /// `queued`, `processing`, `complete`, `partial`, `failed` o `expired`.
    #[schema(examples("complete"))]
    status: String,
    /// Número de intentos iniciados; cero mientras espera el primero.
    attempt: u32,
    /// Motivo del fallo, o null si no hay un error registrado.
    #[schema(required)]
    error: Option<String>,
    #[serde(flatten)]
    versions: version::Versions,
}

async fn app(root: PathBuf) -> Result<App> {
    for p in ["originals", "attempts", "published"] {
        fs::create_dir_all(root.join(p))?;
    }
    #[cfg(not(test))]
    let db = db::connect().await?;
    #[cfg(test)]
    let db = test_support::pool(&root).await;
    Ok(App {
        root,
        db,
        cfg: Arc::new(worker::Config::from_env()?),
        converter: None,
        shutdown: Arc::new(shutdown::Shutdown::default()),
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
        let pending: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE status IN ('queued','processing')")
                .fetch_one(&a.db)
                .await
                .map_err(internal)?;
        if pending as u64 >= a.cfg.max_queued {
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

/// Subir un documento y crear un trabajo
///
/// Selecciona un PDF, DOCX, XLSX o PPTX en el campo `file`. Se procesa el primer
/// archivo del formulario. El límite es 100 MiB por defecto, configurable con
/// `OKF_MAX_UPLOAD_MB`. Guarda el `job_id` de la respuesta para consultar el estado.
#[utoipa::path(
    post, path = "/jobs", tag = "Trabajos", operation_id = "crear_trabajo",
    request_body(content = api_docs::Upload, content_type = "multipart/form-data"),
    responses(
        (status = 202, description = "Original conservado y trabajo en cola", body = Accepted),
        (status = 400, description = "Formulario, nombre no válido o archivo vacío", body = String, content_type = "text/plain"),
        (status = 413, description = "Tamaño por encima del límite", body = String, content_type = "text/plain"),
        (status = 415, description = "Extensión no admitida", body = String, content_type = "text/plain"),
        (status = 429, description = "Cola llena; reintentar más tarde", body = String, content_type = "text/plain"),
        (status = 500, description = "Error de persistencia", body = String, content_type = "text/plain"),
        (status = 507, description = "Espacio libre insuficiente", body = String, content_type = "text/plain")
    )
)]
async fn submit(
    State(a): State<App>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    admit(&a).await?;
    // Un cuerpo por encima del límite llega como error del multipart, con su `413`.
    let rejected = |e: axum::extract::multipart::MultipartError| (e.status(), e.body_text());
    let mut field = loop {
        match multipart.next_field().await.map_err(rejected)? {
            Some(field) if field.file_name().is_some() => break field,
            Some(_) => continue,
            None => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "multipart requiere un archivo".into(),
                ));
            }
        }
    };
    let filename = field.file_name().unwrap_or_default().to_string();
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
    let id = Uuid::new_v4().to_string();
    let original_rel = format!("originals/{id}.{}", format.extension());
    let path = a.root.join(&original_rel);
    // El original se escribe a medida que llega y se calcula su huella en el mismo paso,
    // sin reunir el archivo en memoria. Cualquier rechazo posterior borra lo escrito.
    let stored = store_upload(&a, &mut field, &path).await;
    let hash = match stored {
        Ok(hash) => hash,
        Err(e) => {
            let _ = tokio::fs::remove_file(&path).await;
            return Err(e);
        }
    };
    if let Err(e) = sqlx::query(
        "INSERT INTO jobs(id,filename,original,sha256,status,created_at) VALUES($1,$2,\
        $3,$4,'queued',$5)",
    )
    .bind(&id)
    .bind(&filename)
    .bind(&original_rel)
    .bind(&hash)
    .bind(now())
    .execute(&a.db)
    .await
    {
        let _ = tokio::fs::remove_file(path).await;
        return Err(internal(e));
    }
    Ok((
        StatusCode::ACCEPTED,
        Json(Accepted {
            job_id: id,
            status: "queued".into(),
        }),
    ))
}
/// Escribe el campo del archivo en `path` por partes y devuelve su SHA-256 en
/// hexadecimal. Rechaza un archivo vacío (`400`) o mayor que `OKF_MAX_UPLOAD_MB` (`413`).
async fn store_upload(
    a: &App,
    field: &mut axum::extract::multipart::Field<'_>,
    path: &FsPath,
) -> Result<String, (StatusCode, String)> {
    use tokio::io::AsyncWriteExt;
    let limit = a.cfg.max_upload_bytes();
    let too_large = || {
        (
            StatusCode::PAYLOAD_TOO_LARGE,
            format!(
                "el archivo supera {} MiB (OKF_MAX_UPLOAD_MB)",
                a.cfg.max_upload_mb
            ),
        )
    };
    let mut file = tokio::fs::File::create(path).await.map_err(internal)?;
    let mut hasher = Sha256::new();
    let mut size = 0usize;
    while let Some(chunk) = field
        .chunk()
        .await
        .map_err(|e| (e.status(), e.body_text()))?
    {
        size = size.saturating_add(chunk.len());
        if size > limit {
            return Err(too_large());
        }
        hasher.update(&chunk);
        file.write_all(&chunk).await.map_err(internal)?;
    }
    if size == 0 {
        return Err((StatusCode::BAD_REQUEST, "el archivo está vacío".into()));
    }
    file.sync_all().await.map_err(internal)?;
    Ok(bundle::hex(&hasher.finalize()))
}
fn internal(e: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}
/// Consultar el estado de un trabajo
///
/// Usa el `job_id` recibido al subir el archivo. Repite la consulta mientras esté
/// `queued` o `processing`. Los estados `complete` y `partial` permiten descargar
/// el bundle; `failed` indica un fallo y `expired` que se eliminaron sus archivos.
#[utoipa::path(
    get, path = "/jobs/{id}", tag = "Trabajos", operation_id = "consultar_trabajo",
    params(("id" = String, Path, description = "job_id devuelto por POST /jobs")),
    responses(
        (status = 200, description = "Estado e identidad del conversor; las versiones pueden ser null antes de publicar", body = JobStatus),
        (status = 404, description = "Trabajo inexistente", body = String, content_type = "text/plain"),
        (status = 500, description = "Error al consultar PostgreSQL", body = String, content_type = "text/plain")
    )
)]
async fn status(
    State(a): State<App>,
    Path(id): Path<String>,
) -> Result<Json<JobStatus>, (StatusCode, String)> {
    let row = sqlx::query(
        "SELECT status,attempt,error,converter_version,converter_commit,profile_version \
        FROM jobs WHERE id=$1",
    )
    .bind(&id)
    .fetch_optional(&a.db)
    .await
    .map_err(internal)?;
    row.map(|r| -> Result<_, sqlx::Error> {
        Ok(Json(JobStatus {
            job_id: id,
            status: r.try_get(0)?,
            attempt: r.try_get::<i64, _>(1)? as u32,
            error: r.try_get(2)?,
            versions: version::Versions {
                converter_version: r.try_get(3)?,
                converter_commit: r.try_get(4)?,
                profile_version: r.try_get(5)?,
            },
        }))
    })
    .transpose()
    .map_err(internal)?
    .ok_or((StatusCode::NOT_FOUND, "trabajo inexistente".into()))
}
/// Descargar el bundle OKF en ZIP
///
/// Disponible cuando el trabajo termina en `complete` o `partial`. El enlace de
/// descarga de la respuesta permite guardar el ZIP con el nombre del trabajo.
#[utoipa::path(
    get, path = "/jobs/{id}/bundle", tag = "Trabajos", operation_id = "descargar_bundle",
    params(("id" = String, Path, description = "job_id de un trabajo complete o partial")),
    responses(
        (status = 200, description = "Bundle publicado", body = inline(api_docs::BinaryFile), content_type = "application/zip",
            headers(("Content-Disposition" = String, description = "attachment; filename=\"<job_id>.zip\""))),
        (status = 404, description = "Trabajo inexistente o bundle aún no publicado", body = String, content_type = "text/plain"),
        (status = 410, description = "Trabajo expirado; archivos eliminados por retención", body = String, content_type = "text/plain"),
        (status = 500, description = "Error al leer el registro o el archivo", body = String, content_type = "text/plain")
    )
)]
async fn download(
    State(a): State<App>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let row: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT status,bundle FROM jobs WHERE id=$1")
            .bind(&id)
            .fetch_optional(&a.db)
            .await
            .map_err(internal)?;
    let Some((state, path)) = row else {
        return Err((StatusCode::NOT_FOUND, "trabajo inexistente".into()));
    };
    if state == "expired" {
        return Err((
            StatusCode::GONE,
            "el trabajo expiró; sus archivos ya no están disponibles".into(),
        ));
    }
    let Some(path) = path.filter(|_| matches!(state.as_str(), "complete" | "partial")) else {
        return Err((
            StatusCode::NOT_FOUND,
            "el bundle aún no está publicado".into(),
        ));
    };
    // Si la retención borra el archivo entre la consulta y la apertura, devuelve 410.
    // Una vez abierto, el archivo se sirve completo aunque se borre durante el envío.
    let file = match tokio::fs::File::open(a.root.join(path)).await {
        Ok(file) => file,
        Err(error) => {
            let expired: bool = sqlx::query_scalar("SELECT status='expired' FROM jobs WHERE id=$1")
                .bind(&id)
                .fetch_one(&a.db)
                .await
                .unwrap_or(false);
            if expired {
                return Err((StatusCode::GONE, "el trabajo expiró".into()));
            }
            return Err(internal(error));
        }
    };
    let length = file.metadata().await.map_err(internal)?.len();
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/zip".parse().unwrap());
    headers.insert(header::CONTENT_LENGTH, length.into());
    headers.insert(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"{id}.zip\"")
            .parse()
            .unwrap(),
    );
    // Se envía por partes: la memoria de la API no depende del tamaño del bundle.
    let body = axum::body::Body::from_stream(tokio_util::io::ReaderStream::new(file));
    Ok((headers, body))
}
async fn serve(root: PathBuf, bind: &str) -> Result<()> {
    let mut a = app(root).await?;
    a.shutdown = shutdown::install()?;
    let shutdown = a.shutdown.clone();
    let router = router(a);
    let listener = tokio::net::TcpListener::bind(bind).await?;
    info!(%bind,"API escuchando");
    axum::serve(listener, router)
        .with_graceful_shutdown(async move { shutdown.wait().await })
        .await?;
    Ok(())
}
fn router(a: App) -> Router {
    let body_limit = a.cfg.max_upload_bytes().saturating_add(64 * 1024);
    let mut router = Router::new()
        .route("/health", get(observability::health))
        .route("/metrics", get(observability::metrics))
        .route("/jobs", post(submit))
        .route("/jobs/{id}", get(status))
        .route("/jobs/{id}/bundle", get(download));
    if a.cfg.swagger_enabled {
        router = router.merge(api_docs::swagger_ui());
    }
    router
        .layer(axum::extract::DefaultBodyLimit::max(body_limit))
        .with_state(a)
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    // Antes de crear hilos. El conversor nunca carga archivos de entorno ni credenciales.
    if !matches!(cli.command, Command::Convert { .. }) {
        if let Some(path) = &cli.env_file {
            dotenvy::from_path(path)
                .map_err(|_| anyhow::anyhow!("no se pudo leer el archivo --env-file"))?;
        } else if let Err(error) = dotenvy::from_path(".env")
            && !error.not_found()
        {
            anyhow::bail!("no se pudo leer .env")
        }
    }
    // Los registros van a la salida de error: la estándar del subcomando `convert`
    // transporta su resultado al worker.
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    ooxml::set_limits(worker::Config::from_env()?.zip);
    let root = PathBuf::from(std::env::var("OKF_DATA_DIR").unwrap_or_else(|_| "data".into()));
    if let Command::Validate { bundle } = &cli.command {
        worker::install_panic_hook();
        isolation::resource_limits(&worker::Config::from_env()?)?;
        validate::path(bundle)?;
        println!("bundle válido: {}", bundle.display());
        return Ok(());
    }
    // `convert` impone el aislamiento antes de crear cualquier hilo.
    if let Command::Convert {
        original,
        attempt,
        filename,
    } = cli.command
    {
        return worker::convert_command(&original, &attempt, &filename);
    }
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async {
            match cli.command {
                Command::Api { bind } => serve(root, &bind).await,
                Command::Worker { poll_ms, once } => worker::worker(root, poll_ms, once).await,
                Command::Convert {
                    original,
                    attempt,
                    filename,
                } => worker::convert_command(&original, &attempt, &filename),
                Command::Validate { .. } => unreachable!("validate se ejecuta antes del runtime"),
                Command::BenchRender { pptx, output } => pptx::bench_render(&pptx, &output),
            }
        })
}

/// Entrada en memoria para cargo-fuzz: mismos adaptadores y límites lógicos del servicio.
/// Los errores de formato son esperables; los pánicos se dejan llegar a libFuzzer.
#[doc(hidden)]
pub fn fuzz_input(extension: &str, bytes: &[u8]) {
    let cfg = worker::Config::default();
    if bytes.len() > cfg.max_upload_bytes() {
        return;
    }
    ooxml::set_limits(cfg.zip);
    let format = format::Format::from_extension(extension).expect("objetivo de fuzz conocido");
    let tmp = tempfile::tempdir().expect("directorio temporal del fuzzing");
    let hash = bundle::hex(&Sha256::digest(bytes));
    let _ = format.convert(&cfg, bytes, &format!("fuzz.{extension}"), &hash, tmp.path());
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
    async fn test_app(root: &FsPath) -> App {
        let mut a = app(root.to_path_buf()).await.unwrap();
        Arc::make_mut(&mut a.cfg).retry_base_s = 0;
        a
    }
    async fn job(a: &App, id: &str) -> (String, i64, Option<String>, Option<String>) {
        sqlx::query("SELECT status,attempt,error,bundle FROM jobs WHERE id=$1")
            .bind(id)
            .fetch_one(&a.db)
            .await
            .and_then(|r| Ok((r.try_get(0)?, r.try_get(1)?, r.try_get(2)?, r.try_get(3)?)))
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
        sqlx::query(
            "INSERT INTO jobs(id,filename,original,sha256,status,attempt,lease_until,\
            created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(id)
        .bind(filename)
        .bind(&rel)
        .bind(hex(&Sha256::digest(bytes)))
        .bind(status)
        .bind(attempt)
        .bind(lease)
        .bind(now())
        .execute(&a.db)
        .await
        .unwrap();
    }
    fn example(name: &str) -> Vec<u8> {
        fs::read(format!("examples/pptx/{name}.pptx")).unwrap()
    }
    #[tokio::test]
    async fn pptx_end_to_end_complete_partial_and_durable_recovery() {
        let tmp = tempfile::tempdir().unwrap();
        let a = test_app(tmp.path()).await;
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
        let completed: String = sqlx::query("SELECT status FROM jobs WHERE id=$1")
            .bind(&accepted.job_id)
            .fetch_one(&a.db)
            .await
            .and_then(|r| r.try_get(0))
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
        let published = fs::read(&zip_path).unwrap();
        assert_eq!(
            download_response.headers()[header::CONTENT_LENGTH],
            published.len().to_string().as_str()
        );
        let downloaded = http_body_util::BodyExt::collect(download_response.into_body())
            .await
            .unwrap()
            .to_bytes();
        assert_eq!(downloaded.as_ref(), published.as_slice());
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
        let (state, attempt): (String, i64) =
            sqlx::query("SELECT status,attempt FROM jobs WHERE id='multi'")
                .fetch_one(&a.db)
                .await
                .and_then(|r| Ok((r.try_get(0)?, r.try_get(1)?)))
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
        let state: String = sqlx::query("SELECT status FROM jobs WHERE id='visual'")
            .fetch_one(&a.db)
            .await
            .and_then(|r| r.try_get(0))
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
        let (state, attempt): (String, i64) =
            sqlx::query("SELECT status,attempt FROM jobs WHERE id='recovery'")
                .fetch_one(&a.db)
                .await
                .and_then(|r| Ok((r.try_get(0)?, r.try_get(1)?)))
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
        let a = test_app(tmp.path()).await;
        enqueue(&a, "broken", "broken.pptx", b"not a pptx", "queued", 0, 0).await;
        assert!(work_one(&a).await.unwrap());
        let (state, bundle): (String, Option<String>) =
            sqlx::query("SELECT status,bundle FROM jobs WHERE id='broken'")
                .fetch_one(&a.db)
                .await
                .and_then(|r| Ok((r.try_get(0)?, r.try_get(1)?)))
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
        .path;
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
        let mut a = test_app(tmp.path()).await;
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
        let a = test_app(tmp.path()).await;
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
        let lease: i64 = sqlx::query("SELECT lease_until FROM jobs WHERE id='owned'")
            .fetch_one(&a.db)
            .await
            .and_then(|r| r.try_get(0))
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
        sqlx::query("UPDATE jobs SET status='done-elsewhere' WHERE id='owned'")
            .execute(&a.db)
            .await
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
        .path
    }
    #[tokio::test]
    async fn startup_reconciliation_repairs_interrupted_publications() {
        let tmp = tempfile::tempdir().unwrap();
        let a = test_app(tmp.path()).await;
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
        sqlx::query("UPDATE jobs SET bundle='published/lost/bundle.zip' WHERE id='lost'")
            .execute(&a.db)
            .await
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
        let a = test_app(tmp.path()).await;
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
        let a = test_app(tmp.path()).await;
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
        let a = test_app(tmp.path()).await;
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
        let mut a = test_app(tmp.path()).await;
        let simple = example("simple");
        // Valores por omisión: sin límite de cola ni de espacio libre.
        assert_eq!(post(&a, "simple.pptx", &simple).await, StatusCode::ACCEPTED);
        assert_eq!(post(&a, "simple.pptx", &simple).await, StatusCode::ACCEPTED);
        assert_eq!(post(&a, "vacio.pptx", &[]).await, StatusCode::BAD_REQUEST);
        Arc::make_mut(&mut a.cfg).max_upload_mb = 1;
        let exact = vec![0u8; 1024 * 1024];
        assert_eq!(post(&a, "exacto.pptx", &exact).await, StatusCode::ACCEPTED);
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
        Arc::make_mut(&mut a.cfg).max_queued = 3;
        assert_eq!(
            post(&a, "simple.pptx", &simple).await,
            StatusCode::TOO_MANY_REQUESTS
        );
        Arc::make_mut(&mut a.cfg).max_queued = 4;
        assert_eq!(post(&a, "simple.pptx", &simple).await, StatusCode::ACCEPTED);
        Arc::make_mut(&mut a.cfg).max_queued = 0;
        Arc::make_mut(&mut a.cfg).min_free_mb = u64::MAX;
        assert_eq!(
            post(&a, "simple.pptx", &simple).await,
            StatusCode::INSUFFICIENT_STORAGE
        );
        let pending: i64 = sqlx::query("SELECT COUNT(*) FROM jobs")
            .fetch_one(&a.db)
            .await
            .and_then(|r| r.try_get(0))
            .unwrap();
        assert_eq!(pending, 4);
        // Los rechazos no dejan originales a medio escribir.
        let originals = fs::read_dir(a.root.join("originals")).unwrap().count();
        assert_eq!(originals, 4);
        let exact_hash: String =
            sqlx::query_scalar("SELECT sha256 FROM jobs WHERE filename='exacto.pptx'")
                .fetch_one(&a.db)
                .await
                .unwrap();
        assert_eq!(exact_hash, bundle::hex(&Sha256::digest(&exact)));
    }
}

#[cfg(test)]
mod version_tests {
    use super::*;
    #[tokio::test]
    async fn job_versions_survive_publication_recovery() {
        let tmp = tempfile::tempdir().unwrap();
        let a = app(tmp.path().to_owned()).await.unwrap();
        fs::copy(
            "examples/docx/breve.docx",
            tmp.path().join("originals/v.docx"),
        )
        .unwrap();
        sqlx::query(
            "INSERT INTO jobs(id,filename,original,sha256,status,created_at) VALUES('v',\
            'v.docx','originals/v.docx','-','queued',0)",
        )
        .execute(&a.db)
        .await
        .unwrap();
        worker::work_one(&a).await.unwrap();
        let state = status(State(a.clone()), Path("v".into())).await.unwrap().0;
        assert_eq!(state.versions, version::Versions::current());
        sqlx::query(
            "UPDATE jobs SET status='processing',lease_until=0,converter_version=NULL,\
            converter_commit=NULL,profile_version=NULL",
        )
        .execute(&a.db)
        .await
        .unwrap();
        worker::reconcile(&a).await.unwrap();
        let state = status(State(a), Path("v".into())).await.unwrap().0;
        assert_eq!(state.status, "complete");
        assert_eq!(state.versions, version::Versions::current());
    }
}
