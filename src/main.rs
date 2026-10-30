use anyhow::{Context, Result, bail};
use axum::{
    Json, Router,
    extract::{Multipart, Path, State},
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use betteroffice_pptx::{Presentation, RenderOptions};
use clap::{Parser, Subcommand};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    io::{Cursor, Read, Write},
    path::{Path as FsPath, PathBuf},
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;
use tracing::{error, info};
use uuid::Uuid;
use yaml_serde::Value as YamlValue;
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

mod slide_xml;

const MAX_UPLOAD: usize = 100 * 1024 * 1024;
const MAX_ZIP_ENTRY: u64 = 128 * 1024 * 1024;
const MAX_ZIP_TOTAL: u64 = 512 * 1024 * 1024;
const MAX_ZIP_ENTRIES: usize = 4096;

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
    },
    /// Render every PPTX slide with the selected native Rust backend.
    BenchRender { pptx: PathBuf, output: PathBuf },
}

#[derive(Clone)]
struct App {
    root: PathBuf,
    db: Arc<Mutex<Connection>>,
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
    db.execute_batch("CREATE TABLE IF NOT EXISTS jobs(id TEXT PRIMARY KEY, filename TEXT NOT NULL, original TEXT NOT NULL, sha256 TEXT NOT NULL, status TEXT NOT NULL, attempt INTEGER NOT NULL DEFAULT 0, lease_until INTEGER, bundle TEXT, error TEXT, created_at INTEGER NOT NULL); CREATE INDEX IF NOT EXISTS jobs_status ON jobs(status, created_at);")?;
    Ok(App {
        root,
        db: Arc::new(Mutex::new(db)),
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

async fn submit(
    State(a): State<App>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let mut filename = None;
    let mut bytes = None;
    while let Some(field) = multipart.next_field().await.map_err(internal)? {
        if let Some(n) = field.file_name() {
            filename = Some(n.to_string());
            bytes = Some(field.bytes().await.map_err(internal)?.to_vec());
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
    let ext = FsPath::new(&filename)
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext != "pptx" {
        return Err((
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            format!("formato .{ext} aún no implementado; esta entrega acepta únicamente .pptx"),
        ));
    }
    let bytes = bytes.unwrap_or_default();
    if bytes.is_empty() || bytes.len() > MAX_UPLOAD {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            "el archivo está vacío o supera 100 MiB".into(),
        ));
    }
    let id = Uuid::new_v4().to_string();
    let hash = hex(&Sha256::digest(&bytes));
    let original_rel = format!("originals/{id}.pptx");
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
    Router::new()
        .route("/jobs", post(submit))
        .route("/jobs/{id}", get(status))
        .route("/jobs/{id}/bundle", get(download))
        .layer(axum::extract::DefaultBodyLimit::max(MAX_UPLOAD + 64 * 1024))
        .with_state(a)
}

/// Elemento del flujo de una diapositiva, en orden del árbol de formas.
#[derive(Clone)]
enum Item {
    Text(Vec<slide_xml::Paragraph>),
    Table(String),
    Image { path: String, alt: Option<String> },
    Chart(String),
    AltText(String),
}
/// Comentario de revisión (formato clásico o moderno) con sus respuestas.
#[derive(Clone, Default, Debug)]
struct Comment {
    author: Option<String>,
    date: Option<String>,
    text: String,
    replies: Vec<Comment>,
}
#[derive(Clone, Default)]
struct Slide {
    idx: usize,
    slide_id: Option<u32>,
    title: Option<String>,
    title_inferred: bool,
    kicker: Option<String>,
    section: Option<String>,
    items: Vec<Item>,
    notes: Vec<String>,
    image_paths: Vec<String>,
    decorative_images: usize,
    chart_paths: Vec<String>,
    visible_label: Option<String>,
    hidden: bool,
    /// El contenido sustantivo está solo en una imagen, sin transcripción.
    visual_only: bool,
    /// La vista estática muestra una imagen sobre texto que una animación presenta en secuencia.
    view_hides_text: bool,
    comments: Vec<Comment>,
}
/// `Loss` señala pérdida o incertidumbre sustantiva y vuelve `partial` el bundle;
/// `Note` se informa sin afectar el estado.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Severity {
    Loss,
    Note,
}
#[derive(Clone, Debug)]
struct Issue {
    severity: Severity,
    text: String,
}
impl Issue {
    fn loss(text: String) -> Self {
        Issue {
            severity: Severity::Loss,
            text,
        }
    }
    fn note(text: String) -> Self {
        Issue {
            severity: Severity::Note,
            text,
        }
    }
}
struct Extraction {
    slides: Vec<Slide>,
    issues: Vec<Issue>,
    /// Texto de formas fijas del diseño (layout o patrón) y cuántas diapositivas lo muestran.
    design_texts: Vec<(String, usize)>,
}
fn xml_text(xml: &str) -> String {
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut out = String::new();
    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Text(t)) => {
                if let Ok(s) = t.decode()
                    && !s.trim().is_empty()
                {
                    if !out.is_empty() {
                        out.push(' ')
                    }
                    out.push_str(&s);
                }
            }
            Ok(quick_xml::events::Event::GeneralRef(r)) => {
                let resolved = r
                    .resolve_char_ref()
                    .ok()
                    .flatten()
                    .map(String::from)
                    .or_else(|| {
                        r.decode().ok().and_then(|name| {
                            quick_xml::escape::resolve_predefined_entity(&name).map(str::to_string)
                        })
                    });
                if let Some(s) = resolved {
                    out.push_str(&s);
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    out
}
fn chart_text(xml: &str) -> String {
    xml_text(xml)
}
fn attr<'a>(e: &'a quick_xml::events::BytesStart<'a>, name: &[u8]) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.local_name().as_ref() == name)
        .and_then(|a| String::from_utf8(a.value.into_owned()).ok())
}
fn read_entry(z: &mut ZipArchive<Cursor<&[u8]>>, path: &str) -> Result<Vec<u8>> {
    let mut f = z
        .by_name(path)
        .with_context(|| format!("falta parte PPTX {path}"))?;
    if f.size() > MAX_ZIP_ENTRY {
        bail!("parte PPTX excede límite de expansión");
    }
    let mut b = Vec::new();
    f.read_to_end(&mut b)?;
    Ok(b)
}
fn enforce_package_limits(z: &mut ZipArchive<Cursor<&[u8]>>) -> Result<()> {
    if z.len() > MAX_ZIP_ENTRIES {
        bail!("PPTX contiene demasiadas partes OOXML")
    }
    let mut expanded = 0u64;
    for i in 0..z.len() {
        let f = z.by_index(i)?;
        if f.size() > MAX_ZIP_ENTRY {
            bail!("parte PPTX excede límite de expansión")
        }
        expanded = expanded.saturating_add(f.size());
        if expanded > MAX_ZIP_TOTAL {
            bail!("expansión total PPTX excede 512 MiB")
        }
        if f.size() > 1024 * 1024 && f.compressed_size() > 0 && f.size() / f.compressed_size() > 250
        {
            bail!("relación de compresión PPTX excede el límite permitido")
        }
    }
    Ok(())
}
fn relationship_map(xml: &str) -> HashMap<String, String> {
    let mut r = quick_xml::Reader::from_str(xml);
    let mut map = HashMap::new();
    loop {
        match r.read_event() {
            Ok(quick_xml::events::Event::Start(e)) | Ok(quick_xml::events::Event::Empty(e))
                if e.local_name().as_ref() == b"Relationship" =>
            {
                if let (Some(id), Some(target)) = (attr(&e, b"Id"), attr(&e, b"Target")) {
                    map.insert(id, target);
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    map
}
fn resolve(base: &str, target: &str) -> String {
    let raw = if target.starts_with('/') {
        target.trim_start_matches('/').to_string()
    } else {
        format!(
            "{}/{}",
            base.rsplit_once('/').map(|x| x.0).unwrap_or(""),
            target
        )
    };
    let mut parts = Vec::new();
    for p in raw.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            x => parts.push(x),
        }
    }
    parts.join("/")
}
/// Umbral de área (fracción de la diapositiva) bajo el cual una imagen sin texto
/// alternativo útil se considera un icono decorativo.
const DECORATIVE_AREA: f64 = 0.01;

fn meaningful_alt(descr: Option<&str>) -> Option<String> {
    let d = descr?.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = d.to_ascii_lowercase();
    let filename = [
        ".png", ".jpg", ".jpeg", ".gif", ".svg", ".emf", ".wmf", ".bmp", ".tif", ".tiff", ".webp",
    ]
    .iter()
    .any(|ext| lower.ends_with(ext));
    (!d.is_empty() && !filename).then_some(d)
}
/// Detecta un contador visible "05 / 29" cuyo total coincide con la presentación.
fn page_label(text: &str, total: usize) -> Option<String> {
    let (num, den) = text.split_once('/')?;
    let (num, den) = (num.trim(), den.trim());
    let digits = |s: &str| !s.is_empty() && s.len() <= 4 && s.chars().all(|c| c.is_ascii_digit());
    (digits(num) && digits(den) && den.parse::<usize>().ok() == Some(total))
        .then(|| num.to_string())
}
fn table_markdown(rows: &[Vec<String>]) -> String {
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut md = String::new();
    for (row_index, row) in rows.iter().enumerate() {
        md.push('|');
        for col in 0..cols {
            md.push(' ');
            md.push_str(row.get(col).map(String::as_str).unwrap_or(""));
            md.push_str(" |")
        }
        md.push('\n');
        if row_index == 0 {
            md.push('|');
            for _ in 0..cols {
                md.push_str(" --- |")
            }
            md.push('\n');
        }
    }
    md
}
/// Elige como título el texto de mayor tamaño cuando no hay marcador de título:
/// debe ser único, breve, estar en la parte superior y destacar sobre otro texto.
fn infer_title(blocks: &[Option<slide_xml::Block>], slide_height: i64) -> Option<usize> {
    // Los glifos decorativos (comillas, números en círculos) no compiten por el título.
    let sized: Vec<(usize, u32, &slide_xml::TextShape)> = blocks
        .iter()
        .enumerate()
        .filter_map(|(i, b)| match b {
            Some(slide_xml::Block::Text(t)) => t.max_size.map(|s| (i, s, t)),
            _ => None,
        })
        .filter(|(_, _, t)| t.flat_text().chars().filter(|c| c.is_alphabetic()).count() >= 3)
        .collect();
    let (best, size, shape) = sized.iter().max_by_key(|(_, s, _)| *s).copied()?;
    let ties = sized.iter().filter(|(_, s, _)| *s == size).count();
    let smaller = sized.iter().any(|(_, s, _)| *s < size);
    let near_top = shape
        .rect
        .is_none_or(|r| (r.y as f64) < slide_height as f64 * 0.6);
    (ties == 1 && smaller && near_top && shape.flat_text().chars().count() <= 150).then_some(best)
}
/// Antetítulo: texto breve en mayúsculas situado justo encima del título.
fn find_kicker(
    blocks: &[Option<slide_xml::Block>],
    title: slide_xml::Rect,
    title_size: Option<u32>,
) -> Option<String> {
    blocks
        .iter()
        .filter_map(|b| match b {
            Some(slide_xml::Block::Text(t)) => Some(t),
            _ => None,
        })
        .filter_map(|t| {
            let r = t.rect?;
            let text = t.flat_text();
            let letters = text.chars().filter(|c| c.is_alphabetic()).count() >= 3;
            let upper = text
                .chars()
                .filter(|c| c.is_alphabetic())
                .all(|c| !c.is_lowercase());
            let bottom = r.y + r.cy;
            let above = bottom <= title.y + title.cy / 4 && title.y - bottom <= title.cy;
            let overlaps = r.x < title.x + title.cx && r.x + r.cx > title.x;
            let smaller = match (t.max_size, title_size) {
                (Some(a), Some(b)) => a < b,
                _ => true,
            };
            (letters && upper && above && overlaps && smaller && text.chars().count() <= 80)
                .then_some((bottom, text))
        })
        .max_by_key(|(bottom, _)| *bottom)
        .map(|(_, text)| text)
}
/// Atributo del elemento raíz de una parte XML (`show`, `showMasterSp`).
fn root_attr(xml: &str, name: &[u8]) -> Option<String> {
    let mut reader = quick_xml::Reader::from_str(xml);
    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Start(e)) | Ok(quick_xml::events::Event::Empty(e)) => {
                return attr(&e, name);
            }
            Ok(quick_xml::events::Event::Eof) | Err(_) => return None,
            _ => {}
        }
    }
}
/// Texto de las formas que no son marcadores de posición; en un layout o patrón se
/// muestra en cada diapositiva que lo usa.
fn static_texts(xml: &str) -> Result<Vec<String>> {
    Ok(slide_xml::parse_part(xml, &|_| None)?
        .blocks
        .into_iter()
        .filter_map(|b| match b {
            slide_xml::Block::Text(t) if t.placeholder.is_none() => Some(t.flat_text()),
            _ => None,
        })
        .filter(|t| !t.is_empty())
        .collect())
}
/// Fracción del área de `a` cubierta por `b`.
fn covered_fraction(a: slide_xml::Rect, b: slide_xml::Rect) -> f64 {
    let w = (a.x + a.cx).min(b.x + b.cx) - a.x.max(b.x);
    let h = (a.y + a.cy).min(b.y + b.cy) - a.y.max(b.y);
    let area = a.cx as f64 * a.cy as f64;
    if w <= 0 || h <= 0 || area <= 0.0 {
        return 0.0;
    }
    (w as f64 * h as f64) / area
}
/// Una imagen que cubre al menos esta fracción de la diapositiva, sin texto que la
/// acompañe, constituye contenido solo visual.
const VISUAL_ONLY_AREA: f64 = 0.15;
const VISUAL_ONLY_MAX_CHARS: usize = 40;
fn extract(bytes: &[u8], attempt: &FsPath) -> Result<Extraction> {
    let mut z = ZipArchive::new(Cursor::new(bytes))?;
    enforce_package_limits(&mut z)?;
    let pres = String::from_utf8(read_entry(&mut z, "ppt/presentation.xml")?)?;
    let rels = String::from_utf8(read_entry(&mut z, "ppt/_rels/presentation.xml.rels")?)?;
    let relmap = relationship_map(&rels);
    let mut order = Vec::<(u32, String, bool)>::new();
    let mut sections = HashMap::<u32, String>::new();
    let mut current_section: Option<String> = None;
    let mut slide_size = (12_192_000i64, 6_858_000i64);
    let mut rd = quick_xml::Reader::from_str(&pres);
    loop {
        match rd.read_event() {
            Ok(quick_xml::events::Event::Start(e)) | Ok(quick_xml::events::Event::Empty(e))
                if e.local_name().as_ref() == b"sldId" =>
            {
                let id = attr(&e, b"id").and_then(|x| x.parse().ok());
                let rid = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref().ends_with(b":id"))
                    .and_then(|a| String::from_utf8(a.value.into_owned()).ok());
                match (id, rid) {
                    (Some(id), Some(rid)) => {
                        if let Some(target) = relmap.get(&rid) {
                            order.push((
                                id,
                                resolve("ppt/presentation.xml", target),
                                attr(&e, b"show").as_deref() == Some("0"),
                            ));
                        }
                    }
                    // Las secciones (p14:sectionLst) enumeran diapositivas sin r:id.
                    (Some(id), None) => {
                        if let Some(name) = &current_section {
                            sections.insert(id, name.clone());
                        }
                    }
                    _ => {}
                }
            }
            Ok(quick_xml::events::Event::Start(e)) if e.local_name().as_ref() == b"section" => {
                current_section = attr(&e, b"name")
            }
            Ok(quick_xml::events::Event::End(e)) if e.local_name().as_ref() == b"section" => {
                current_section = None
            }
            Ok(quick_xml::events::Event::Start(e)) | Ok(quick_xml::events::Event::Empty(e))
                if e.local_name().as_ref() == b"sldSz" =>
            {
                if let (Some(cx), Some(cy)) = (
                    attr(&e, b"cx").and_then(|v| v.parse().ok()),
                    attr(&e, b"cy").and_then(|v| v.parse().ok()),
                ) {
                    slide_size = (cx, cy);
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(e) => return Err(e.into()),
            _ => {}
        }
    }
    if order.is_empty() {
        bail!("PPTX no contiene diapositivas en la secuencia de presentación")
    }
    let total = order.len();
    let positions: HashMap<String, usize> = order
        .iter()
        .enumerate()
        .map(|(i, (_, path, _))| (path.clone(), i + 1))
        .collect();
    let mut slides = Vec::new();
    let mut all_issues = Vec::new();
    for i in 0..z.len() {
        let name = z.by_index(i)?.name().to_string().to_ascii_lowercase();
        if name.starts_with("ppt/embeddings/") {
            all_issues.push(Issue::loss(format!(
                "Objeto incrustado no interpretado: {name}"
            )));
        }
        if name.starts_with("ppt/media/")
            && [".mp3", ".wav", ".m4a", ".mp4", ".mov", ".avi"]
                .iter()
                .any(|ext| name.ends_with(ext))
        {
            all_issues.push(Issue::loss(format!(
                "Audio o video conservado solo en el original: {name}"
            )));
        }
    }
    fs::create_dir_all(attempt.join("assets"))?;
    let rels_of = |z: &mut ZipArchive<Cursor<&[u8]>>, part: &str| -> HashMap<String, String> {
        let relpath = part
            .rsplit_once('/')
            .map(|(p, file)| format!("{p}/_rels/{file}.rels"))
            .unwrap_or_default();
        relationship_map(
            &String::from_utf8(read_entry(z, &relpath).unwrap_or_default()).unwrap_or_default(),
        )
    };
    // Destino Markdown de un hipervínculo: URL externa o concepto de otra diapositiva.
    let link_target = |map: &HashMap<String, String>, part: &str, rid: &str| -> Option<String> {
        let target = map.get(rid)?;
        let lower = target.to_ascii_lowercase();
        if lower.starts_with("http://")
            || lower.starts_with("https://")
            || lower.starts_with("mailto:")
        {
            return Some(target.clone());
        }
        positions
            .get(&resolve(part, target))
            .map(|n| format!("slide-{n:03}.md"))
    };
    let authors = comment_authors(&mut z);
    let mut layout_texts = HashMap::<String, Vec<String>>::new();
    let mut design_texts = Vec::<(String, usize)>::new();
    let slide_area = (slide_size.0 as f64) * (slide_size.1 as f64);
    let cover =
        |r: Option<slide_xml::Rect>| r.map_or(0.0, |r| (r.cx as f64 * r.cy as f64) / slide_area);
    for (index, (slide_id, path, hidden)) in order.into_iter().enumerate() {
        let raw = String::from_utf8(read_entry(&mut z, &path)?)?;
        let animated = raw.contains("<p:timing");
        let smap = rels_of(&mut z, &path);
        // Texto fijo del layout y del patrón, visible salvo que la diapositiva lo oculte.
        if root_attr(&raw, b"showMasterSp").as_deref() != Some("0")
            && let Some(layout) = smap
                .values()
                .find(|t| t.contains("slideLayouts/"))
                .map(|t| resolve(&path, t))
        {
            if !layout_texts.contains_key(&layout) {
                let mut texts = Vec::new();
                if let Ok(b) = read_entry(&mut z, &layout) {
                    let xml = String::from_utf8(b)?;
                    texts.extend(static_texts(&xml)?);
                    if root_attr(&xml, b"showMasterSp").as_deref() != Some("0") {
                        let lmap = rels_of(&mut z, &layout);
                        if let Some(master) = lmap
                            .values()
                            .find(|t| t.contains("slideMasters/"))
                            .map(|t| resolve(&layout, t))
                            && let Ok(b) = read_entry(&mut z, &master)
                        {
                            texts.extend(static_texts(&String::from_utf8(b)?)?);
                        }
                    }
                }
                layout_texts.insert(layout.clone(), texts);
            }
            for text in &layout_texts[&layout] {
                match design_texts.iter_mut().find(|(t, _)| t == text) {
                    Some((_, n)) => *n += 1,
                    None => design_texts.push((text.clone(), 1)),
                }
            }
        }
        let raw_hidden = quick_xml::Reader::from_str(&raw)
            .read_event()
            .ok()
            .and_then(|e| match e {
                quick_xml::events::Event::Start(e) => attr(&e, b"show"),
                _ => None,
            })
            .as_deref()
            == Some("0");
        let mut s = Slide {
            idx: index + 1,
            slide_id: Some(slide_id),
            hidden: hidden || raw_hidden,
            section: sections.get(&slide_id).cloned(),
            ..Slide::default()
        };
        let part = slide_xml::parse_part(&raw, &|rid| link_target(&smap, &path, rid))?;
        if part.smartart_frames > 0 {
            all_issues.push(Issue::loss(format!(
                "Diapositiva {} contiene SmartArt cuyo texto no se extrajo",
                index + 1
            )));
        }
        s.decorative_images += part.background_images;
        let equations = part.equations;
        let mut blocks: Vec<Option<slide_xml::Block>> = part.blocks.into_iter().map(Some).collect();
        // Numeración visible: marcador sldNum o contador "NN / total" en un cuadro de texto.
        for b in blocks.iter_mut() {
            if let Some(slide_xml::Block::Text(t)) = b {
                let text = t.flat_text();
                let label = match t.placeholder.as_deref() {
                    Some("sldNum") => {
                        Some(text.chars().all(|c| c.is_ascii_digit()).then_some(text))
                    }
                    None => page_label(&text, total).map(Some),
                    _ => None,
                };
                if let Some(label) = label {
                    if label.is_some() {
                        s.visible_label = label;
                    }
                    *b = None;
                }
            }
        }
        // La vista dibuja todas las formas a la vez: una imagen grande posterior a un texto
        // lo tapa, aunque en la presentación la animación los muestre en secuencia.
        if animated {
            let mut earlier: Vec<Option<slide_xml::Rect>> = Vec::new();
            for b in blocks.iter().flatten() {
                match b {
                    slide_xml::Block::Text(t) => earlier.push(t.rect),
                    slide_xml::Block::Picture(p) => {
                        let c = cover(p.rect);
                        if c >= 0.5
                            && earlier.iter().any(|t| {
                                c >= 0.9
                                    || t.zip(p.rect)
                                        .is_some_and(|(t, p)| covered_fraction(t, p) >= 0.5)
                            })
                        {
                            s.view_hides_text = true;
                        }
                    }
                    _ => {}
                }
            }
        }
        let explicit = blocks.iter().position(|b| {
            matches!(b, Some(slide_xml::Block::Text(t))
                if matches!(t.placeholder.as_deref(), Some("title") | Some("ctrTitle")))
        });
        let title_index = explicit.or_else(|| {
            let inferred = infer_title(&blocks, slide_size.1);
            s.title_inferred = inferred.is_some();
            inferred
        });
        if let Some(i) = title_index
            && let Some(slide_xml::Block::Text(t)) = blocks[i].take()
        {
            s.title = Some(t.flat_text());
            if let Some(r) = t.rect {
                s.kicker = find_kicker(&blocks, r, t.max_size);
            }
        }
        let mut assets = HashMap::<String, String>::new();
        let mut largest_image = 0.0f64;
        for block in blocks.into_iter().flatten() {
            match block {
                slide_xml::Block::Text(t) => s.items.push(Item::Text(t.paragraphs)),
                slide_xml::Block::Table(rows) => s.items.push(Item::Table(table_markdown(&rows))),
                slide_xml::Block::Picture(p) => {
                    let alt = meaningful_alt(p.descr.as_deref());
                    let small = p.rect.is_some_and(|r| {
                        (r.cx as f64) * (r.cy as f64) < slide_area * DECORATIVE_AREA
                    });
                    if alt.is_none() && small {
                        s.decorative_images += 1;
                        continue;
                    }
                    let Some(target) = smap.get(&p.rid) else {
                        all_issues.push(Issue::loss(format!(
                            "Diapositiva {}: imagen sin relación resoluble ({})",
                            index + 1,
                            p.rid
                        )));
                        continue;
                    };
                    if target.contains("://") {
                        all_issues.push(Issue::loss(format!(
                            "Diapositiva {}: imagen vinculada externamente no incluida: {target}",
                            index + 1
                        )));
                        continue;
                    }
                    if alt.is_none() {
                        largest_image = largest_image.max(cover(p.rect));
                    }
                    let rp = resolve(&path, target);
                    let asset = match assets.get(&rp) {
                        Some(a) => a.clone(),
                        None => {
                            let data = read_entry(&mut z, &rp)?;
                            let ext = FsPath::new(&rp)
                                .extension()
                                .and_then(|x| x.to_str())
                                .unwrap_or("bin")
                                .to_ascii_lowercase();
                            let name = format!(
                                "assets/slide-{:03}-image-{:02}.{ext}",
                                index + 1,
                                assets.len() + 1
                            );
                            fs::write(attempt.join(&name), data)?;
                            assets.insert(rp, name.clone());
                            s.image_paths.push(name.clone());
                            name
                        }
                    };
                    s.items.push(Item::Image { path: asset, alt });
                }
                slide_xml::Block::AltText(d) => {
                    if let Some(d) = meaningful_alt(Some(&d)) {
                        s.items.push(Item::AltText(d));
                    }
                }
                slide_xml::Block::Chart(rid) => {
                    if let Some(target) = smap.get(&rid) {
                        let cp = resolve(&path, target);
                        s.chart_paths.push(cp.clone());
                        if let Ok(b) = read_entry(&mut z, &cp) {
                            let text = chart_text(std::str::from_utf8(&b)?);
                            if !text.is_empty() {
                                s.items.push(Item::Chart(format!(
                                    "Datos del gráfico: {}",
                                    slide_xml::escape_inline(&text)
                                )));
                            }
                        }
                    }
                }
            }
        }
        for target in smap.values().filter(|t| t.contains("comments/")) {
            if let Ok(b) = read_entry(&mut z, &resolve(&path, target)) {
                s.comments
                    .extend(parse_comments(&String::from_utf8(b)?, &authors)?);
            }
        }
        // Las notas son una parte OOXML aparte; se omiten sus marcadores de imagen,
        // número, encabezado, pie y fecha.
        if let Some(target) = smap.values().find(|t| t.contains("notesSlides/")) {
            let np = resolve(&path, target);
            if let Ok(b) = read_entry(&mut z, &np) {
                let nmap = rels_of(&mut z, &np);
                let notes = slide_xml::parse_part(std::str::from_utf8(&b)?, &|rid| {
                    link_target(&nmap, &np, rid)
                })?;
                for block in notes.blocks {
                    if let slide_xml::Block::Text(t) = block
                        && matches!(t.placeholder.as_deref(), None | Some("body"))
                    {
                        s.notes.extend(t.paragraphs.iter().flat_map(|p| {
                            p.md.split('\n')
                                .map(str::trim)
                                .filter(|l| !l.is_empty())
                                .map(str::to_string)
                                .collect::<Vec<_>>()
                        }));
                    }
                }
            }
        }
        let body_chars: usize = s
            .items
            .iter()
            .map(|i| match i {
                Item::Text(p) => p
                    .iter()
                    .map(|p| p.plain.chars().filter(|c| c.is_alphanumeric()).count())
                    .sum(),
                Item::Table(md) | Item::Chart(md) | Item::AltText(md) => {
                    md.chars().filter(|c| c.is_alphanumeric()).count()
                }
                Item::Image { .. } => 0,
            })
            .sum();
        s.visual_only = largest_image >= VISUAL_ONLY_AREA && body_chars < VISUAL_ONLY_MAX_CHARS;
        let n = index + 1;
        if s.visual_only {
            all_issues.push(Issue::loss(format!(
                "Diapositiva {n}: contenido solo visual; la imagen se adjunta sin transcripción"
            )));
        }
        if equations > 0 {
            all_issues.push(Issue::note(format!(
                "Diapositiva {n}: {equations} ecuación(es) convertidas a notación lineal; la vista conserva su forma"
            )));
        }
        if !s.comments.is_empty() {
            let total: usize = s.comments.iter().map(|c| 1 + c.replies.len()).sum();
            all_issues.push(Issue::note(format!(
                "Diapositiva {n}: {total} comentario(s) de revisión conservados como contenido auxiliar"
            )));
        }
        if s.view_hides_text {
            all_issues.push(Issue::loss(format!(
                "Diapositiva {n}: la vista estática muestra una imagen sobre texto que la animación presenta en secuencia; el texto está en el concepto, no en la vista"
            )));
        } else if animated {
            all_issues.push(Issue::note(format!(
                "Diapositiva {n} contiene animaciones; la vista muestra todas las formas a la vez"
            )));
        }
        if s.items.is_empty() && s.title.is_none() {
            all_issues.push(Issue::note(format!(
                "Diapositiva {n} sin contenido propio extraíble; lo visible procede del diseño y queda en la vista"
            )));
        } else if s.title.is_none() {
            all_issues.push(Issue::note(format!("Diapositiva {n} sin título explícito")));
        } else if s.title_inferred {
            all_issues.push(Issue::note(format!(
                "Diapositiva {n}: título inferido por tamaño y posición del texto"
            )));
        }
        slides.push(s);
    }
    for (text, count) in &design_texts {
        all_issues.push(Issue::note(format!(
            "Texto del diseño visible en {count} diapositiva(s), registrado en documento.md y no repetido en sus conceptos: «{}»",
            truncate_chars(text, 120)
        )));
    }
    Ok(Extraction {
        slides,
        issues: all_issues,
        design_texts,
    })
}
/// Recorta en un límite de palabra y marca el corte con «…».
fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    format!("{}…", cut.trim_end())
}
fn slide_description(s: &Slide, deck: &str) -> String {
    let mut d = format!("Diapositiva {} de {deck}", s.idx);
    if s.hidden {
        d.push_str(" (oculta)");
    }
    if let Some(t) = &s.title {
        d.push_str(": ");
        d.push_str(t);
    }
    d.push('.');
    let first = s.items.iter().find_map(|i| match i {
        Item::Text(p) => p
            .iter()
            .map(|p| p.plain.split_whitespace().collect::<Vec<_>>().join(" "))
            .find(|l| !l.is_empty() && s.kicker.as_deref() != Some(l.as_str())),
        _ => None,
    });
    match first {
        Some(line) => {
            d.push(' ');
            d.push_str(&truncate_chars(&line, 160));
        }
        None if s.visual_only => d.push_str(" Contenido solo visual."),
        None => {}
    }
    d
}
fn index_entry(s: &Slide, href: &str) -> String {
    let label = match &s.title {
        Some(t) => format!("Diapositiva {}: {}", s.idx, slide_xml::escape_inline(t)),
        None => format!("Diapositiva {}", s.idx),
    };
    let mut flags = Vec::new();
    if s.hidden {
        flags.push("oculta");
    }
    if s.visual_only {
        flags.push("contenido solo visual");
    }
    let suffix = if flags.is_empty() {
        String::new()
    } else {
        format!(" — {}", flags.join(", "))
    };
    format!("- [{label}]({href}){suffix}\n")
}
fn esc(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', " ")
}
fn yaml(s: &str) -> String {
    format!("\"{}\"", esc(s))
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|v| format!("{v:02x}")).collect()
}
fn markdown_slide(s: &Slide, deck: &str, instant: &str) -> String {
    let mut out = format!(
        "---\ntype: Slide\ntitle: {}\ndescription: {}\nsources:\n  - id: original\n    resource: /references/original.pptx\ngenerated:\n  by: process:document-conversion\n  at: {}\nsource_spans:\n  - id: slide-{}\n    source_id: original\n    kind: pptx_slide\n    slide_index: {}\n",
        yaml(s.title.as_deref().unwrap_or("Diapositiva sin título")),
        yaml(&slide_description(s, deck)),
        yaml(instant),
        s.idx,
        s.idx
    );
    if let Some(id) = s.slide_id {
        out.push_str(&format!("    slide_id: {}\n", id));
    }
    if let Some(label) = &s.visible_label {
        out.push_str(&format!("    display_label: {}\n", yaml(label)));
    }
    if s.title_inferred {
        out.push_str("title_inferred: true\n");
    }
    if let Some(kicker) = &s.kicker {
        out.push_str(&format!("kicker: {}\n", yaml(kicker)));
    }
    if let Some(section) = &s.section {
        out.push_str(&format!("section: {}\n", yaml(section)));
    }
    if s.visual_only {
        out.push_str("visual_only: true\n");
    }
    if s.view_hides_text {
        out.push_str("view_hides_text: true\n");
    }
    if s.title.is_none() || s.title_inferred || s.visual_only {
        out.push_str("status: draft\n");
    }
    out.push_str("---\n\n");
    if let Some(title) = &s.title {
        out.push_str(&format!("# {}\n\n", slide_xml::escape_inline(title)));
        if s.title_inferred {
            out.push_str("> Título inferido por tamaño y posición del texto; la diapositiva no tiene marcador de título.\n\n");
        }
    } else {
        out.push_str(&format!(
            "# Diapositiva {} (sin título explícito)\n\n",
            s.idx
        ));
    }
    if s.hidden {
        out.push_str("> Diapositiva oculta en la presentación original.\n\n");
    }
    if s.visual_only {
        out.push_str("> Contenido solo visual: lo sustantivo está en la imagen y no tiene transcripción.\n\n");
    }
    if s.view_hides_text {
        out.push_str("> La vista estática muestra una imagen sobre texto que la animación original presenta en secuencia; el texto se conserva abajo.\n\n");
    }
    out.push_str("## Contenido\n\n");
    let mut in_list = false;
    let mut image_no = 0;
    for item in &s.items {
        if in_list && !matches!(item, Item::Text(_)) {
            out.push('\n');
            in_list = false;
        }
        match item {
            Item::Text(paragraphs) => {
                for p in paragraphs {
                    let indent = "  ".repeat(p.level as usize);
                    let mut lines = p.md.split('\n');
                    out.push_str(&format!("{indent}- {}\n", lines.next().unwrap_or("")));
                    for line in lines {
                        out.push_str(&format!("{indent}  {line}\n"));
                    }
                }
                in_list = true;
            }
            Item::Table(md) => out.push_str(&format!("{md}\n")),
            Item::Image { path, alt } => {
                image_no += 1;
                let alt = alt
                    .as_deref()
                    .map(slide_xml::escape_inline)
                    .unwrap_or_else(|| format!("Imagen {image_no} de la diapositiva {}", s.idx));
                out.push_str(&format!("![{alt}](../{path})\n\n"));
            }
            Item::Chart(text) => out.push_str(&format!("{text}\n\n")),
            Item::AltText(text) => out.push_str(&format!(
                "> Texto alternativo: {}\n\n",
                slide_xml::escape_inline(text)
            )),
        }
    }
    if s.items.is_empty() {
        out.push_str("Sin texto extraído.\n");
    }
    while out.ends_with("\n\n") {
        out.pop();
    }
    if !s.notes.is_empty() {
        out.push_str("\n## Notas del presentador\n\n");
        out.push_str(&s.notes.join("\n\n"));
        out.push('\n');
    }
    if !s.comments.is_empty() {
        out.push_str("\n## Comentarios de revisión\n\n> Comentarios del archivo original; no forman parte del contenido presentado.\n\n");
        fn line(c: &Comment, indent: &str) -> String {
            let mut head = format!(
                "{indent}- **{}**",
                slide_xml::escape_inline(c.author.as_deref().unwrap_or("Autor desconocido"))
            );
            if let Some(d) = &c.date {
                head.push_str(&format!(" · {d}"));
            }
            let text = c.text.split_whitespace().collect::<Vec<_>>().join(" ");
            let mut out = format!("{head}: {}\n", slide_xml::escape_inline(&text));
            for r in &c.replies {
                out.push_str(&line(r, &format!("{indent}  ")));
            }
            out
        }
        for c in &s.comments {
            out.push_str(&line(c, ""));
        }
    }
    out
}
/// Autores de comentarios: `commentAuthors.xml` (clásico) y `authors.xml` (moderno).
fn comment_authors(z: &mut ZipArchive<Cursor<&[u8]>>) -> HashMap<String, String> {
    let mut authors = HashMap::new();
    for part in ["ppt/commentAuthors.xml", "ppt/authors.xml"] {
        let Ok(b) = read_entry(z, part) else { continue };
        let xml = String::from_utf8_lossy(&b);
        let mut reader = quick_xml::Reader::from_str(&xml);
        loop {
            match reader.read_event() {
                Ok(quick_xml::events::Event::Start(e)) | Ok(quick_xml::events::Event::Empty(e))
                    if matches!(e.local_name().as_ref(), b"cmAuthor" | b"author") =>
                {
                    if let (Some(id), Some(name)) = (attr(&e, b"id"), attr(&e, b"name")) {
                        authors.insert(id, name);
                    }
                }
                Ok(quick_xml::events::Event::Eof) | Err(_) => break,
                _ => {}
            }
        }
    }
    authors
}
/// Lee comentarios clásicos (`p:cm` con `p:text`) y modernos (`p188:cm` con párrafos
/// DrawingML y `p188:reply`).
fn parse_comments(xml: &str, authors: &HashMap<String, String>) -> Result<Vec<Comment>> {
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut open: Vec<Comment> = Vec::new();
    let mut done = Vec::new();
    let mut in_text = false;
    loop {
        match reader.read_event()? {
            quick_xml::events::Event::Start(e) => match e.local_name().as_ref() {
                b"cm" | b"reply" => open.push(Comment {
                    author: attr(&e, b"authorId").map(|id| authors.get(&id).cloned().unwrap_or(id)),
                    date: attr(&e, b"dt")
                        .or_else(|| attr(&e, b"created"))
                        .map(|d| d.chars().take(10).collect()),
                    ..Comment::default()
                }),
                b"text" | b"t" => in_text = true,
                _ => {}
            },
            quick_xml::events::Event::Text(t) if in_text => {
                if let Some(c) = open.last_mut() {
                    c.text.push_str(&t.decode()?);
                }
            }
            quick_xml::events::Event::GeneralRef(r) if in_text => {
                if let Some(c) = open.last_mut()
                    && let Some(s) = r.resolve_char_ref()?.map(String::from).or_else(|| {
                        r.decode().ok().and_then(|n| {
                            quick_xml::escape::resolve_predefined_entity(&n).map(str::to_string)
                        })
                    })
                {
                    c.text.push_str(&s);
                }
            }
            quick_xml::events::Event::End(e) => match e.local_name().as_ref() {
                b"text" | b"t" => in_text = false,
                b"p" => {
                    if let Some(c) = open.last_mut() {
                        c.text.push('\n');
                    }
                }
                b"reply" => {
                    if let Some(r) = open.pop()
                        && let Some(parent) = open.last_mut()
                    {
                        parent.replies.push(r);
                    }
                }
                b"cm" => {
                    if let Some(c) = open.pop() {
                        done.push(c);
                    }
                }
                _ => {}
            },
            quick_xml::events::Event::Eof => break,
            _ => {}
        }
    }
    Ok(done)
}
fn write_zip_file(zip: &mut ZipWriter<fs::File>, name: &str, data: &[u8]) -> Result<()> {
    zip.start_file(name, SimpleFileOptions::default())?;
    zip.write_all(data)?;
    Ok(())
}
fn render_font() -> Result<Vec<u8>> {
    if let Ok(path) = std::env::var("OKF_FONT_PATH") {
        return fs::read(path).context("no se pudo leer OKF_FONT_PATH");
    }
    for path in [
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
    ] {
        if let Ok(b) = fs::read(path) {
            return Ok(b);
        }
    }
    bail!("no hay fuente compatible; configura OKF_FONT_PATH con una fuente TrueType")
}
fn build_bundle(
    bytes: &[u8],
    filename: &str,
    hash: &str,
    extraction: &Extraction,
    attempt: &FsPath,
) -> Result<(PathBuf, String)> {
    let slides = &extraction.slides;
    let started = Instant::now();
    let mut ppt = Presentation::open(bytes)?;
    let font = render_font()?;
    ppt.register_font("Arial", false, false, &font)?;
    ppt.register_font("Arial", true, false, &font)?;
    if ppt.slides().len() != slides.len() {
        bail!("cantidad de vistas no coincide con diapositivas")
    }
    let generated = chrono_now();
    let mut allissues = extraction.issues.clone();
    let zip_path = attempt.join("bundle.zip");
    let f = fs::File::create(&zip_path)?;
    let mut z = ZipWriter::new(f);
    fs::create_dir_all(attempt.join("assets"))?;
    for s in slides {
        let rendered = ppt
            .render_png(s.idx - 1, &RenderOptions::default())
            .context("falló renderizado PPTX Rust")?;
        if rendered.bytes.is_empty() {
            bail!("vista obligatoria ausente en diapositiva {}", s.idx)
        }
        let view = format!("assets/slide-{:03}.png", s.idx);
        write_zip_file(&mut z, &view, &rendered.bytes)?;
        if rendered.skipped_images > 0 {
            allissues.push(Issue::loss(format!(
                "Diapositiva {}: renderizador omitió {} imagen(es)",
                s.idx, rendered.skipped_images
            )));
        }
        let mut md = markdown_slide(s, filename, &generated);
        md.push_str(&format!(
            "\n## Vista visual\n\n![Vista de la diapositiva {}](../assets/slide-{:03}.png)\n",
            s.idx, s.idx
        ));
        write_zip_file(
            &mut z,
            &format!("contenido/slide-{:03}.md", s.idx),
            md.as_bytes(),
        )?;
    }
    for entry in fs::read_dir(attempt.join("assets"))? {
        let e = entry?;
        if e.file_type()?.is_file() {
            let name = format!("assets/{}", e.file_name().to_string_lossy());
            write_zip_file(&mut z, &name, &fs::read(e.path())?)?;
        }
    }
    let original_ext = FsPath::new(filename)
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("pptx");
    let orig = format!("references/original.{original_ext}");
    write_zip_file(&mut z, &orig, bytes)?;
    let (losses, notes): (Vec<&Issue>, Vec<&Issue>) =
        allissues.iter().partition(|i| i.severity == Severity::Loss);
    let state = if losses.is_empty() {
        "complete"
    } else {
        "partial"
    };
    let slide_links = slides
        .iter()
        .map(|s| index_entry(s, &format!("contenido/slide-{:03}.md", s.idx)))
        .collect::<String>();
    let content_slide_links = slides
        .iter()
        .map(|s| index_entry(s, &format!("slide-{:03}.md", s.idx)))
        .collect::<String>();
    let design_md = if extraction.design_texts.is_empty() {
        String::new()
    } else {
        let lines = extraction
            .design_texts
            .iter()
            .map(|(t, n)| {
                format!(
                    "- {} ({n} diapositiva(s))\n",
                    slide_xml::escape_line_start(&slide_xml::escape_inline(t))
                )
            })
            .collect::<String>();
        format!(
            "\n## Texto del diseño\n\nFormas fijas del layout o del patrón, visibles en las diapositivas que los usan y no repetidas en sus conceptos.\n\n{lines}"
        )
    };
    let index = format!(
        "---\nokf_version: \"0.2\"\n---\n\n# Bundle OKF\n\n- [Documento fuente](documento.md)\n- [Índice de diapositivas](contenido/index.md)\n- [Informe de conversión](conversion-report.md)\n\n## Diapositivas\n\n{slide_links}"
    );
    let contents_index = format!("# Contenido\n\n## Diapositivas\n\n{content_slide_links}");
    let doc = format!(
        "---\ntype: Source Document\ntitle: {}\ndescription: Presentación fuente convertida a conceptos OKF por diapositiva.\nsources:\n  - id: original\n    resource: /{orig}\ngenerated:\n  by: process:document-conversion\n  at: {}\nsource_file:\n  path: {}\n  sha256: {}\n---\n\n# {}\n\nPresentación de {} diapositivas.\n{design_md}",
        yaml(filename),
        yaml(&generated),
        yaml(&orig),
        hash,
        slide_xml::escape_inline(filename),
        slides.len()
    );
    let list = |items: &[&Issue], empty: &str| -> String {
        if items.is_empty() {
            format!("{empty}\n")
        } else {
            items
                .iter()
                .map(|i| format!("- {}\n", slide_xml::escape_inline(&i.text)))
                .collect()
        }
    };
    let count = |f: fn(&Slide) -> bool| slides.iter().filter(|s| f(s)).count();
    let report = format!(
        "---\ntype: Conversion Report\ntitle: Informe de conversión\ngenerated:\n  by: process:document-conversion\n  at: {}\nstatus: {}\nformat: pptx\nsha256: {}\n---\n\n# Informe de conversión PPTX\n\n- Estado: `{}`\n- Diapositivas inventariadas: {}\n- Diapositivas representadas con concepto y vista: {}\n- Vistas adjuntas: {}\n- Diapositivas con contenido solo visual: {}\n- Títulos inferidos: {}; diapositivas sin título: {}\n- Imágenes adjuntas: {}\n- Imágenes decorativas visibles solo en las vistas (iconos sin texto alternativo útil, menos del 1 % del área, o fondos): {}\n- Vistas: BetterOffice PPTX Raster (Rust)\n- Duración de ensamblado/renderizado: {} ms\n\n## Incidencias\n\nPérdidas o incertidumbres sustantivas; determinan el estado `partial`.\n\n{}\n## Observaciones\n\nInformación sobre la conversión que no afecta el estado.\n\n{}",
        yaml(&generated),
        state,
        hash,
        state,
        slides.len(),
        slides.len(),
        slides.len(),
        count(|s| s.visual_only),
        count(|s| s.title_inferred),
        count(|s| s.title.is_none()),
        slides.iter().map(|s| s.image_paths.len()).sum::<usize>(),
        slides.iter().map(|s| s.decorative_images).sum::<usize>(),
        started.elapsed().as_millis(),
        list(&losses, "Sin pérdidas ni incertidumbres sustantivas."),
        list(&notes, "Sin observaciones.")
    );
    write_zip_file(&mut z, "index.md", index.as_bytes())?;
    write_zip_file(&mut z, "contenido/index.md", contents_index.as_bytes())?;
    write_zip_file(&mut z, "documento.md", doc.as_bytes())?;
    write_zip_file(&mut z, "conversion-report.md", report.as_bytes())?;
    z.finish()?;
    validate_bundle(&zip_path)?;
    Ok((zip_path, state.into()))
}
fn frontmatter(md: &str) -> Result<YamlValue> {
    if !md.starts_with("---\n") {
        bail!("concepto sin frontmatter")
    };
    let rest = &md[4..];
    let end = rest.find("\n---\n").context("frontmatter sin cierre")?;
    Ok(yaml_serde::from_str(&rest[..end])?)
}
fn local_link_exists(
    entry: &str,
    target: &str,
    entries: &std::collections::HashSet<String>,
) -> bool {
    if target.starts_with("http://")
        || target.starts_with("https://")
        || target.starts_with("mailto:")
        || target.is_empty()
    {
        return true;
    }
    let t = target.split('#').next().unwrap_or("");
    if t.is_empty() {
        return true;
    }
    let raw = if t.starts_with('/') {
        t.trim_start_matches('/').to_string()
    } else {
        format!(
            "{}/{}",
            entry.rsplit_once('/').map(|x| x.0).unwrap_or(""),
            t
        )
    };
    let mut p = Vec::new();
    for c in raw.split('/') {
        match c {
            "" | "." => {}
            ".." => {
                p.pop();
            }
            x => p.push(x),
        }
    }
    entries.contains(&p.join("/"))
}
fn validate_bundle(path: &FsPath) -> Result<()> {
    let mut z = ZipArchive::new(fs::File::open(path)?)?;
    let names: std::collections::HashSet<String> = (0..z.len())
        .map(|i| z.by_index(i).map(|e| e.name().to_string()))
        .collect::<std::result::Result<_, _>>()?;
    for req in [
        "index.md",
        "contenido/index.md",
        "documento.md",
        "conversion-report.md",
        "references/original.pptx",
    ] {
        if !names.contains(req) {
            bail!("bundle no contiene {req}")
        }
    }
    let mut root = String::new();
    z.by_name("index.md")?.read_to_string(&mut root)?;
    let rootfm = frontmatter(&root)?;
    if rootfm.get("okf_version").and_then(YamlValue::as_str) != Some("0.2") {
        bail!("index.md no declara okf_version 0.2")
    }
    for name in names.iter().filter(|n| n.ends_with(".md")) {
        let mut md = String::new();
        z.by_name(name)?.read_to_string(&mut md)?;
        let reserved = name == "index.md" || name == "contenido/index.md" || name == "log.md";
        if !reserved {
            let fm = frontmatter(&md)?;
            if fm
                .get("type")
                .and_then(YamlValue::as_str)
                .filter(|x| !x.trim().is_empty())
                .is_none()
            {
                bail!("concepto {name} carece de type")
            }
            if name != "conversion-report.md"
                && (fm.get("title").is_none()
                    || fm.get("description").is_none()
                    || fm.get("sources").is_none()
                    || fm.get("generated").is_none())
            {
                bail!("concepto {name} carece de metadatos requeridos")
            }
            if let Some(sources) = fm.get("sources").and_then(YamlValue::as_sequence) {
                for source in sources {
                    if let Some(resource) = source.get("resource").and_then(YamlValue::as_str) {
                        let resource = resource.trim_start_matches('/');
                        if !names.contains(resource) {
                            bail!("recurso de origen sin resolver en {name}: {resource}")
                        }
                    }
                }
            }
        }
        if let Some(suffix) = name
            .strip_prefix("contenido/slide-")
            .and_then(|n| n.strip_suffix(".md"))
        {
            let view = format!("assets/slide-{suffix}.png");
            let relative = format!("../{view}");
            if !names.contains(&view) || !md.contains(&format!("]({relative})")) {
                bail!("diapositiva {suffix} carece de su vista visual obligatoria o de su enlace")
            }
        }
        let mut pos = 0;
        while let Some(start) = md[pos..].find("](") {
            let s = pos + start + 2;
            // `\](` es texto literal escapado, no un enlace.
            if md[..pos + start].ends_with('\\') {
                pos = s;
                continue;
            }
            // Un destino entre `<` y `>` puede contener paréntesis.
            let close = if md[s..].starts_with('<') {
                md[s..].find('>').map(|i| i + 1)
            } else {
                md[s..].find(')')
            };
            if let Some(end) = close {
                let target = md[s..s + end]
                    .trim()
                    .trim_start_matches('<')
                    .trim_end_matches('>');
                if !local_link_exists(name, target, &names) {
                    bail!("enlace interno sin resolver en {name}: {target}")
                }
                pos = s + end + 1
            } else {
                break;
            }
        }
    }
    Ok(())
}
fn chrono_now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

