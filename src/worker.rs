//! Cola durable de conversiones. Cada conversión corre en un proceso hijo vigilado
//! (memoria, tiempo), con una reserva que se renueva mientras dura; los fallos se
//! clasifican en permanentes y transitorios, y al arrancar se reconcilia el estado de
//! la base con lo que hay en disco.
use crate::App;
use crate::bundle::{frontmatter, validate_bundle};
use crate::now;
use crate::output::Stages;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use std::{
    fmt, fs,
    io::{ErrorKind, Read},
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant},
};
use tokio::io::AsyncReadExt;
use tracing::{info, warn};
use yaml_serde::Value as YamlValue;

/// Límites y plazos del worker; cada uno se ajusta con la variable de entorno indicada.
#[derive(Clone, Debug)]
pub struct Config {
    /// Publicar Swagger UI y OpenAPI en la API (`OKF_SWAGGER_ENABLED`).
    pub swagger_enabled: bool,
    /// Memoria residente máxima del proceso de conversión, en MB (`OKF_MAX_MEMORY_MB`).
    pub max_memory_mb: u64,
    pub max_output_mb: u64,
    /// Descriptores abiertos máximos del conversor (RLIMIT_NOFILE); fijo, 256.
    pub max_open_files: u64,
    /// Plazo de gracia del worker tras SIGTERM/SIGINT (`OKF_SHUTDOWN_GRACE_S`).
    pub shutdown_grace: Duration,
    /// Antigüedad máxima aceptable del latido, en segundos; fija, 30. API y workers
    /// deben coincidir, por eso no se configura.
    pub worker_health_s: i64,
    pub retention_days: u64,
    /// Diapositivas máximas por presentación (`OKF_MAX_SLIDES`).
    pub max_slides: usize,
    /// Bloques máximos (párrafos y tablas de primer nivel) por documento DOCX
    /// (`OKF_MAX_BLOCKS`).
    pub max_blocks: usize,
    /// Celdas con contenido máximas por libro XLSX (`OKF_MAX_CELLS`).
    pub max_cells: usize,
    /// Páginas máximas por documento PDF (`OKF_MAX_PAGES`).
    pub max_pages: usize,
    /// Duración máxima de una conversión (`OKF_CONVERSION_TIMEOUT_S`).
    pub timeout: Duration,
    /// Vigencia de la reserva en segundos; se renueva cada tercio. Fija, 60.
    pub lease_s: i64,
    /// Intentos máximos de un trabajo con fallos transitorios (`OKF_MAX_ATTEMPTS`).
    pub max_attempts: u32,
    /// Espera antes del primer reintento, en segundos; se duplica en cada intento.
    /// Fija, 30; las pruebas la ponen a 0.
    pub retry_base_s: i64,
    /// Tamaño máximo de un archivo subido, en MiB (`OKF_MAX_UPLOAD_MB`).
    pub max_upload_mb: u64,
    /// Trabajos sin terminar (`queued` o `processing`) a partir de los cuales la API
    /// responde `429`; 0 no limita (`OKF_MAX_QUEUED`).
    pub max_queued: u64,
    /// Espacio libre mínimo del volumen de datos, en MiB, por debajo del cual la API
    /// responde `507`; 0 no lo comprueba (`OKF_MIN_FREE_MB`).
    pub min_free_mb: u64,
    /// Límites de expansión de los paquetes OOXML (`OKF_MAX_ZIP_ENTRY_MB`,
    /// `OKF_MAX_ZIP_TOTAL_MB`, `OKF_MAX_ZIP_ENTRIES`).
    pub zip: crate::ooxml::ZipLimits,
    /// Píxeles máximos de una imagen que el conversor decodifica (`OKF_MAX_IMAGE_PIXELS`).
    pub max_image_pixels: u64,
    /// OCR local de las páginas PDF escaneadas (`OKF_OCR_*`).
    pub ocr: OcrSettings,
}

