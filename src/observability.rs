//! Salud y métricas persistidas, sin un cliente Prometheus adicional (plan 2, C7).
use crate::{App, now, output::Stages};
use anyhow::Result;
use axum::{
    Json,
    extract::State,
    http::{StatusCode, header},
    response::IntoResponse,
};
use rusqlite::{Connection, params};
use std::{fmt::Write, time::Duration};

pub(crate) fn init(db: &Connection) -> Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS worker_heartbeats(id TEXT PRIMARY KEY,seen_at INTEGER NOT NULL);
        CREATE TABLE IF NOT EXISTS conversion_metrics(format TEXT NOT NULL,result TEXT NOT NULL,count INTEGER NOT NULL,read_ms INTEGER NOT NULL,assemble_ms INTEGER NOT NULL,validate_ms INTEGER NOT NULL,total_ms INTEGER NOT NULL,peak_mb INTEGER NOT NULL,retries INTEGER NOT NULL,PRIMARY KEY(format,result));")?;
    Ok(())
}

pub(crate) struct Heartbeat {
    task: tokio::task::JoinHandle<()>,
    app: App,
    id: String,
}
impl Heartbeat {
    pub(crate) async fn start(a: &App) -> Result<Self> {
        let id = uuid::Uuid::new_v4().to_string();
        a.db.lock().await.execute(
            "INSERT INTO worker_heartbeats(id,seen_at) VALUES(?1,?2)",
            params![id, now()],
        )?;
        a.db.lock().await.execute(
            "DELETE FROM worker_heartbeats WHERE seen_at < ?1",
            [now().saturating_sub(a.cfg.worker_health_s.saturating_mul(10))],
        )?;
        let (app, worker_id) = (a.clone(), id.clone());
        let task = tokio::spawn(async move {
            let interval = Duration::from_secs((app.cfg.worker_health_s / 3).max(1) as u64);
            loop {
                tokio::time::sleep(interval).await;
                if let Err(error) = app.db.lock().await.execute(
                    "UPDATE worker_heartbeats SET seen_at=?2 WHERE id=?1",
                    params![worker_id, now()],
                ) {
                    tracing::warn!(%error, "no se pudo registrar el latido del worker");
                }
            }
        });
        Ok(Self {
            task,
            app: a.clone(),
            id,
        })
    }
    pub(crate) async fn stop(self) {
        self.task.abort();
        let _ = self.task.await;
        if let Err(error) = self
            .app
            .db
            .lock()
            .await
            .execute("DELETE FROM worker_heartbeats WHERE id=?1", [self.id])
        {
            tracing::warn!(%error, "no se pudo retirar el latido del worker");
        }
    }
}

#[derive(serde::Serialize)]
pub(crate) struct Health {
    status: &'static str,
    database: bool,
    storage: bool,
    worker: bool,
    worker_age_s: Option<i64>,
}
pub(crate) async fn health(State(a): State<App>) -> impl IntoResponse {
    let latest =
        a.db.lock()
            .await
            .query_row("SELECT MAX(seen_at) FROM worker_heartbeats", [], |r| {
                r.get::<_, Option<i64>>(0)
            });
    let database = latest.is_ok();
    let age = latest
        .ok()
        .flatten()
        .map(|seen| now().saturating_sub(seen).max(0));
    let worker = age.is_some_and(|age| age <= a.cfg.worker_health_s);
    let storage = a.cfg.min_free_mb == 0
        || crate::free_space_mb(&a.root).is_ok_and(|free| free >= a.cfg.min_free_mb);
    let ok = database && storage && worker;
    (
        if ok {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(Health {
            status: if ok { "ok" } else { "unhealthy" },
            database,
            storage,
            worker,
            worker_age_s: age,
        }),
    )
}

#[derive(Clone, Copy)]
pub(crate) struct Attempt<'a> {
    pub format: &'a str,
    pub result: &'a str,
    pub stages: Stages,
    pub total_ms: u64,
    pub peak_mb: u64,
    pub retry: bool,
}
pub(crate) async fn record(a: &App, m: Attempt<'_>) -> Result<()> {
    a.db.lock().await.execute(
        "INSERT INTO conversion_metrics(format,result,count,read_ms,assemble_ms,validate_ms,total_ms,peak_mb,retries) VALUES(?1,?2,1,?3,?4,?5,?6,?7,?8)
        ON CONFLICT(format,result) DO UPDATE SET count=count+1,read_ms=read_ms+excluded.read_ms,assemble_ms=assemble_ms+excluded.assemble_ms,validate_ms=validate_ms+excluded.validate_ms,total_ms=total_ms+excluded.total_ms,peak_mb=MAX(peak_mb,excluded.peak_mb),retries=retries+excluded.retries",
        params![m.format,m.result,m.stages.read_ms,m.stages.assemble_ms,m.stages.validate_ms,m.total_ms,m.peak_mb,u64::from(m.retry)],
    )?;
    Ok(())
}