async fn claim(a: &App) -> Result<Option<(String, String, String, u32)>> {
    let mut db = a.db.lock().await;
    let tx = db.transaction()?;
    tx.execute("UPDATE jobs SET status='queued',lease_until=NULL,error='worker anterior interrumpido; se reprocesará el original' WHERE status IN ('processing','validating') AND lease_until < ?1",[now()])?;
    let row:Option<(String,String,String,u32)>=tx.query_row("SELECT id,filename,original,attempt FROM jobs WHERE status='queued' ORDER BY created_at LIMIT 1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
    if let Some((id, _, _, attempt)) = &row {
        tx.execute(
            "UPDATE jobs SET status='processing',attempt=?2,lease_until=?3,error=NULL WHERE id=?1",
            params![id, attempt + 1, now() + 600],
        )?;
    }
    tx.commit()?;
    Ok(row.map(|(id, f, o, n)| (id, f, o, n + 1)))
}
async fn work_one(a: &App) -> Result<bool> {
    let Some((id, filename, original, attempt_no)) = claim(a).await? else {
        return Ok(false);
    };
    let attempt = a
        .root
        .join("attempts")
        .join(&id)
        .join(format!("{attempt_no}"));
    if attempt.exists() {
        fs::remove_dir_all(&attempt)?
    }
    fs::create_dir_all(&attempt)?;
    let result = (|| -> Result<(String, PathBuf)> {
        let bytes = fs::read(a.root.join(original))?;
        let hash = hex(&Sha256::digest(&bytes));
        let extraction = extract(&bytes, &attempt)?;
        if extraction.slides.is_empty() {
            bail!("sin diapositivas")
        };
        let (bundle, state) = build_bundle(&bytes, &filename, &hash, &extraction, &attempt)?;
        Ok((state, bundle))
    })();
    match result {
        Ok((status, bundle)) => {
            let pubdir = a.root.join("published").join(&id);
            let staging = a.root.join("published").join(format!(".{id}.tmp"));
            if staging.exists() {
                fs::remove_dir_all(&staging)?
            }
            fs::create_dir_all(&staging)?;
            let finalzip = staging.join("bundle.zip");
            fs::copy(bundle, &finalzip)?;
            if pubdir.exists() {
                fs::remove_dir_all(&pubdir)?
            }
            fs::rename(&staging, &pubdir)?;
            let db = a.db.lock().await;
            db.execute(
                "UPDATE jobs SET status=?2,bundle=?3,lease_until=NULL,error=NULL WHERE id=?1",
                params![id, status, format!("published/{id}/bundle.zip")],
            )?;
            info!(%id,%status,attempt_no,"trabajo publicado");
        }
        Err(e) => {
            error!(%id,error=%e,"falló conversión");
            let db = a.db.lock().await;
            db.execute(
                "UPDATE jobs SET status='failed',lease_until=NULL,error=?2 WHERE id=?1",
                params![id, e.to_string()],
            )?;
        }
    }
    Ok(true)
}
async fn worker(root: PathBuf, poll_ms: u64) -> Result<()> {
    let a = app(root)?;
    loop {
        if !work_one(&a).await? {
            tokio::time::sleep(Duration::from_millis(poll_ms)).await;
        }
    }
}
fn bench_render(input: &FsPath, output: &FsPath) -> Result<()> {
    let started = Instant::now();
    let bytes = fs::read(input)?;
    fs::create_dir_all(output)?;
    let mut ppt = Presentation::open(&bytes)?;
    let font = render_font()?;
    ppt.register_font("Arial", false, false, &font)?;
    ppt.register_font("Arial", true, false, &font)?;
    let mut total = 0usize;
    for i in 0..ppt.slides().len() {
        let image = ppt.render_png(i, &RenderOptions::default())?;
        if image.bytes.is_empty() {
            bail!("sin PNG para diapositiva {}", i + 1)
        }
        let path = output.join(format!("slide-{:03}.png", i + 1));
        fs::write(path, &image.bytes)?;
        total += image.bytes.len();
        println!(
            "slide={} png_bytes={} skipped_images={}",
            i + 1,
            image.bytes.len(),
            image.skipped_images
        );
    }
    println!(
        "renderer=betteroffice-pptx-raster slides={} elapsed_ms={} png_total_bytes={}",
        ppt.slides().len(),
        started.elapsed().as_millis(),
        total
    );
    Ok(())
}
#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let cli = Cli::parse();
    let root = PathBuf::from(std::env::var("OKF_DATA_DIR").unwrap_or_else(|_| "data".into()));
    match cli.command {
        Command::Api { bind } => serve(root, &bind).await,
        Command::Worker { poll_ms } => worker(root, poll_ms).await,
        Command::BenchRender { pptx, output } => bench_render(&pptx, &output),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let a = app(tmp.path().to_path_buf()).unwrap();
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
        let unsupported = b"--pptx-test-boundary\r\nContent-Disposition: form-data; name=\"file\"; filename=\"later.pdf\"\r\nContent-Type: application/pdf\r\n\r\nplaceholder\r\n--pptx-test-boundary--\r\n";
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
        assert!(untitled.contains("status: draft"));
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
        assert!(slide.contains("status: draft"));
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
        assert!(stale.exists());
        assert!(a.root.join("attempts/recovery/2/bundle.zip").exists());
    }
    #[tokio::test]
    async fn malformed_pptx_fails_without_publishing_and_missing_view_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let a = app(tmp.path().to_path_buf()).unwrap();
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
    /// Presentaciones reales del curso. Por defecto se usan las copias reducidas de
    /// `examples/pptx/corpus` (XML íntegro, imágenes mínimas); `OKF_CORPUS_DIR` permite
    /// apuntar a los originales.
    fn corpus(name: &str) -> (Vec<Slide>, Vec<Issue>, String) {
        let dir = std::env::var("OKF_CORPUS_DIR").unwrap_or_else(|_| "examples/pptx/corpus".into());
        let path = format!("{dir}/{name}.pptx");
        let bytes = fs::read(&path).unwrap_or_else(|e| panic!("no se pudo leer {path}: {e}"));
        let tmp = tempfile::tempdir().unwrap();
        let Extraction { slides, issues, .. } = extract(&bytes, tmp.path()).unwrap();
        let md = slides
            .iter()
            .map(|s| markdown_slide(s, name, "2026-01-01T00:00:00Z"))
            .collect();
        (slides, issues, md)
    }
    fn text_lines(s: &Slide) -> Vec<String> {
        s.items
            .iter()
            .filter_map(|i| match i {
                Item::Text(p) => Some(p.iter().map(|p| p.plain.clone()).collect::<Vec<_>>()),
                _ => None,
            })
            .flatten()
            .collect()
    }
    #[test]
    fn corpus_s1_infers_titles_labels_links_and_drops_icons() {
        let (slides, issues, md) = corpus("s1");
        assert_eq!(slides.len(), 29);
        assert!(slides.iter().all(|s| s.title.is_some() && s.title_inferred));
        assert!(issues.iter().all(|i| !i.text.contains("sin título")));
        // Títulos inferidos e iconos se informan, pero no son pérdidas.
        assert!(issues.iter().all(|i| i.severity == Severity::Note));
        let s18 = &slides[17];
        assert_eq!(s18.title.as_deref(), Some("Localizar el fallo"));
        assert_eq!(s18.kicker.as_deref(), Some("ARQUITECTURA Y DIAGNÓSTICO"));
        assert_eq!(slides[4].visible_label.as_deref(), Some("05"));
        assert!(
            slides
                .iter()
                .all(|s| text_lines(s).iter().all(|t| !t.contains(" / 29")))
        );
        // Fragmentos de un mismo párrafo se unen sin espacios añadidos.
        assert!(!md.contains("Determinismo  hasta"));
        // Las notas conservan sus bloques y no arrastran el número de diapositiva.
        let notes = &slides[1].notes;
        assert_eq!(notes.first().map(String::as_str), Some("GUÍA DOCENTE"));
        assert!(notes.iter().all(|n| n.trim() != "2" && !n.ends_with(" 2")));
        assert!(md.contains("](https://misw4411-aplicaciones-basadas-en-llms.github.io/llm-retrieval-engineering-course/)"));
        assert!(slides.iter().map(|s| s.decorative_images).sum::<usize>() >= 80);
        assert!(slides.iter().map(|s| s.image_paths.len()).sum::<usize>() <= 20);
        assert!(md.contains("- \\+ consulta"));
    }
    #[test]
    fn corpus_s2_s3_keep_entities_and_clean_notes() {
        let (s2, issues2, md2) = corpus("s2");
        let (s3, _, md3) = corpus("s3");
        assert_eq!((s2.len(), s3.len()), (27, 34));
        assert_eq!(s2[6].title.as_deref(), Some("Chunk Size & Chunk Overlap"));
        assert!(s2.iter().chain(&s3).all(|s| !s.title_inferred));
        for md in [&md2, &md3] {
            assert!(!md.contains("‹#›"));
        }
        // Diapositiva solo visual: la imagen se enlaza y las notas se conservan.
        let s3_4 = &s3[3];
        assert!(s3_4.title.is_none());
        assert!(matches!(s3_4.items.as_slice(), [Item::Image { .. }]));
        assert!(s3_4.notes.iter().any(|n| n.contains("vecino más cercano")));
        assert!(s3_4.visual_only && s2[6].visual_only);
        assert_eq!(s2.iter().filter(|s| s.visual_only).count(), 15);
        // Animación con imagen a pantalla completa sobre el texto (s3, diapositiva 21).
        assert!(s3[20].view_hides_text && !s3[20].visual_only);
        assert!(issues2.iter().any(|i| i.severity == Severity::Note
            && i.text.contains("Texto del diseño")
            && i.text.contains("MISO – Maestría en Ingeniería de Software")));
    }
    #[test]
    fn equations_comments_and_alt_text_are_kept_without_duplicates() {
        let bytes = example("rich");
        let tmp = tempfile::tempdir().unwrap();
        let extraction = extract(&bytes, tmp.path()).unwrap();
        let s = &extraction.slides[0];
        let md = markdown_slide(s, "rich.pptx", "2026-01-01T00:00:00Z");
        assert!(md.contains("- `E=mc^2+a/b`"), "{md}");
        // La imagen de mc:Fallback representa la misma ecuación: no se adjunta.
        assert!(s.image_paths.is_empty());
        assert!(md.contains("> Texto alternativo: Ciclo: consulta, recuperación y respuesta"));
        assert!(md.contains("> Texto alternativo: Flecha de la consulta hacia el índice"));
        assert!(md.contains("- **Revisora** · 2026-10-30: Revisar la cifra de la ecuación"));
        let notes: Vec<&str> = extraction
            .issues
            .iter()
            .filter(|i| i.severity == Severity::Note)
            .map(|i| i.text.as_str())
            .collect();
        assert!(notes.iter().any(|n| n.contains("ecuación(es) convertidas")));
        assert!(
            notes
                .iter()
                .any(|n| n.contains("1 comentario(s) de revisión"))
        );
        let (zip, state) = build_bundle(
            &bytes,
            "rich.pptx",
            &hex(&Sha256::digest(&bytes)),
            &extraction,
            tmp.path(),
        )
        .unwrap();
        assert_eq!(state, "complete");
        validate_bundle(&zip).unwrap();
    }
    #[test]
    fn modern_comments_keep_authors_and_replies() {
        let authors = HashMap::from([("{A1}".to_string(), "Ana".to_string())]);
        let xml = r#"<p188:cmLst xmlns:p188="p188" xmlns:a="a"><p188:cm id="{C1}" authorId="{A1}" created="2026-10-29T09:15:00.000"><p188:replyLst><p188:reply id="{R1}" authorId="{B2}" created="2026-10-30T11:00:00.000"><p188:txBody><a:p><a:r><a:t>Hecho</a:t></a:r></a:p></p188:txBody></p188:reply></p188:replyLst><p188:txBody><a:p><a:r><a:t>Falta la fuente &amp; la fecha</a:t></a:r></a:p></p188:txBody></p188:cm></p188:cmLst>"#;
        let comments = parse_comments(xml, &authors).unwrap();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].author.as_deref(), Some("Ana"));
        assert_eq!(comments[0].date.as_deref(), Some("2026-10-29"));
        assert_eq!(comments[0].text.trim(), "Falta la fuente & la fecha");
        assert_eq!(comments[0].replies[0].author.as_deref(), Some("{B2}"));
        assert_eq!(comments[0].replies[0].text.trim(), "Hecho");
    }
    #[test]
    fn chart_cache_values_are_recoverable_from_chart_xml() {
        let xml = r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:v>Revenue</c:v><c:v>North</c:v><c:v>12</c:v></c:chartSpace>"#;
        let text = chart_text(xml);
        assert!(text.contains("Revenue"));
        assert!(text.contains("North"));
        assert!(text.contains("12"));
    }
}