/// OCR local de las páginas PDF escaneadas; desactivado por defecto.
#[derive(Clone, Debug)]
pub struct OcrSettings {
    /// Aplicar OCR a las páginas escaneadas sin capa de texto (`OKF_OCR_ENABLED`).
    pub enabled: bool,
    /// Directorio con `det.onnx`, `rec.onnx` y `dict.txt` (`OKF_OCR_MODELS_DIR`).
    pub models_dir: PathBuf,
    /// Páginas máximas con OCR por documento; las demás quedan como vista
    /// (`OKF_OCR_MAX_PAGES`).
    pub max_pages: usize,
    /// Memoria, en MB, que se suma a `OKF_MAX_MEMORY_MB` en la conversión de un PDF
    /// cuando el OCR está activo (`OKF_OCR_MEMORY_MB`).
    pub memory_mb: u64,
}
impl Default for OcrSettings {
    fn default() -> Self {
        OcrSettings {
            enabled: false,
            models_dir: PathBuf::from("/usr/local/share/okf/ocr"),
            max_pages: 40,
            memory_mb: 256,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            swagger_enabled: true,
            max_memory_mb: 500,
            max_output_mb: 512,
            max_open_files: 256,
            shutdown_grace: Duration::from_secs(30),
            worker_health_s: 30,
            retention_days: 0,
            max_slides: 300,
            max_blocks: 50_000,
            max_cells: 1_000_000,
            max_pages: 2_000,
            timeout: Duration::from_secs(300),
            lease_s: 60,
            max_attempts: 3,
            retry_base_s: 30,
            max_upload_mb: 100,
            max_queued: 0,
            min_free_mb: 0,
            zip: crate::ooxml::ZipLimits::default(),
            max_image_pixels: 33_554_432,
            ocr: OcrSettings::default(),
        }
    }
}
impl Config {
    pub fn from_env() -> Result<Self> {
        fn var<T: std::str::FromStr>(name: &str, default: T) -> Result<T> {
            match std::env::var(name) {
                Ok(v) => v
                    .trim()
                    .parse()
                    .map_err(|_| anyhow::anyhow!("{name} no es un número válido: {v}")),
                Err(_) => Ok(default),
            }
        }
        let d = Config::default();
        let c = Config {
            swagger_enabled: var("OKF_SWAGGER_ENABLED", d.swagger_enabled)
                .map_err(|_| anyhow::anyhow!("OKF_SWAGGER_ENABLED debe ser true o false"))?,
            retention_days: var("OKF_RETENTION_DAYS", d.retention_days)?,
            max_memory_mb: var("OKF_MAX_MEMORY_MB", d.max_memory_mb)?,
            max_output_mb: var("OKF_MAX_OUTPUT_MB", d.max_output_mb)?,
            shutdown_grace: Duration::from_secs(var(
                "OKF_SHUTDOWN_GRACE_S",
                d.shutdown_grace.as_secs(),
            )?),
            max_slides: var("OKF_MAX_SLIDES", d.max_slides)?,
            max_blocks: var("OKF_MAX_BLOCKS", d.max_blocks)?,
            max_cells: var("OKF_MAX_CELLS", d.max_cells)?,
            max_pages: var("OKF_MAX_PAGES", d.max_pages)?,
            timeout: Duration::from_secs(var("OKF_CONVERSION_TIMEOUT_S", d.timeout.as_secs())?),
            max_attempts: var("OKF_MAX_ATTEMPTS", d.max_attempts)?,
            max_upload_mb: var("OKF_MAX_UPLOAD_MB", d.max_upload_mb)?,
            max_queued: var("OKF_MAX_QUEUED", d.max_queued)?,
            min_free_mb: var("OKF_MIN_FREE_MB", d.min_free_mb)?,
            zip: crate::ooxml::ZipLimits {
                entry_mb: var("OKF_MAX_ZIP_ENTRY_MB", d.zip.entry_mb)?,
                total_mb: var("OKF_MAX_ZIP_TOTAL_MB", d.zip.total_mb)?,
                entries: var("OKF_MAX_ZIP_ENTRIES", d.zip.entries)?,
            },
            max_image_pixels: var("OKF_MAX_IMAGE_PIXELS", d.max_image_pixels)?,
            ocr: OcrSettings {
                enabled: var("OKF_OCR_ENABLED", d.ocr.enabled)
                    .map_err(|_| anyhow::anyhow!("OKF_OCR_ENABLED debe ser true o false"))?,
                models_dir: std::env::var_os("OKF_OCR_MODELS_DIR")
                    .map(PathBuf::from)
                    .unwrap_or(d.ocr.models_dir.clone()),
                max_pages: var("OKF_OCR_MAX_PAGES", d.ocr.max_pages)?,
                memory_mb: var("OKF_OCR_MEMORY_MB", d.ocr.memory_mb)?,
            },
            // Plazos internos y descriptores: fijos (arquitectura técnica, §5).
            ..d
        };
        if c.max_output_mb == 0 {
            bail!("OKF_MAX_OUTPUT_MB debe ser positivo")
        }
        if c.max_attempts == 0 {
            bail!("OKF_MAX_ATTEMPTS debe ser al menos 1")
        }
        if c.max_upload_mb == 0 || c.zip.entry_mb == 0 || c.zip.total_mb == 0 || c.zip.entries == 0
        {
            bail!("OKF_MAX_UPLOAD_MB y los límites OKF_MAX_ZIP_* deben ser mayores que 0")
        }
        if c.ocr.enabled && c.ocr.max_pages == 0 {
            bail!(
                "OKF_OCR_MAX_PAGES debe ser mayor que 0; para no aplicar OCR, OKF_OCR_ENABLED=false"
            )
        }
        Ok(c)
    }
    /// Memoria máxima, en MB, del proceso que convierte `filename`: la conversión de un
    /// PDF con OCR activo suma el presupuesto propio del OCR.
    pub(crate) fn memory_limit_mb(&self, filename: &str) -> u64 {
        let pdf =
            crate::format::Format::from_filename(filename) == Some(crate::format::Format::Pdf);
        if pdf && self.ocr.enabled {
            self.max_memory_mb.saturating_add(self.ocr.memory_mb)
        } else {
            self.max_memory_mb
        }
    }
    /// Tamaño máximo de un archivo subido, en bytes.
    pub fn max_upload_bytes(&self) -> usize {
        usize::try_from(self.max_upload_mb.saturating_mul(1024 * 1024)).unwrap_or(usize::MAX)
    }
    /// Espera antes del siguiente intento, con tope de una hora.
    fn backoff(&self, attempt: u32) -> i64 {
        let factor = 1i64 << attempt.saturating_sub(1).min(16);
        self.retry_base_s.saturating_mul(factor).min(3600)
    }
}

/// Un fallo es permanente si reintentar no cambiará el resultado (archivo inválido,
/// límite superado); es transitorio si depende del entorno (disco, proceso caído).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FailureKind {
    Permanent,
    Transient,
}
/// Rechazo explícito de la entrada: siempre permanente.
#[derive(Debug)]
pub struct Rejected(pub String);
impl fmt::Display for Rejected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Rejected {}

pub fn classify(e: &anyhow::Error) -> FailureKind {
    if e.chain().any(|c| c.is::<Rejected>()) {
        return FailureKind::Permanent;
    }
    // Un ZIP truncado o con datos inválidos no mejora al reintentar; un error del
    // sistema de archivos (disco lleno, permisos, archivo ausente) puede desaparecer.
    match e
        .chain()
        .find_map(|c| c.downcast_ref::<std::io::Error>())
        .map(std::io::Error::kind)
    {
        Some(ErrorKind::InvalidData | ErrorKind::UnexpectedEof | ErrorKind::InvalidInput)
        | None => FailureKind::Permanent,
        Some(_) => FailureKind::Transient,
    }
}

/// Resultado de una conversión correcta: estado del informe, ruta del bundle y duración
/// de sus etapas. El proceso hijo lo entrega como una línea JSON.
#[derive(Debug, Serialize, Deserialize)]
pub struct Converted {
    pub status: String,
    pub bundle: PathBuf,
    pub stages: Stages,
    #[serde(default)]
    pub(crate) versions: crate::version::Versions,
    /// `conversion_id` del bundle (plan 3, I1).
    #[serde(default)]
    pub(crate) conversion_id: Option<String>,
    /// Memoria residente máxima del proceso que convirtió, en MB, si el sistema la da
    /// (`getrusage` en Linux y macOS). El muestreo no ve los picos de menos de 100 ms.
    #[serde(default)]
    pub peak_mb: Option<u64>,
}

/// Convierte un original en el directorio del intento. Es el trabajo del proceso hijo.
pub fn convert(cfg: &Config, original: &Path, filename: &str, dir: &Path) -> Result<Converted> {
    let input = fs::File::open(original)
        .with_context(|| format!("no se pudo abrir el original {}", original.display()))?;
    convert_file(cfg, input, filename, dir)
}