pub(crate) async fn metrics(
    State(a): State<App>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let db = a.db.lock().await;
    let mut out = String::from("# HELP okf_jobs Trabajos por estado.\n# TYPE okf_jobs gauge\n");
    for state in [
        "queued",
        "processing",
        "complete",
        "partial",
        "failed",
        "expired",
    ] {
        let count: u64 = db
            .query_row("SELECT COUNT(*) FROM jobs WHERE status=?1", [state], |r| {
                r.get(0)
            })
            .map_err(crate::internal)?;
        writeln!(out, "okf_jobs{{status=\"{state}\"}} {count}").unwrap();
    }
    out.push_str("# HELP okf_conversions_total Intentos terminados por formato y resultado.\n# TYPE okf_conversions_total counter\n");
    let mut query = db
        .prepare("SELECT format,result,count FROM conversion_metrics ORDER BY format,result")
        .map_err(crate::internal)?;
    let rows = query
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, u64>(2)?,
            ))
        })
        .map_err(crate::internal)?;
    for row in rows {
        let (format, result, count) = row.map_err(crate::internal)?;
        // Las etiquetas son enumeraciones internas, nunca nombres ni texto de documentos.
        if !matches!(
            format.as_str(),
            "pptx" | "docx" | "xlsx" | "pdf" | "desconocido"
        ) || !matches!(
            result.as_str(),
            "complete" | "partial" | "failed" | "transient_failure" | "lease_lost" | "interrupted"
        ) {
            continue;
        }
        writeln!(
            out,
            "okf_conversions_total{{format=\"{format}\",result=\"{result}\"}} {count}"
        )
        .unwrap();
    }
    out.push_str("# HELP okf_conversion_duration_seconds Duracion de los intentos, incluidos fallos.\n# TYPE okf_conversion_duration_seconds summary\n# HELP okf_conversion_stage_seconds Duracion de etapas en conversiones terminadas.\n# TYPE okf_conversion_stage_seconds summary\n# HELP okf_conversion_peak_memory_bytes Maximo historico residente por formato.\n# TYPE okf_conversion_peak_memory_bytes gauge\n# HELP okf_retries_total Intentos repetidos observados.\n# TYPE okf_retries_total counter\n");
    for format in ["pptx", "docx", "xlsx", "pdf", "desconocido"] {
        let (count,total,peak,retries): (u64,u64,u64,u64) = db.query_row("SELECT COALESCE(SUM(count),0),COALESCE(SUM(total_ms),0),COALESCE(MAX(peak_mb),0),COALESCE(SUM(retries),0) FROM conversion_metrics WHERE format=?1", [format], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).map_err(crate::internal)?;
        writeln!(out,"okf_conversion_duration_seconds_sum{{format=\"{format}\"}} {:.3}\nokf_conversion_duration_seconds_count{{format=\"{format}\"}} {count}\nokf_conversion_peak_memory_bytes{{format=\"{format}\"}} {}\nokf_retries_total{{format=\"{format}\"}} {retries}",total as f64/1000.0,peak.saturating_mul(1024*1024)).unwrap();
        let (n,read,assemble,validate): (u64,u64,u64,u64) = db.query_row("SELECT COALESCE(SUM(count),0),COALESCE(SUM(read_ms),0),COALESCE(SUM(assemble_ms),0),COALESCE(SUM(validate_ms),0) FROM conversion_metrics WHERE format=?1 AND result IN ('complete','partial')", [format], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).map_err(crate::internal)?;
        for (stage, ms) in [
            ("read", read),
            ("assemble", assemble),
            ("validate", validate),
        ] {
            writeln!(out,"okf_conversion_stage_seconds_sum{{format=\"{format}\",stage=\"{stage}\"}} {:.3}\nokf_conversion_stage_seconds_count{{format=\"{format}\",stage=\"{stage}\"}} {n}",ms as f64/1000.0).unwrap();
        }
    }
    Ok((
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        out,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    async fn get(a: &App, uri: &str) -> (StatusCode, String) {
        let r = crate::router(a.clone())
            .oneshot(
                axum::http::Request::builder()
                    .uri(uri)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        (
            r.status(),
            String::from_utf8(r.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap(),
        )
    }
    #[tokio::test]
    async fn health_requires_database_storage_and_a_recent_worker() {
        let tmp = tempfile::tempdir().unwrap();
        let mut a = crate::app(tmp.path().to_owned()).unwrap();
        assert_eq!(get(&a, "/health").await.0, StatusCode::SERVICE_UNAVAILABLE);
        let heartbeat = Heartbeat::start(&a).await.unwrap();
        assert_eq!(get(&a, "/health").await.0, StatusCode::OK);
        a.db.lock()
            .await
            .execute(
                "UPDATE worker_heartbeats SET seen_at=?1",
                [now() - a.cfg.worker_health_s - 1],
            )
            .unwrap();
        assert_eq!(get(&a, "/health").await.0, StatusCode::SERVICE_UNAVAILABLE);
        a.db.lock()
            .await
            .execute("UPDATE worker_heartbeats SET seen_at=?1", [now()])
            .unwrap();
        std::sync::Arc::make_mut(&mut a.cfg).min_free_mb = u64::MAX;
        assert_eq!(get(&a, "/health").await.0, StatusCode::SERVICE_UNAVAILABLE);
        std::sync::Arc::make_mut(&mut a.cfg).min_free_mb = 0;
        heartbeat.stop().await;
        assert_eq!(get(&a, "/health").await.0, StatusCode::SERVICE_UNAVAILABLE);
        a.db.lock()
            .await
            .execute("DROP TABLE worker_heartbeats", [])
            .unwrap();
        assert!(get(&a, "/health").await.1.contains("\"database\":false"));
    }
    #[tokio::test]
    async fn metrics_are_persistent_and_include_real_conversion_stages() {
        let tmp = tempfile::tempdir().unwrap();
        let a = crate::app(tmp.path().to_owned()).unwrap();
        std::fs::copy(
            "examples/docx/breve.docx",
            tmp.path().join("originals/a.docx"),
        )
        .unwrap();
        a.db.lock().await.execute("INSERT INTO jobs(id,filename,original,sha256,status,created_at,attempt) VALUES('a','a.docx','originals/a.docx','-','queued',0,1)",[]).unwrap();
        crate::worker::work_one(&a).await.unwrap();
        let (code, body) = get(&a, "/metrics").await;
        assert_eq!(code, StatusCode::OK);
        assert!(body.contains("okf_jobs{status=\"complete\"} 1"));
        assert!(body.contains("okf_conversions_total{format=\"docx\",result=\"complete\"} 1"));
        assert!(body.contains("okf_retries_total{format=\"docx\"} 1"));
        assert!(
            body.contains(
                "okf_conversion_stage_seconds_count{format=\"docx\",stage=\"validate\"} 1"
            )
        );
        assert!(!body.contains("a.docx"));
        let reopened = crate::app(tmp.path().to_owned()).unwrap();
        assert_eq!(body, get(&reopened, "/metrics").await.1);
    }
}
