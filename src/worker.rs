//! Cola durable de conversiones. Cada conversión corre en un proceso hijo vigilado
//! (memoria, tiempo), con una reserva que se renueva mientras dura; los fallos se
//! clasifican en permanentes y transitorios, y al arrancar se reconcilia el estado de
//! la base con lo que hay en disco.
use crate::App;
use crate::bundle::{frontmatter, validate_bundle};
use crate::now;
use crate::output::Stages;
use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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
    /// Memoria residente máxima del proceso de conversión, en MB (`OKF_MAX_MEMORY_MB`).
    pub max_memory_mb: u64,
    pub max_output_mb: u64,
    pub max_open_files: u64,
    pub shutdown_grace: Duration,
    pub worker_health_s: i64,
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
    /// Vigencia de la reserva en segundos; se renueva cada tercio (`OKF_LEASE_S`).
    pub lease_s: i64,
    /// Intentos máximos de un trabajo con fallos transitorios (`OKF_MAX_ATTEMPTS`).
    pub max_attempts: u32,
    /// Espera antes del primer reintento, en segundos; se duplica en cada intento
    /// (`OKF_RETRY_BASE_S`).
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
}
impl Default for Config {
    fn default() -> Self {
        Config {
            max_memory_mb: 500,
            max_output_mb: 512,
            max_open_files: 256,
            shutdown_grace: Duration::from_secs(30),
            worker_health_s: 30,
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
            worker_health_s: var("OKF_WORKER_HEALTH_S", d.worker_health_s)?,
            max_memory_mb: var("OKF_MAX_MEMORY_MB", d.max_memory_mb)?,
            max_output_mb: var("OKF_MAX_OUTPUT_MB", d.max_output_mb)?,
            max_open_files: var("OKF_MAX_OPEN_FILES", d.max_open_files)?,
            shutdown_grace: Duration::from_secs(var(
                "OKF_SHUTDOWN_GRACE_S",
                d.shutdown_grace.as_secs(),
            )?),
            max_slides: var("OKF_MAX_SLIDES", d.max_slides)?,
            max_blocks: var("OKF_MAX_BLOCKS", d.max_blocks)?,
            max_cells: var("OKF_MAX_CELLS", d.max_cells)?,
            max_pages: var("OKF_MAX_PAGES", d.max_pages)?,
            timeout: Duration::from_secs(var("OKF_CONVERSION_TIMEOUT_S", d.timeout.as_secs())?),
            lease_s: var("OKF_LEASE_S", d.lease_s)?,
            max_attempts: var("OKF_MAX_ATTEMPTS", d.max_attempts)?,
            retry_base_s: var("OKF_RETRY_BASE_S", d.retry_base_s)?,
            max_upload_mb: var("OKF_MAX_UPLOAD_MB", d.max_upload_mb)?,
            max_queued: var("OKF_MAX_QUEUED", d.max_queued)?,
            min_free_mb: var("OKF_MIN_FREE_MB", d.min_free_mb)?,
            zip: crate::ooxml::ZipLimits {
                entry_mb: var("OKF_MAX_ZIP_ENTRY_MB", d.zip.entry_mb)?,
                total_mb: var("OKF_MAX_ZIP_TOTAL_MB", d.zip.total_mb)?,
                entries: var("OKF_MAX_ZIP_ENTRIES", d.zip.entries)?,
            },
            max_image_pixels: var("OKF_MAX_IMAGE_PIXELS", d.max_image_pixels)?,
        };
        if c.max_output_mb == 0 || c.max_open_files < 16 {
            bail!("OKF_MAX_OUTPUT_MB debe ser positivo y OKF_MAX_OPEN_FILES al menos 16")
        }
        if c.worker_health_s < 3 {
            bail!("OKF_WORKER_HEALTH_S debe ser al menos 3")
        }
        if c.lease_s < 3 || c.max_attempts == 0 {
            bail!("OKF_LEASE_S debe ser al menos 3 y OKF_MAX_ATTEMPTS al menos 1")
        }
        if c.max_upload_mb == 0 || c.zip.entry_mb == 0 || c.zip.total_mb == 0 || c.zip.entries == 0
        {
            bail!("OKF_MAX_UPLOAD_MB y los límites OKF_MAX_ZIP_* deben ser mayores que 0")
        }
        Ok(c)
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
    /// Memoria residente máxima del proceso que convirtió, en MB, si el sistema la da
    /// (`getrusage` en Linux y macOS). El muestreo no ve los picos de menos de 100 ms.
    #[serde(default)]
    pub peak_mb: Option<u64>,
}

/// Convierte un original en el directorio del intento. Es el trabajo del proceso hijo.
pub fn convert(cfg: &Config, original: &Path, filename: &str, dir: &Path) -> Result<Converted> {
    let format = crate::format::require(filename)?;
    let bytes = fs::read(original)
        .with_context(|| format!("no se pudo leer el original {}", original.display()))?;
    let hash = crate::bundle::hex(&Sha256::digest(&bytes));
    let built = format.convert(cfg, &bytes, filename, &hash, dir)?;
    Ok(Converted {
        status: built.status,
        bundle: built.path,
        stages: built.stages,
        versions: crate::version::Versions::current(),
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
        .and_then(|()| convert(&cfg, original, filename, dir));
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
    let db = a.db.lock().await;
    let n = db.execute(
        "UPDATE jobs SET lease_until=?3 WHERE id=?1 AND attempt=?2 AND status='processing'",
        params![id, attempt, now() + a.cfg.lease_s],
    )?;
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
            if mb > cfg.max_memory_mb {
                return Ok(Some(Outcome::Failed(
                    FailureKind::Permanent,
                    format!(
                        "la conversión superó el límite de memoria de {} MB ({mb} MB)",
                        cfg.max_memory_mb
                    ),
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
    let mut child = tokio::process::Command::new(exe)
        .arg("convert")
        .arg(original)
        .arg(dir)
        .arg(&job.filename)
        .env("OKF_MAX_MEMORY_MB", a.cfg.max_memory_mb.to_string())
        .env("OKF_MAX_OUTPUT_MB", a.cfg.max_output_mb.to_string())
        .env("OKF_MAX_OPEN_FILES", a.cfg.max_open_files.to_string())
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
            if converted
                .peak_mb
                .is_some_and(|peak| peak > a.cfg.max_memory_mb)
            {
                Outcome::Failed(
                    FailureKind::Permanent,
                    format!(
                        "la conversión superó el límite de memoria de {} MB (máximo {} MB)",
                        a.cfg.max_memory_mb,
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

/// Estado del bundle publicado de un trabajo, si existe y es válido. Un bundle
/// inválido (publicación interrumpida) se elimina para que el trabajo se reprocese.
fn published_state(root: &Path, id: &str) -> Option<(String, crate::version::Versions)> {
    let dir = root.join("published").join(id);
    let zip = dir.join("bundle.zip");
    if !zip.exists() {
        return None;
    }
    let state = (|| -> Result<(String, crate::version::Versions)> {
        validate_bundle(&zip)?;
        let mut z = zip::ZipArchive::new(fs::File::open(&zip)?)?;
        let mut report = String::new();
        z.by_name("conversion-report.md")?
            .read_to_string(&mut report)?;
        let state = frontmatter(&report)?
            .get("status")
            .and_then(YamlValue::as_str)
            .map(str::to_string)
            .context("informe sin estado")?;
        if !matches!(state.as_str(), "complete" | "partial") {
            bail!("estado de informe desconocido: {state}")
        }
        Ok((
            state,
            crate::version::Versions::from_report(&frontmatter(&report)?),
        ))
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
fn recover_expired(tx: &Transaction<'_>, root: &Path, cfg: &Config) -> Result<()> {
    let expired: Vec<(String, u32)> = tx
        .prepare("SELECT id,attempt FROM jobs WHERE status='processing' AND lease_until < ?1")?
        .query_map([now()], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (id, attempt) in expired {
        if let Some((state, versions)) = published_state(root, &id) {
            tx.execute(
                "UPDATE jobs SET status=?2,bundle=?3,lease_until=NULL,retry_at=NULL,error=NULL,converter_version=?4,converter_commit=?5,profile_version=?6 WHERE id=?1",
                params![id, state, format!("published/{id}/bundle.zip"), versions.converter_version, versions.converter_commit, versions.profile_version],
            )?;
            remove_attempts(root, &id);
            info!(%id, %state, "publicación interrumpida completada al reconciliar");
        } else if attempt >= cfg.max_attempts {
            tx.execute(
                "UPDATE jobs SET status='failed',lease_until=NULL,retry_at=NULL,error=?2 WHERE id=?1",
                params![id, format!("el proceso de conversión se interrumpió en {attempt} intentos")],
            )?;
            remove_attempts(root, &id);
        } else {
            tx.execute(
                "UPDATE jobs SET status='queued',lease_until=NULL,retry_at=?2,error='worker anterior interrumpido; se reprocesará el original' WHERE id=?1",
                params![id, now() + cfg.backoff(attempt)],
            )?;
        }
    }
    Ok(())
}

async fn claim(a: &App) -> Result<Option<Job>> {
    let mut db = a.db.lock().await;
    let tx = db.transaction()?;
    recover_expired(&tx, &a.root, &a.cfg)?;
    let job = tx
        .query_row(
            "SELECT id,filename,original,attempt FROM jobs WHERE status='queued' AND (retry_at IS NULL OR retry_at <= ?1) ORDER BY created_at LIMIT 1",
            [now()],
            |r| {
                Ok(Job {
                    id: r.get(0)?,
                    filename: r.get(1)?,
                    original: r.get(2)?,
                    attempt: r.get::<_, u32>(3)? + 1,
                })
            },
        )
        .optional()?;
    if let Some(job) = &job {
        tx.execute(
            "UPDATE jobs SET status='processing',attempt=?2,lease_until=?3,retry_at=NULL,error=NULL,converter_version=?4,converter_commit=?5,profile_version=?6 WHERE id=?1",
            params![job.id, job.attempt, now() + a.cfg.lease_s, crate::version::PACKAGE, crate::version::COMMIT, crate::version::PROFILE],
        )?;
    }
    tx.commit()?;
    Ok(job)
}

/// Copia el bundle a `published/<id>` con un renombrado atómico y registra el estado.
async fn publish(a: &App, job: &Job, c: &Converted) -> Result<()> {
    // Confirma que la reserva sigue siendo de este intento antes de tocar `published/`.
    if !renew(a, &job.id, job.attempt).await? {
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
    a.db.lock().await.execute(
        "UPDATE jobs SET status=?3,bundle=?4,lease_until=NULL,retry_at=NULL,error=NULL,converter_version=?5,converter_commit=?6,profile_version=?7 WHERE id=?1 AND attempt=?2",
        params![id, job.attempt, c.status, format!("published/{id}/bundle.zip"), c.versions.converter_version, c.versions.converter_commit, c.versions.profile_version],
    )?;
    remove_attempts(&a.root, id);
    info!(%id, status = %c.status, attempt = job.attempt, "trabajo publicado");
    Ok(())
}

/// Registra un fallo: el transitorio vuelve a la cola con espera creciente mientras
/// queden intentos; el permanente, o el transitorio sin intentos, termina en `failed`.
async fn fail(a: &App, job: &Job, kind: FailureKind, message: &str) -> Result<()> {
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
    a.db.lock().await.execute(
        "UPDATE jobs SET status=?3,lease_until=NULL,retry_at=?4,error=?5 WHERE id=?1 AND attempt=?2 AND status='processing'",
        params![job.id, job.attempt, status, retry_at, error],
    )?;
    if !retry {
        remove_attempts(&a.root, &job.id);
    }
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
            let mut db = a.db.lock().await;
            let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let owned = tx
                .prepare("SELECT 1 FROM jobs WHERE id=?1 AND attempt=?2 AND status='processing'")?
                .exists(params![job.id, job.attempt])?;
            if owned {
                if dir.exists()
                    && let Err(error) = fs::remove_dir_all(&dir)
                {
                    warn!(%error, "limpieza interrumpida pendiente para el próximo intento");
                }
                tx.execute("UPDATE jobs SET status='queued',attempt=attempt-1,lease_until=NULL,retry_at=NULL,error=NULL WHERE id=?1", [&job.id])?;
                info!(id = %job.id, "conversión interrumpida; vuelve a la cola sin consumir intento");
            }
            tx.commit()?;
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

/// Reconciliación al arrancar: elimina publicaciones a medio hacer, completa las que
/// terminaron sin registrarse, reencola los trabajos cuyo bundle desapareció y borra
/// los intentos de trabajos terminados.
pub async fn reconcile(a: &App) -> Result<()> {
    let published = a.root.join("published");
    for entry in fs::read_dir(&published)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') && name.ends_with(".tmp") {
            fs::remove_dir_all(entry.path())?;
            info!(%name, "publicación a medio hacer eliminada");
        }
    }
    let mut db = a.db.lock().await;
    let tx = db.transaction()?;
    recover_expired(&tx, &a.root, &a.cfg)?;
    let done: Vec<(String, String)> = tx
        .prepare("SELECT id,bundle FROM jobs WHERE status IN ('complete','partial')")?
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get::<_, Option<String>>(1)?.unwrap_or_default(),
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;
    for (id, bundle) in done {
        if bundle.is_empty() || !a.root.join(&bundle).is_file() {
            tx.execute(
                "UPDATE jobs SET status='queued',bundle=NULL,retry_at=NULL,error='el bundle publicado no existe; se reprocesa el original' WHERE id=?1",
                [&id],
            )?;
            warn!(%id, "bundle publicado ausente; trabajo reencolado");
        }
    }
    for entry in fs::read_dir(a.root.join("attempts"))? {
        let id = entry?.file_name().to_string_lossy().into_owned();
        let status: Option<String> = tx
            .query_row("SELECT status FROM jobs WHERE id=?1", [&id], |r| r.get(0))
            .optional()?;
        if matches!(
            status.as_deref(),
            None | Some("complete" | "partial" | "failed")
        ) {
            remove_attempts(&a.root, &id);
        }
    }
    tx.commit()?;
    Ok(())
}

pub async fn worker(root: PathBuf, poll_ms: u64, once: bool) -> Result<()> {
    let mut a = crate::app(root)?;
    a.converter = Some(std::env::current_exe()?);
    a.shutdown = crate::shutdown::install()?;
    // El límite de memoria solo es una garantía si se puede medir.
    if rss_mb(std::process::id()).await.is_none() {
        bail!("no se puede medir la memoria de los procesos (ni /proc ni ps disponibles)")
    }
    crate::isolation::log_support();
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
    result
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    // Se ejecuta en otro proceso, sin inyectar disparadores en el binario de producción.
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
        let mut a = crate::app(tmp.path().to_path_buf()).unwrap();
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
            a.db.lock().await.execute("INSERT INTO jobs(id,filename,original,sha256,status,created_at) VALUES(?1,'test.docx','unused','-','queued',0)", [id]).unwrap();
            assert!(work_one(&a).await.unwrap());
            let (status, attempt, error): (String, u32, String) =
                a.db.lock()
                    .await
                    .query_row(
                        "SELECT status,attempt,error FROM jobs WHERE id=?1",
                        [id],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                    )
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
}