fn convert_file(cfg: &Config, input: fs::File, filename: &str, dir: &Path) -> Result<Converted> {
    let format = crate::format::require(filename)?;
    let mut bytes = Vec::new();
    input
        .take((cfg.max_upload_bytes() as u64).saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() > cfg.max_upload_bytes() {
        return Err(Rejected("el original supera OKF_MAX_UPLOAD_MB".into()).into());
    }
    let hash = crate::bundle::hex(&Sha256::digest(&bytes));
    let built = format.convert(cfg, &bytes, filename, &hash, dir)?;
    Ok(Converted {
        status: built.status,
        bundle: built.path,
        stages: built.stages,
        versions: crate::version::Versions::current(),
        conversion_id: Some(built.conversion_id),
        peak_mb: own_peak_mb(),
    })
}

/// Máximo residente real incluso si la conversión termina entre dos muestras del padre.
fn own_peak_mb() -> Option<u64> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // getrusage inicializa toda la estructura cuando devuelve 0.
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } != 0 {
        return None;
    }
    let rss = u64::try_from(unsafe { usage.assume_init() }.ru_maxrss).ok()?;
    #[cfg(target_os = "macos")]
    let divisor = 1024 * 1024; // Darwin da bytes; Linux da KiB.
    #[cfg(not(target_os = "macos"))]
    let divisor = 1024;
    Some(rss.div_ceil(divisor))
}

/// Códigos de salida del proceso hijo para los fallos clasificados.
pub const EXIT_PERMANENT: i32 = 3;
pub const EXIT_TRANSIENT: i32 = 4;
pub const EXIT_PANIC: i32 = 5;

/// Solo para el proceso conversor: un defecto determinista no se reintenta.
pub(crate) fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        eprintln!("defecto del conversor: {info}");
        std::process::exit(EXIT_PANIC);
    }));
}

/// Subcomando interno `convert`: escribe el resultado como una línea JSON en la salida
/// estándar o termina con el código del tipo de fallo y el mensaje en la salida de error.
pub fn convert_command(original: &Path, dir: &Path, filename: &str) -> Result<()> {
    install_panic_hook();
    let cfg = Config::from_env()?;
    let result = crate::isolation::apply(&cfg, original, dir)
        .and_then(|input| convert_file(&cfg, input, filename, dir));
    match result {
        Ok(c) => {
            println!("{}", serde_json::to_string(&c)?);
            Ok(())
        }
        Err(e) => {
            eprintln!("{e:#}");
            std::process::exit(match classify(&e) {
                FailureKind::Permanent => EXIT_PERMANENT,
                FailureKind::Transient => EXIT_TRANSIENT,
            })
        }
    }
}

struct Job {
    id: String,
    filename: String,
    original: String,
    attempt: u32,
}
#[derive(Debug)]
enum Outcome {
    Done(Converted),
    Failed(FailureKind, String),
    /// Otro worker tomó el trabajo porque la reserva venció; este intento se abandona.
    LeaseLost,
    Shutdown,
}

/// Memoria residente de un proceso en MB: `/proc` en Linux, `ps` en otros sistemas.
async fn rss_mb(pid: u32) -> Option<u64> {
    if let Ok(status) = fs::read_to_string(format!("/proc/{pid}/status")) {
        return status
            .lines()
            .find_map(|l| l.strip_prefix("VmRSS:"))
            .and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<u64>().ok())
            .map(|kb| kb / 1024);
    }
    let out = tokio::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .await
        .ok()?;
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<u64>()
        .ok()
        .map(|kb| kb / 1024)
}

/// Prolonga la reserva si el trabajo sigue siendo de este intento.
pub(crate) async fn renew(a: &App, id: &str, attempt: u32) -> Result<bool> {
    let n = sqlx::query(
        "UPDATE jobs SET lease_until=$3 WHERE id=$1 AND attempt=$2 AND status='processing'",
    )
    .bind(id)
    .bind(i64::from(attempt))
    .bind(now() + a.cfg.lease_s)
    .execute(&a.db)
    .await?
    .rows_affected();
    Ok(n == 1)
}

/// Vigilancia periódica común: plazo, memoria y renovación de la reserva.
struct Watch<'a> {
    a: &'a App,
    job: &'a Job,
    started: Instant,
    renewed: Instant,
    peak_mb: u64,
}

/// Recursos que usó un intento: memoria residente máxima (solo en el proceso hijo) y
/// duración total.
#[derive(Clone, Copy, Debug, Default)]
struct Usage {
    peak_mb: u64,
    total_ms: u64,
}

impl Watch<'_> {
    fn usage(&self) -> Usage {
        Usage {
            peak_mb: self.peak_mb,
            total_ms: u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX),
        }
    }
    async fn check(&mut self, pid: Option<u32>) -> Result<Option<Outcome>> {
        let cfg = &self.a.cfg;
        if self
            .a
            .shutdown
            .elapsed()
            .is_some_and(|elapsed| elapsed >= cfg.shutdown_grace)
        {
            return Ok(Some(Outcome::Shutdown));
        }
        if let Some(pid) = pid
            && let Some(mb) = rss_mb(pid).await
        {
            self.peak_mb = self.peak_mb.max(mb);
            let limit = cfg.memory_limit_mb(&self.job.filename);
            if mb > limit {
                return Ok(Some(Outcome::Failed(
                    FailureKind::Permanent,
                    format!("la conversión superó el límite de memoria de {limit} MB ({mb} MB)"),
                )));
            }
        }
        if self.started.elapsed() > cfg.timeout {
            return Ok(Some(Outcome::Failed(
                FailureKind::Permanent,
                format!(
                    "la conversión superó el tiempo máximo de {} s",
                    cfg.timeout.as_secs()
                ),
            )));
        }
        if self.renewed.elapsed() >= Duration::from_secs((cfg.lease_s / 3) as u64) {
            if !renew(self.a, &self.job.id, self.job.attempt).await? {
                return Ok(Some(Outcome::LeaseLost));
            }
            self.renewed = Instant::now();
        }
        Ok(None)
    }
}

const TICK: Duration = Duration::from_millis(100);

