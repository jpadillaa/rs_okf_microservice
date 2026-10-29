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

#[derive(Clone, Default)]
struct Slide {
    idx: usize,
    slide_id: Option<u32>,
    title: Option<String>,
    texts: Vec<String>,
    tables: Vec<String>,
    notes: String,
    image_paths: Vec<String>,
    chart_paths: Vec<String>,
    visible_label: Option<String>,
    hidden: bool,
}
fn xml_text(xml: &str) -> String {
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut out = String::new();
    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Text(t)) => {
                if let Ok(s) = t.decode() {
                    if !s.trim().is_empty() {
                        if !out.is_empty() {
                            out.push(' ')
                        }
                        out.push_str(&s);
                    }
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
fn extract(bytes: &[u8], attempt: &FsPath) -> Result<(Vec<Slide>, Vec<String>)> {
    let mut z = ZipArchive::new(Cursor::new(bytes))?;
    enforce_package_limits(&mut z)?;
    let pres = String::from_utf8(read_entry(&mut z, "ppt/presentation.xml")?)?;
    let rels = String::from_utf8(read_entry(&mut z, "ppt/_rels/presentation.xml.rels")?)?;
    let relmap = relationship_map(&rels);
    let mut order = Vec::<(u32, String, bool)>::new();
    let mut rd = quick_xml::Reader::from_str(&pres);
    loop {
        match rd.read_event() {
            Ok(quick_xml::events::Event::Start(e)) | Ok(quick_xml::events::Event::Empty(e))
                if e.local_name().as_ref() == b"sldId" =>
            {
                if let (Some(id), Some(rid)) = (
                    attr(&e, b"id").and_then(|x| x.parse().ok()),
                    e.attributes()
                        .flatten()
                        .find(|a| a.key.as_ref().ends_with(b":id"))
                        .and_then(|a| String::from_utf8(a.value.into_owned()).ok()),
                ) {
                    if let Some(target) = relmap.get(&rid) {
                        order.push((
                            id,
                            resolve("ppt/presentation.xml", target),
                            attr(&e, b"show").as_deref() == Some("0"),
                        ));
                    }
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
    let mut slides = Vec::new();
    let mut all_issues = Vec::new();
    for i in 0..z.len() {
        let name = z.by_index(i)?.name().to_string().to_ascii_lowercase();
        if name.starts_with("ppt/embeddings/") {
            all_issues.push(format!("Objeto incrustado no interpretado: {name}"));
        }
        if name.starts_with("ppt/media/")
            && [".mp3", ".wav", ".m4a", ".mp4", ".mov", ".avi"]
                .iter()
                .any(|ext| name.ends_with(ext))
        {
            all_issues.push(format!(
                "Audio o video conservado solo en el original: {name}"
            ));
        }
    }
    fs::create_dir_all(attempt.join("assets"))?;
    for (index, (slide_id, path, hidden)) in order.into_iter().enumerate() {
        let raw = String::from_utf8(read_entry(&mut z, &path)?)?;
        if raw.contains("<p:timing") {
            all_issues.push(format!(
                "Diapositiva {} contiene animaciones no representadas",
                index + 1
            ));
        }
        let relpath = path
            .rsplit_once('/')
            .map(|(p, _)| format!("{p}/_rels/{}.rels", path.rsplit('/').next().unwrap()))
            .unwrap_or_default();
        let sr = if !relpath.is_empty() {
            String::from_utf8(read_entry(&mut z, &relpath).unwrap_or_default()).unwrap_or_default()
        } else {
            String::new()
        };
        let smap = relationship_map(&sr);
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
            ..Slide::default()
        };
        s.image_paths = smap
            .values()
            .filter(|target| {
                matches!(
                    FsPath::new(target)
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(str::to_ascii_lowercase)
                        .as_deref(),
                    Some("png" | "jpg" | "jpeg" | "gif" | "bmp" | "tif" | "tiff" | "svg" | "webp")
                )
            })
            .map(|target| resolve(&path, target))
            .collect();
        // Text is read from shape text bodies; table cell text remains separately tabulated.
        let mut rr = quick_xml::Reader::from_str(&raw);
        let mut in_table = false;
        let mut in_shape = false;
        let mut shape_title = false;
        let mut shape_display_label = false;
        let mut table_rows: Vec<Vec<String>> = Vec::new();
        let mut table_row: Vec<String> = Vec::new();
        let mut table_cell = String::new();
        let mut in_cell = false;
        let mut shape_text = String::new();
        loop {
            match rr.read_event() {
                Ok(quick_xml::events::Event::Start(e)) if e.local_name().as_ref() == b"sp" => {
                    in_shape = true;
                    shape_title = false;
                    shape_display_label = false;
                    shape_text.clear()
                }
                Ok(quick_xml::events::Event::Empty(e)) if e.local_name().as_ref() == b"ph" => {
                    if attr(&e, b"type").as_deref() == Some("title")
                        || attr(&e, b"type").as_deref() == Some("ctrTitle")
                    {
                        shape_title = true
                    }
                    if attr(&e, b"type").as_deref() == Some("sldNum") {
                        shape_display_label = true
                    }
                }
                Ok(quick_xml::events::Event::Start(e)) if e.local_name().as_ref() == b"ph" => {
                    if attr(&e, b"type").as_deref() == Some("title")
                        || attr(&e, b"type").as_deref() == Some("ctrTitle")
                    {
                        shape_title = true
                    }
                    if attr(&e, b"type").as_deref() == Some("sldNum") {
                        shape_display_label = true
                    }
                }
                Ok(quick_xml::events::Event::Start(e)) if e.local_name().as_ref() == b"tbl" => {
                    in_table = true;
                    table_rows.clear()
                }
                Ok(quick_xml::events::Event::Start(e))
                    if e.local_name().as_ref() == b"tr" && in_table =>
                {
                    table_row.clear()
                }
                Ok(quick_xml::events::Event::Start(e))
                    if e.local_name().as_ref() == b"tc" && in_table =>
                {
                    in_cell = true;
                    table_cell.clear()
                }
                Ok(quick_xml::events::Event::End(e))
                    if e.local_name().as_ref() == b"tc" && in_table =>
                {
                    table_row.push(table_cell.trim().to_string());
                    in_cell = false
                }
                Ok(quick_xml::events::Event::End(e))
                    if e.local_name().as_ref() == b"tr" && in_table =>
                {
                    table_rows.push(table_row.clone())
                }
                Ok(quick_xml::events::Event::End(e)) if e.local_name().as_ref() == b"tbl" => {
                    in_table = false;
                    if !table_rows.is_empty() {
                        let cols = table_rows.iter().map(Vec::len).max().unwrap_or(0);
                        let mut md = String::new();
                        for (row_index, row) in table_rows.iter().enumerate() {
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
                        s.tables.push(md)
                    }
                }
                Ok(quick_xml::events::Event::Text(t)) => {
                    if let Ok(v) = t.decode() {
                        let v = v.into_owned();
                        if !v.trim().is_empty() {
                            if in_table {
                                if in_cell {
                                    if !table_cell.is_empty() {
                                        table_cell.push(' ')
                                    }
                                    table_cell.push_str(&v)
                                }
                            } else if in_shape {
                                if !shape_text.is_empty() {
                                    shape_text.push(' ')
                                }
                                shape_text.push_str(&v)
                            }
                        }
                    }
                }
                Ok(quick_xml::events::Event::End(e)) if e.local_name().as_ref() == b"sp" => {
                    if !shape_text.trim().is_empty() {
                        let t = shape_text.trim().to_owned();
                        if shape_title {
                            s.title = Some(t.clone())
                        }
                        if shape_display_label && t.chars().all(|c| c.is_ascii_digit()) {
                            s.visible_label = Some(t.clone())
                        }
                        s.texts.push(t);
                    }
                    shape_text.clear();
                    in_shape = false
                }
                Ok(quick_xml::events::Event::Empty(e)) if e.local_name().as_ref() == b"blip" => {
                    if let Some(rid) = e
                        .attributes()
                        .flatten()
                        .find(|a| a.key.as_ref().ends_with(b":embed"))
                        .and_then(|a| String::from_utf8(a.value.into_owned()).ok())
                    {
                        if let Some(t) = smap.get(&rid) {
                            s.image_paths.push(resolve(&path, t));
                        }
                    }
                }
                Ok(quick_xml::events::Event::Empty(e))
                    if e.local_name().as_ref() == b"graphicData" =>
                {
                    if let Some(uri) = attr(&e, b"uri") {
                        if uri.contains("chart") { /* relationship resolved by chart frame below */
                        }
                    }
                }
                Ok(quick_xml::events::Event::Eof) => break,
                Err(e) => return Err(e.into()),
                _ => {}
            }
        }
        s.image_paths.sort();
        s.image_paths.dedup();
        // Resolve table/chart relationships and materialize referenced picture bytes.
        for i in 0..s.image_paths.len() {
            let rp = s.image_paths[i].clone();
            let data = read_entry(&mut z, &rp)?;
            let ext = FsPath::new(&rp)
                .extension()
                .and_then(|x| x.to_str())
                .unwrap_or("bin");
            let name = format!("assets/slide-{:03}-image-{:02}.{ext}", index + 1, i + 1);
            fs::write(attempt.join(&name), data)?;
            s.image_paths[i] = name;
        }
        // Notes are a separate OOXML part related to this slide.
        for target in smap.values() {
            if target.contains("notesSlides/") {
                let np = resolve(&path, target);
                if let Ok(b) = read_entry(&mut z, &np) {
                    s.notes = xml_text(std::str::from_utf8(&b)?);
                }
            }
            if target.contains("charts/") {
                let cp = resolve(&path, target);
                s.chart_paths.push(cp.clone());
                if let Ok(b) = read_entry(&mut z, &cp) {
                    let text = chart_text(std::str::from_utf8(&b)?);
                    if !text.is_empty() {
                        s.texts.push(format!("Datos del gráfico: {text}"));
                    }
                }
            }
        }
        if s.title.is_none() {
            all_issues.push(format!("Diapositiva {} sin título explícito", index + 1));
        }
        slides.push(s);
    }
    Ok((slides, all_issues))
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
fn markdown_slide(s: &Slide, hash: &str, instant: &str, assets: &[String]) -> String {
    let mut out = format!(
        "---\ntype: Slide\ntitle: {}\ndescription: {}\nsources:\n  - id: original\n    resource: /references/original.pptx\ngenerated:\n  by: process:document-conversion\n  at: {}\nsource_spans:\n  - id: slide-{}\n    source_id: original\n    kind: pptx_slide\n    slide_index: {}\n",
        yaml(s.title.as_deref().unwrap_or("Diapositiva sin título")),
        yaml(&format!("Contenido extraído de la diapositiva {}.", s.idx)),
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
    if s.title.is_none() {
        out.push_str("status: draft\n");
    }
    out.push_str("---\n\n");
    if let Some(title) = &s.title {
        out.push_str(&format!("# {}\n\n", title));
    } else {
        out.push_str(&format!(
            "# Diapositiva {} (sin título explícito)\n\n",
            s.idx
        ));
    }
    if s.hidden {
        out.push_str("> Diapositiva oculta en la presentación original.\n\n");
    }
    out.push_str("## Texto\n\n");
    for t in &s.texts {
        out.push_str(&format!("- {}\n", t));
    }
    if s.texts.is_empty() {
        out.push_str("Sin texto extraído.\n");
    }
    if !s.tables.is_empty() {
        out.push_str("\n## Tablas\n\n");
        for (i, t) in s.tables.iter().enumerate() {
            out.push_str(&format!("Tabla {}\n\n{}\n", i + 1, t));
        }
    }
    if !assets.is_empty() {
        out.push_str("## Imágenes\n\n");
        for a in assets {
            out.push_str(&format!("![Imagen de diapositiva {}](../{})\n\n", s.idx, a));
        }
    }
    if !s.chart_paths.is_empty() {
        out.push_str("## Gráficos\n\n");
        out.push_str("Datos recuperados del XML del gráfico, cuando estuvieron disponibles.\n\n");
    }
    if !s.notes.is_empty() {
        out.push_str("## Notas del presentador\n\n");
        out.push_str(s.notes.trim());
        out.push_str("\n");
    }
    let _ = hash;
    out
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
    slides: &[Slide],
    issues: &[String],
    attempt: &FsPath,
) -> Result<(PathBuf, String)> {
    let started = Instant::now();
    let mut ppt = Presentation::open(bytes)?;
    let font = render_font()?;
    ppt.register_font("Arial", false, false, &font)?;
    ppt.register_font("Arial", true, false, &font)?;
    if ppt.slides().len() != slides.len() {
        bail!("cantidad de vistas no coincide con diapositivas")
    }
    let generated = chrono_now();
    let mut allissues = issues.to_vec();
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
            allissues.push(format!(
                "Diapositiva {}: renderizador omitió {} imagen(es)",
                s.idx, rendered.skipped_images
            ));
        }
        let assets = s.image_paths.clone();
        let mut md = markdown_slide(s, hash, &generated, &assets);
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
    let state = if allissues.is_empty() {
        "complete"
    } else {
        "partial"
    };
    let slide_links = (1..=slides.len())
        .map(|i| format!("- [Diapositiva {i}](contenido/slide-{i:03}.md)\n"))
        .collect::<String>();
    let content_slide_links = (1..=slides.len())
        .map(|i| format!("- [Diapositiva {i}](slide-{i:03}.md)\n"))
        .collect::<String>();
    let index = format!(
        "---\nokf_version: \"0.2\"\n---\n\n# Bundle OKF\n\n- [Documento fuente](documento.md)\n- [Índice de diapositivas](contenido/index.md)\n- [Informe de conversión](conversion-report.md)\n\n## Diapositivas\n\n{slide_links}"
    );
    let contents_index = format!("# Contenido\n\n## Diapositivas\n\n{content_slide_links}");
    let doc = format!(
        "---\ntype: Source Document\ntitle: {}\ndescription: Presentación fuente convertida a conceptos OKF por diapositiva.\nsources:\n  - id: original\n    resource: /{orig}\ngenerated:\n  by: process:document-conversion\n  at: {}\nsource_file:\n  path: {}\n  sha256: {}\n---\n\n# {}\n\nPresentación de {} diapositivas.\n",
        yaml(filename),
        yaml(&generated),
        yaml(&orig),
        hash,
        filename,
        slides.len()
    );
    let issue_md = if allissues.is_empty() {
        "Sin incidencias editoriales detectadas.".to_string()
    } else {
        allissues.iter().map(|i| format!("- {i}\n")).collect()
    };
    let report = format!(
        "---\ntype: Conversion Report\ntitle: Informe de conversión\ngenerated:\n  by: process:document-conversion\n  at: {}\nstatus: {}\nformat: pptx\nsha256: {}\n---\n\n# Informe de conversión PPTX\n\n- Estado: `{}`\n- Diapositivas inventariadas: {}\n- Diapositivas representadas con concepto y vista: {}\n- Vistas adjuntas: {}\n- Imágenes adjuntas: {}\n- Vistas: BetterOffice PPTX Raster (Rust)\n- Duración de ensamblado/renderizado: {} ms\n\n## Incidencias\n\n{}\n",
        yaml(&generated),
        state,
        hash,
        state,
        slides.len(),
        slides.len(),
        slides.len(),
        slides.iter().map(|s| s.image_paths.len()).sum::<usize>(),
        started.elapsed().as_millis(),
        issue_md
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
            if let Some(end) = md[s..].find(')') {
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
        let (slides, issues) = extract(&bytes, &attempt)?;
        if slides.is_empty() {
            bail!("sin diapositivas")
        };
        let (bundle, state) = build_bundle(&bytes, &filename, &hash, &slides, &issues, &attempt)?;
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
        assert!(content_index.contains("slide-001.md"));
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
        assert_eq!(state, "partial");
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
        assert!(slide.contains("## Tablas"));
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
        assert!(report.contains("sin título"));
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
        let (slides, issues) = extract(&simple, &attempt).unwrap();
        let good = build_bundle(
            &simple,
            "simple.pptx",
            &hex(&Sha256::digest(&simple)),
            &slides,
            &issues,
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
    #[test]
    fn chart_cache_values_are_recoverable_from_chart_xml() {
        let xml = r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:v>Revenue</c:v><c:v>North</c:v><c:v>12</c:v></c:chartSpace>"#;
        let text = chart_text(xml);
        assert!(text.contains("Revenue"));
        assert!(text.contains("North"));
        assert!(text.contains("12"));
    }
}