/// Ejecuta la conversión en un proceso hijo y lo termina si supera la memoria o el
/// plazo, o si la reserva pasa a otro worker.
async fn run_child(
    a: &App,
    exe: &Path,
    job: &Job,
    original: &Path,
    dir: &Path,
    usage: &mut Usage,
) -> Result<Outcome> {
    let mut command = tokio::process::Command::new(exe);
    command.env_clear();
    for key in ["PATH", "TMPDIR", "TMP", "TEMP", "OKF_FONT_PATH", "RUST_LOG"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    let mut child = command
        .arg("convert")
        .arg(original)
        .arg(dir)
        .arg(&job.filename)
        .env(
            "OKF_MAX_MEMORY_MB",
            a.cfg.memory_limit_mb(&job.filename).to_string(),
        )
        .env("OKF_MAX_OUTPUT_MB", a.cfg.max_output_mb.to_string())
        .env(
            "OKF_CONVERSION_TIMEOUT_S",
            a.cfg.timeout.as_secs().to_string(),
        )
        .env("OKF_MAX_SLIDES", a.cfg.max_slides.to_string())
        .env("OKF_MAX_BLOCKS", a.cfg.max_blocks.to_string())
        .env("OKF_MAX_CELLS", a.cfg.max_cells.to_string())
        .env("OKF_MAX_PAGES", a.cfg.max_pages.to_string())
        .env("OKF_MAX_ZIP_ENTRY_MB", a.cfg.zip.entry_mb.to_string())
        .env("OKF_MAX_ZIP_TOTAL_MB", a.cfg.zip.total_mb.to_string())
        .env("OKF_MAX_ZIP_ENTRIES", a.cfg.zip.entries.to_string())
        .env("OKF_MAX_IMAGE_PIXELS", a.cfg.max_image_pixels.to_string())
        .env("OKF_OCR_ENABLED", a.cfg.ocr.enabled.to_string())
        .env("OKF_OCR_MODELS_DIR", &a.cfg.ocr.models_dir)
        .env("OKF_OCR_MAX_PAGES", a.cfg.ocr.max_pages.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("no se pudo iniciar el proceso de conversión")?;
    let read = |mut r: Box<dyn tokio::io::AsyncRead + Unpin + Send>| {
        tokio::spawn(async move {
            let mut buf = Vec::new();
            let _ = r.read_to_end(&mut buf).await;
            String::from_utf8_lossy(&buf).into_owned()
        })
    };
    let stdout = read(Box::new(
        child.stdout.take().context("sin salida estándar")?,
    ));
    let stderr = read(Box::new(
        child.stderr.take().context("sin salida de error")?,
    ));
    let mut watch = Watch {
        a,
        job,
        started: Instant::now(),
        renewed: Instant::now(),
        peak_mb: 0,
    };
    let mut tick = tokio::time::interval(TICK);
    let status = loop {
        let exited = tokio::select! {
            s = child.wait() => Some(s?),
            _ = tick.tick() => None,
        };
        if let Some(status) = exited {
            break status;
        }
        if let Some(outcome) = watch.check(child.id()).await? {
            *usage = watch.usage();
            child.kill().await?;
            return Ok(outcome);
        }
    };
    *usage = watch.usage();
    let (out, err) = (stdout.await?, stderr.await?);
    let message = err.trim().to_string();
    if let Some(message) = crate::isolation::limit_failure(status, &message, &a.cfg) {
        return Ok(Outcome::Failed(FailureKind::Permanent, message));
    }
    Ok(match status.code() {
        Some(0) => {
            let line = out.lines().last().unwrap_or_default();
            let converted: Converted = serde_json::from_str(line)
                .context("respuesta del proceso de conversión sin formato esperado")?;
            usage.peak_mb = usage.peak_mb.max(converted.peak_mb.unwrap_or(0));
            // La salida del hijo no es de confianza: publicar copia esta ruta con los
            // permisos del supervisor, que puede leer más que el hijo aislado.
            if converted.bundle != dir.join("bundle.zip") {
                Outcome::Failed(
                    FailureKind::Permanent,
                    format!(
                        "el proceso de conversión indicó un bundle fuera de su intento: {}",
                        converted.bundle.display()
                    ),
                )
            } else if converted
                .peak_mb
                .is_some_and(|peak| peak > a.cfg.memory_limit_mb(&job.filename))
            {
                Outcome::Failed(
                    FailureKind::Permanent,
                    format!(
                        "la conversión superó el límite de memoria de {} MB (máximo {} MB)",
                        a.cfg.memory_limit_mb(&job.filename),
                        converted.peak_mb.unwrap()
                    ),
                )
            } else if watch.started.elapsed() > a.cfg.timeout {
                Outcome::Failed(
                    FailureKind::Permanent,
                    format!(
                        "la conversión superó el tiempo máximo de {} s",
                        a.cfg.timeout.as_secs()
                    ),
                )
            } else {
                Outcome::Done(converted)
            }
        }
        Some(EXIT_PERMANENT) => Outcome::Failed(FailureKind::Permanent, message),
        Some(EXIT_TRANSIENT) => Outcome::Failed(FailureKind::Transient, message),
        Some(EXIT_PANIC) => Outcome::Failed(
            FailureKind::Permanent,
            format!("defecto del conversor (pánico): {message}"),
        ),
        // Señal o código desconocido: se reintenta hasta el máximo de intentos.
        _ => Outcome::Failed(
            FailureKind::Transient,
            format!("el proceso de conversión terminó de forma anormal ({status}): {message}"),
        ),
    })
}

/// Conversión en un hilo del propio proceso, para las pruebas unitarias: renueva la
/// reserva y respeta el plazo, pero no puede medir ni limitar la memoria.
async fn run_local(
    a: &App,
    job: &Job,
    original: &Path,
    dir: &Path,
    usage: &mut Usage,
) -> Result<Outcome> {
    let (cfg, original, filename, dir) = (
        (*a.cfg).clone(),
        original.to_path_buf(),
        job.filename.clone(),
        dir.to_path_buf(),
    );
    let mut task = tokio::task::spawn_blocking(move || convert(&cfg, &original, &filename, &dir));
    let mut watch = Watch {
        a,
        job,
        started: Instant::now(),
        renewed: Instant::now(),
        peak_mb: 0,
    };
    let mut tick = tokio::time::interval(TICK);
    loop {
        let finished = tokio::select! {
            r = &mut task => Some(r?),
            _ = tick.tick() => None,
        };
        if let Some(result) = finished {
            *usage = watch.usage();
            return Ok(match result {
                Ok(c) => Outcome::Done(c),
                Err(e) => Outcome::Failed(classify(&e), format!("{e:#}")),
            });
        }
        if let Some(outcome) = watch.check(None).await? {
            *usage = watch.usage();
            return Ok(outcome);
        }
    }
}

/// Estado, identidad del conversor e identificador de conversión de un bundle publicado.
struct Published {
    state: String,
    versions: crate::version::Versions,
    conversion_id: Option<String>,
}

/// Estado del bundle publicado de un trabajo, si existe y es válido. Un bundle
/// inválido (publicación interrumpida) se elimina para que el trabajo se reprocese.
fn published_state(root: &Path, id: &str) -> Option<Published> {
    let dir = root.join("published").join(id);
    let zip = dir.join("bundle.zip");
    if !zip.exists() {
        return None;
    }
    let state = (|| -> Result<Published> {
        validate_bundle(&zip)?;
        let mut z = zip::ZipArchive::new(fs::File::open(&zip)?)?;
        let mut report = String::new();
        z.by_name("conversion-report.md")?
            .read_to_string(&mut report)?;
        let fm = frontmatter(&report)?;
        let state = fm
            .get("status")
            .and_then(YamlValue::as_str)
            .map(str::to_string)
            .context("informe sin estado")?;
        if !matches!(state.as_str(), "complete" | "partial") {
            bail!("estado de informe desconocido: {state}")
        }
        Ok(Published {
            state,
            versions: crate::version::Versions::from_report(&fm),
            conversion_id: fm
                .get("conversion_id")
                .and_then(YamlValue::as_str)
                .map(str::to_owned),
        })
    })();
    match state {
        Ok(s) => Some(s),
        Err(e) => {
            warn!(%id, error = %e, "bundle publicado inválido; se elimina");
            let _ = fs::remove_dir_all(&dir);
            None
        }
    }
}

fn remove_attempts(root: &Path, id: &str) {
    let dir = root.join("attempts").join(id);
    if dir.exists()
        && let Err(e) = fs::remove_dir_all(&dir)
    {
        warn!(%id, error = %e, "no se pudieron eliminar los intentos");
    }
}

/// Trabajos cuya reserva venció: el worker murió o quedó colgado. Si llegó a publicar,
/// se completa el registro; si no, se reintenta con espera o se marca fallido al
/// agotar los intentos.
async fn recover_expired(
    tx: &mut Transaction<'_, Postgres>,
    root: &Path,
    cfg: &Config,
) -> Result<()> {
    let expired: Vec<(String, i64)> = sqlx::query_as(
        "SELECT id,attempt FROM jobs WHERE status='processing' AND lease_until < $1 FOR \
        UPDATE SKIP LOCKED",
    )
    .bind(now())
    .fetch_all(&mut **tx)
    .await?;
    for (id, attempt) in expired {
        if let Some(Published {
            state,
            versions,
            conversion_id,
        }) = published_state(root, &id)
        {
            sqlx::query(
                "UPDATE jobs SET status=$2,bundle=$3,lease_until=NULL,retry_at=NULL,error=NULL,\
                converter_version=$4,converter_commit=$5,profile_version=$6,conversion_id=$8,\
                finished_at=COALESCE(finished_at,$7) WHERE id=$1",
            )
            .bind(&id)
            .bind(&state)
            .bind(format!("published/{id}/bundle.zip"))
            .bind(&versions.converter_version)
            .bind(&versions.converter_commit)
            .bind(&versions.profile_version)
            .bind(now())
            .bind(&conversion_id)
            .execute(&mut **tx)
            .await?;
            remove_attempts(root, &id);
            info!(%id, %state, "publicación interrumpida completada al reconciliar");
        } else if attempt >= i64::from(cfg.max_attempts) {
            sqlx::query(
                "UPDATE jobs SET status='failed',lease_until=NULL,retry_at=NULL,error=$2,\
                finished_at=$3 WHERE id=$1",
            )
            .bind(&id)
            .bind(format!(
                "el proceso de conversión se interrumpió en {attempt} intentos"
            ))
            .bind(now())
            .execute(&mut **tx)
            .await?;
            remove_attempts(root, &id);
        } else {
            sqlx::query(
                "UPDATE jobs SET status='queued',lease_until=NULL,retry_at=$2,\
                error='worker anterior interrumpido; se reprocesará el original' WHERE id=$1",
            )
            .bind(&id)
            .bind(now() + cfg.backoff(attempt as u32))
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}

async fn claim(a: &App) -> Result<Option<Job>> {
    let mut tx = a.db.begin().await?;
    recover_expired(&mut tx, &a.root, &a.cfg).await?;
    // Hace visibles los recuperados y libera sus filas antes de elegir uno nuevo.
    tx.commit().await?;
    let mut tx = a.db.begin().await?;
    let row = sqlx::query(
        "SELECT id,filename,original,attempt FROM jobs WHERE status='queued' AND \
        (retry_at IS NULL OR retry_at <= $1) ORDER BY created_at,id LIMIT 1 FOR UPDATE \
        SKIP LOCKED",
    )
    .bind(now())
    .fetch_optional(&mut *tx)
    .await?;
    let job = row
        .map(|r| -> Result<Job> {
            Ok(Job {
                id: r.try_get(0)?,
                filename: r.try_get(1)?,
                original: r.try_get(2)?,
                attempt: u32::try_from(r.try_get::<i64, _>(3)?)?
                    .checked_add(1)
                    .context("demasiados intentos")?,
            })
        })
        .transpose()?;
    if let Some(job) = &job {
        sqlx::query(
            "UPDATE jobs SET status='processing',attempt=$2,lease_until=$3,retry_at=NULL,\
            error=NULL,converter_version=$4,converter_commit=$5,profile_version=$6,\
            conversion_id=NULL,finished_at=NULL WHERE id=$1",
        )
        .bind(&job.id)
        .bind(i64::from(job.attempt))
        .bind(now() + a.cfg.lease_s)
        .bind(crate::version::PACKAGE)
        .bind(crate::version::COMMIT)
        .bind(crate::version::PROFILE)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(job)
}

/// Copia el bundle a `published/<id>` con un renombrado atómico y registra el estado.
async fn publish(a: &App, job: &Job, c: &Converted) -> Result<()> {
    // El bloqueo de la fila protege también el renombrado contra recuperación/retención.
    let mut tx = a.db.begin().await?;
    let owned = sqlx::query(
        "SELECT id FROM jobs WHERE id=$1 AND attempt=$2 AND status='processing' FOR UPDATE",
    )
    .bind(&job.id)
    .bind(i64::from(job.attempt))
    .fetch_optional(&mut *tx)
    .await?
    .is_some();
    if !owned {
        warn!(id = %job.id, "reserva perdida antes de publicar; se abandona el intento");
        return Ok(());
    }
    let id = &job.id;
    let pubdir = a.root.join("published").join(id);
    let staging = a.root.join("published").join(format!(".{id}.tmp"));
    if staging.exists() {
        fs::remove_dir_all(&staging)?
    }
    fs::create_dir_all(&staging)?;
    fs::copy(&c.bundle, staging.join("bundle.zip"))?;
    if pubdir.exists() {
        fs::remove_dir_all(&pubdir)?
    }
    fs::rename(&staging, &pubdir)?;
    sqlx::query(
        "UPDATE jobs SET status=$3,bundle=$4,lease_until=NULL,retry_at=NULL,error=NULL,\
        converter_version=$5,converter_commit=$6,profile_version=$7,finished_at=$8,\
        conversion_id=$9 WHERE id=$1 AND attempt=$2",
    )
    .bind(id)
    .bind(i64::from(job.attempt))
    .bind(&c.status)
    .bind(format!("published/{id}/bundle.zip"))
    .bind(&c.versions.converter_version)
    .bind(&c.versions.converter_commit)
    .bind(&c.versions.profile_version)
    .bind(now())
    .bind(&c.conversion_id)
    .execute(&mut *tx)
    .await?;
    remove_attempts(&a.root, id);
    tx.commit().await?;
    info!(%id, status = %c.status, attempt = job.attempt, "trabajo publicado");
    Ok(())
}

/// Registra un fallo: el transitorio vuelve a la cola con espera creciente mientras
/// queden intentos; el permanente, o el transitorio sin intentos, termina en `failed`.
async fn fail(a: &App, job: &Job, kind: FailureKind, message: &str) -> Result<()> {
    let mut tx = a.db.begin().await?;
    let owned = sqlx::query(
        "SELECT id FROM jobs WHERE id=$1 AND attempt=$2 AND status='processing' FOR UPDATE",
    )
    .bind(&job.id)
    .bind(i64::from(job.attempt))
    .fetch_optional(&mut *tx)
    .await?
    .is_some();
    if !owned {
        return Ok(());
    }
    let retry = kind == FailureKind::Transient && job.attempt < a.cfg.max_attempts;
    let (status, retry_at, error) = if retry {
        (
            "queued",
            Some(now() + a.cfg.backoff(job.attempt)),
            format!(
                "intento {} falló por un error transitorio; se reintentará: {message}",
                job.attempt
            ),
        )
    } else if kind == FailureKind::Transient {
        (
            "failed",
            None,
            format!("falló tras {} intentos: {message}", job.attempt),
        )
    } else {
        ("failed", None, message.to_string())
    };
    sqlx::query(
        "UPDATE jobs SET status=$3,lease_until=NULL,retry_at=$4,error=$5,\
        finished_at=CASE WHEN $3='failed' THEN $6 ELSE NULL END WHERE id=$1 AND \
        attempt=$2 AND status='processing'",
    )
    .bind(&job.id)
    .bind(i64::from(job.attempt))
    .bind(status)
    .bind(retry_at)
    .bind(&error)
    .bind(now())
    .execute(&mut *tx)
    .await?;
    if !retry {
        remove_attempts(&a.root, &job.id);
    }
    tx.commit().await?;
    warn!(id = %job.id, attempt = job.attempt, ?kind, %error, "falló conversión");
    Ok(())
}

pub async fn work_one(a: &App) -> Result<bool> {
    if a.shutdown.requested() {
        return Ok(false);
    }
    let Some(job) = claim(a).await? else {
        return Ok(false);
    };
    let dir = a
        .root
        .join("attempts")
        .join(&job.id)
        .join(job.attempt.to_string());
    let original = a.root.join(&job.original);
    let mut usage = Usage::default();
    let outcome = async {
        if dir.exists() {
            fs::remove_dir_all(&dir)?
        }
        fs::create_dir_all(&dir)?;
        match &a.converter {
            Some(exe) => run_child(a, exe, &job, &original, &dir, &mut usage).await,
            None => run_local(a, &job, &original, &dir, &mut usage).await,
        }
    }
    .await
    // Un fallo de la propia supervisión (disco, arranque del hijo) es del entorno.
    .unwrap_or_else(|e| Outcome::Failed(FailureKind::Transient, format!("{e:#}")));
    let stats = log_attempt(&job, &outcome, usage);
    if let Err(error) = crate::observability::record(a, stats).await {
        warn!(%error, "no se pudieron persistir las métricas del intento");
    }
    match outcome {
        Outcome::Done(c) => {
            if let Err(e) = publish(a, &job, &c).await {
                fail(
                    a,
                    &job,
                    FailureKind::Transient,
                    &format!("publicación: {e:#}"),
                )
                .await?
            }
        }
        Outcome::Failed(kind, message) => fail(a, &job, kind, &message).await?,
        Outcome::Shutdown => {
            // Limpia antes de hacer visible la cola: el siguiente intento puede
            // reutilizar el mismo número y directorio en otro worker.
            let mut tx = a.db.begin().await?;
            let owned = sqlx::query(
                "SELECT id FROM jobs WHERE id=$1 AND attempt=$2 AND status='processing' FOR UPDATE",
            )
            .bind(&job.id)
            .bind(i64::from(job.attempt))
            .fetch_optional(&mut *tx)
            .await?
            .is_some();
            if owned {
                if dir.exists()
                    && let Err(error) = fs::remove_dir_all(&dir)
                {
                    warn!(%error, "limpieza interrumpida pendiente para el próximo intento");
                }
                sqlx::query(
                    "UPDATE jobs SET status='queued',attempt=attempt-1,lease_until=NULL,\
                    retry_at=NULL,error=NULL WHERE id=$1",
                )
                .bind(&job.id)
                .execute(&mut *tx)
                .await?;
                info!(id = %job.id, "conversión interrumpida; vuelve a la cola sin consumir intento");
            }
            tx.commit().await?;
        }
        Outcome::LeaseLost => {
            warn!(id = %job.id, attempt = job.attempt, "reserva tomada por otro worker; se abandona el intento")
        }
    }
    Ok(true)
}

/// Una línea de registro por intento: formato, resultado, duración de cada etapa y total,
/// y memoria máxima. No incluye nada del contenido del documento.
fn log_attempt<'a>(
    job: &Job,
    outcome: &'a Outcome,
    usage: Usage,
) -> crate::observability::Attempt<'a> {
    let format = crate::format::Format::from_filename(&job.filename)
        .map_or("desconocido", |f| f.extension());
    let mut peak_mb = usage.peak_mb;
    let (status, stages) = match outcome {
        Outcome::Done(c) => {
            peak_mb = peak_mb.max(c.peak_mb.unwrap_or(0));
            (c.status.as_str(), c.stages)
        }
        Outcome::Failed(FailureKind::Permanent, _) => ("failed", Stages::default()),
        Outcome::Failed(FailureKind::Transient, _) => ("transient_failure", Stages::default()),
        Outcome::LeaseLost => ("lease_lost", Stages::default()),
        Outcome::Shutdown => ("interrupted", Stages::default()),
    };
    info!(
        job_id = %job.id,
        attempt = job.attempt,
        format,
        status,
        read_ms = stages.read_ms,
        assemble_ms = stages.assemble_ms,
        validate_ms = stages.validate_ms,
        total_ms = usage.total_ms,
        peak_mb,
        "intento terminado"
    );
    crate::observability::Attempt {
        format,
        result: status,
        stages,
        total_ms: usage.total_ms,
        peak_mb,
        retry: job.attempt > 1 && !matches!(outcome, Outcome::Shutdown | Outcome::LeaseLost),
    }
}

/// Antigüedad mínima de un original sin fila para borrarlo. La API escribe el original
/// antes de registrar el trabajo, y otra instancia puede estar recibiéndolo todavía.
const ORPHAN_ORIGINAL_GRACE: Duration = Duration::from_secs(3600);

/// Reconciliación al arrancar: elimina publicaciones a medio hacer, completa las que
/// terminaron sin registrarse, reencola los trabajos cuyo bundle desapareció y borra
/// los intentos de trabajos terminados y los originales que nunca llegaron a registrarse.
pub async fn reconcile(a: &App) -> Result<()> {
    let mut tx = a.db.begin().await?;
    recover_expired(&mut tx, &a.root, &a.cfg).await?;
    // Las filas bloqueadas impiden que publicación o retención compitan con esta limpieza.
    let rows: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT id,status,bundle FROM jobs WHERE status IN ('queued','complete',\
        'partial','failed','expired') FOR UPDATE SKIP LOCKED",
    )
    .fetch_all(&mut *tx)
    .await?;
    for (id, state, bundle) in rows {
        let staging = a.root.join("published").join(format!(".{id}.tmp"));
        if staging.exists() {
            fs::remove_dir_all(staging)?;
        }
        if matches!(state.as_str(), "complete" | "partial")
            && bundle.as_ref().is_none_or(|p| !a.root.join(p).is_file())
        {
            sqlx::query(
                "UPDATE jobs SET status='queued',bundle=NULL,finished_at=NULL,retry_at=NULL,\
                error='el bundle publicado no existe; se reprocesa el original' WHERE id=$1",
            )
            .bind(&id)
            .execute(&mut *tx)
            .await?;
            warn!(%id, "bundle publicado ausente; trabajo reencolado");
        }
        if matches!(
            state.as_str(),
            "complete" | "partial" | "failed" | "expired"
        ) {
            remove_attempts(&a.root, &id);
        }
    }
    // Directorios sin fila: el original siempre se registra antes de crear intentos/publicaciones.
    for (folder, temporary) in [("attempts", false), ("published", true)] {
        for entry in fs::read_dir(a.root.join(folder))? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let id = if temporary {
                let Some(id) = name.strip_prefix('.').and_then(|s| s.strip_suffix(".tmp")) else {
                    continue;
                };
                id
            } else {
                &name
            };
            let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM jobs WHERE id=$1)")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
            if !exists {
                fs::remove_dir_all(entry.path())?;
            }
        }
    }
    // Originales sin fila: la API cayó entre escribir el original y registrar el trabajo.
    for entry in fs::read_dir(a.root.join("originals"))? {
        let entry = entry?;
        let path = entry.path();
        let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let age = entry.metadata()?.modified()?.elapsed().unwrap_or_default();
        if age < ORPHAN_ORIGINAL_GRACE {
            continue;
        }
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM jobs WHERE id=$1)")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        if !exists {
            fs::remove_file(&path)?;
            warn!(%id, "original sin trabajo registrado eliminado");
        }
    }
    tx.commit().await?;
    Ok(())
}

pub async fn worker(root: PathBuf, poll_ms: u64, once: bool) -> Result<()> {
    let mut a = crate::app(root).await?;
    a.converter = Some(std::env::current_exe()?);
    a.shutdown = crate::shutdown::install()?;
    // El límite de memoria solo es una garantía si se puede medir.
    if rss_mb(std::process::id()).await.is_none() {
        bail!("no se puede medir la memoria de los procesos (ni /proc ni ps disponibles)")
    }
    crate::isolation::log_support();
    if a.cfg.ocr.enabled {
        crate::pdf::ocr::check_models(&a.cfg.ocr.models_dir)?;
        info!(dir = %a.cfg.ocr.models_dir.display(), "OCR local activo para las páginas PDF escaneadas");
    }
    let maintenance = crate::retention::start(&a).await?;
    reconcile(&a).await?;
    let heartbeat = crate::observability::Heartbeat::start(&a).await?;
    let result = async {
        info!(?a.cfg, "worker iniciado");
        while !a.shutdown.requested() {
            if !work_one(&a).await? {
                if once {
                    break;
                }
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(poll_ms)) => {},
                    _ = a.shutdown.wait() => {},
                }
            }
        }
        Ok(())
    }
    .await;
    heartbeat.stop().await;
    if let Some(task) = maintenance {
        task.abort();
        let _ = task.await;
    }
    result
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[tokio::test]
    async fn concurrent_claims_skip_locked_rows_and_recover_expired_jobs_once() {
        let tmp = tempfile::tempdir().unwrap();
        let a = crate::app(tmp.path().to_owned()).await.unwrap();
        let mut b = crate::app(tmp.path().to_owned()).await.unwrap();
        std::sync::Arc::make_mut(&mut b.cfg).retry_base_s = 0;
        let mut a = a;
        std::sync::Arc::make_mut(&mut a.cfg).retry_base_s = 0;
        for id in ["a_locked", "b_ready", "c_expired"] {
            sqlx::query(
                "INSERT INTO jobs(id,filename,original,sha256,status,created_at,lease_until,\
                attempt) VALUES($1,'a.docx','unused','-',CASE WHEN $1='c_expired' THEN \
                'processing' ELSE 'queued' END,0,0,0)",
            )
            .bind(id)
            .execute(&a.db)
            .await
            .unwrap();
        }
        let mut lock = a.db.begin().await.unwrap();
        sqlx::query("SELECT id FROM jobs WHERE id='a_locked' FOR UPDATE")
            .fetch_one(&mut *lock)
            .await
            .unwrap();
        let (first, second) = tokio::time::timeout(Duration::from_secs(3), async {
            tokio::join!(claim(&a), claim(&b))
        })
        .await
        .expect("la reclamación no debe esperar por la fila bloqueada");
        let mut ids = [first.unwrap().unwrap().id, second.unwrap().unwrap().id];
        ids.sort();
        assert_eq!(ids, ["b_ready", "c_expired"]);
        assert!(claim(&b).await.unwrap().is_none());
        lock.rollback().await.unwrap();
        let job = claim(&b).await.unwrap().unwrap();
        assert_eq!(job.id, "a_locked");
        assert_eq!(job.attempt, 1);
        assert!(!renew(&a, &job.id, 0).await.unwrap());
        assert!(renew(&a, &job.id, 1).await.unwrap());
    }

    // Se ejecuta en otro proceso, sin inyectar disparadores en el binario de producción.
    #[test]
    fn ocr_memory_budget_applies_only_to_pdf_with_ocr_enabled() {
        let mut cfg = Config::default();
        assert_eq!(cfg.memory_limit_mb("a.pdf"), 500);
        cfg.ocr.enabled = true;
        assert_eq!(cfg.memory_limit_mb("a.pdf"), 756);
        assert_eq!(cfg.memory_limit_mb("A.PDF"), 756);
        assert_eq!(cfg.memory_limit_mb("a.docx"), 500);
        let err = crate::pdf::ocr::check_models(std::path::Path::new("/nonexistent/okf-ocr"))
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("OKF_OCR_MODELS_DIR") && err.contains("tools/ocr_models.sh"),
            "{err}"
        );
    }

    #[test]
    fn panic_child() {
        if std::env::var_os("RUST_TEST_PANIC_CHILD").is_some() {
            install_panic_hook();
            panic!("pánico de prueba del conversor");
        }
    }

    #[tokio::test]
    async fn panic_fails_on_first_attempt_but_unknown_exit_retries() {
        let tmp = tempfile::tempdir().unwrap();
        let mut a = crate::app(tmp.path().to_path_buf()).await.unwrap();
        let script = tmp.path().join("converter.sh");
        let exe = std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .replace('\'', "'\"'\"'");
        fs::write(&script, format!("#!/bin/sh\nRUST_TEST_PANIC_CHILD=1 exec '{exe}' --exact worker::tests::panic_child --nocapture\n")).unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
        a.converter = Some(script.clone());
        for id in ["panic", "unknown"] {
            if id == "unknown" {
                fs::write(&script, "#!/bin/sh\nexit 17\n").unwrap();
            }
            sqlx::query(
                "INSERT INTO jobs(id,filename,original,sha256,status,created_at) VALUES($1,\
                'test.docx','unused','-','queued',0)",
            )
            .bind(id)
            .execute(&a.db)
            .await
            .unwrap();
            assert!(work_one(&a).await.unwrap());
            let (status, attempt, error): (String, i64, String) =
                sqlx::query("SELECT status,attempt,error FROM jobs WHERE id=$1")
                    .bind(id)
                    .fetch_one(&a.db)
                    .await
                    .and_then(|r| Ok((r.try_get(0)?, r.try_get(1)?, r.try_get(2)?)))
                    .unwrap();
            assert_eq!(attempt, 1);
            if id == "panic" {
                assert_eq!(status, "failed");
                assert!(error.contains("defecto del conversor"), "{error}");
                assert!(error.contains("pánico de prueba del conversor"), "{error}");
                assert!(error.contains("src/worker.rs"), "{error}");
                assert!(!work_one(&a).await.unwrap());
            } else {
                assert_eq!(status, "queued");
                assert!(error.contains("transitorio"));
            }
        }
    }

    #[tokio::test]
    async fn bundle_outside_the_attempt_is_not_published() {
        let tmp = tempfile::tempdir().unwrap();
        let mut a = crate::app(tmp.path().to_path_buf()).await.unwrap();
        // Un hijo comprometido señala un archivo que solo el supervisor puede leer.
        let secret = tmp.path().join("secreto.txt");
        fs::write(&secret, "OKF_DATABASE_URL=postgres://okf:clave@db/okf").unwrap();
        let script = tmp.path().join("converter.sh");
        let converted = serde_json::json!({
            "status": "complete",
            "bundle": secret,
            "stages": {"read_ms": 0, "assemble_ms": 0, "validate_ms": 0},
        });
        fs::write(
            &script,
            format!("#!/bin/sh\ncat <<'EOF'\n{converted}\nEOF\n"),
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
        a.converter = Some(script);
        sqlx::query(
            "INSERT INTO jobs(id,filename,original,sha256,status,created_at) VALUES('ajeno',\
            'test.docx','unused','-','queued',0)",
        )
        .execute(&a.db)
        .await
        .unwrap();
        assert!(work_one(&a).await.unwrap());
        let (status, attempt, error, bundle): (String, i64, String, Option<String>) =
            sqlx::query_as("SELECT status,attempt,error,bundle FROM jobs WHERE id='ajeno'")
                .fetch_one(&a.db)
                .await
                .unwrap();
        assert_eq!((status.as_str(), attempt), ("failed", 1));
        assert!(error.contains("fuera de su intento"), "{error}");
        assert_eq!(bundle, None);
        assert!(!tmp.path().join("published").join("ajeno").exists());
    }
}
