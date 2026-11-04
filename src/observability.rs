//! Salud y métricas persistidas, sin un cliente Prometheus adicional (plan 2, C7).
use crate::{App, now, output::Stages};
use anyhow::Result;
use axum::{
    Json,
    extract::State,
    http::{StatusCode, header},
    response::IntoResponse,
};
use sqlx::Row;
use std::{fmt::Write, time::Duration};

pub(crate) struct Heartbeat {
    task: tokio::task::JoinHandle<()>,
    app: App,
    id: String,
}
impl Heartbeat {
    pub(crate) async fn start(a: &App) -> Result<Self> {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO worker_heartbeats(id,seen_at) VALUES($1,$2)")
            .bind(&id)
            .bind(now())
            .execute(&a.db)
            .await?;
        sqlx::query("DELETE FROM worker_heartbeats WHERE seen_at < $1")
            .bind(now().saturating_sub(a.cfg.worker_health_s.saturating_mul(10)))
            .execute(&a.db)
            .await?;
        let (app, worker_id) = (a.clone(), id.clone());
        let task = tokio::spawn(async move {
            let interval = Duration::from_secs((app.cfg.worker_health_s / 3).max(1) as u64);
            loop {
                tokio::time::sleep(interval).await;
                if let Err(error) =
                    sqlx::query("UPDATE worker_heartbeats SET seen_at=$2 WHERE id=$1")
                        .bind(&worker_id)
                        .bind(now())
                        .execute(&app.db)
                        .await
                {
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
        if let Err(error) = sqlx::query("DELETE FROM worker_heartbeats WHERE id=$1")
            .bind(&self.id)
            .execute(&self.app.db)
            .await
        {
            tracing::warn!(%error, "no se pudo retirar el latido del worker");
        }
    }
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub(crate) struct Health {
    /// `ok` si las tres comprobaciones pasan; `unhealthy` en otro caso.
    #[schema(examples("ok"))]
    status: &'static str,
    /// PostgreSQL responde a la consulta de latidos.
    database: bool,
    /// El volumen cumple el espacio mínimo configurado.
    storage: bool,
    /// Existe un latido de worker dentro del plazo permitido.
    worker: bool,
    /// Segundos desde el último latido, o null si no está disponible.
    #[schema(required)]
    worker_age_s: Option<i64>,
}
/// Comprobar la salud del servicio
///
/// Comprueba PostgreSQL, espacio libre y latido del worker. Si solo está arrancada
/// la API y no hay un worker activo, devuelve `503` con `worker: false`.
#[utoipa::path(
    get, path = "/health", tag = "Operación", operation_id = "consultar_salud",
    responses(
        (status = 200, description = "Servicio disponible", body = Health),
        (status = 503, description = "Base de datos, almacenamiento o worker no disponible", body = Health)
    )
)]
pub(crate) async fn health(State(a): State<App>) -> impl IntoResponse {
    let latest = sqlx::query("SELECT MAX(seen_at) FROM worker_heartbeats")
        .fetch_one(&a.db)
        .await
        .and_then(|r| r.try_get::<Option<i64>, _>(0));
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
    sqlx::query(
        "INSERT INTO conversion_metrics(format,result,count,read_ms,assemble_ms,\
        validate_ms,total_ms,peak_mb,retries) VALUES($1,$2,1,$3,$4,$5,$6,$7,$8)
       \
         ON CONFLICT(format,result) DO UPDATE SET count=conversion_metrics.count+1,\
        read_ms=conversion_metrics.read_ms+excluded.read_ms,\
        assemble_ms=conversion_metrics.assemble_ms+excluded.assemble_ms,\
        validate_ms=conversion_metrics.validate_ms+excluded.validate_ms,\
        total_ms=conversion_metrics.total_ms+excluded.total_ms,\
        peak_mb=GREATEST(conversion_metrics.peak_mb,excluded.peak_mb),\
        retries=conversion_metrics.retries+excluded.retries",
    )
    .bind(m.format)
    .bind(m.result)
    .bind(m.stages.read_ms as i64)
    .bind(m.stages.assemble_ms as i64)
    .bind(m.stages.validate_ms as i64)
    .bind(m.total_ms as i64)
    .bind(m.peak_mb as i64)
    .bind(i64::from(m.retry))
    .execute(&a.db)
    .await?;
    Ok(())
}

/// Consultar las métricas de Prometheus
///
/// Contadores de trabajos e intentos, duraciones y memoria máxima por formato.
/// Se conservan en PostgreSQL; no incluyen contenido ni identificadores de documentos.
#[utoipa::path(
    get, path = "/metrics", tag = "Operación", operation_id = "consultar_metricas",
    responses(
        (status = 200, description = "Métricas en formato de texto Prometheus 0.0.4", body = String, content_type = "text/plain"),
        (status = 500, description = "Error al consultar PostgreSQL", body = String, content_type = "text/plain")
    )
)]
pub(crate) async fn metrics(
    State(a): State<App>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let db = &a.db;
    let mut out = String::from("# HELP okf_jobs Trabajos por estado.\n# TYPE okf_jobs gauge\n");
    for state in [
        "queued",
        "processing",
        "complete",
        "partial",
        "failed",
        "expired",
    ] {
        let count: i64 = sqlx::query("SELECT COUNT(*) FROM jobs WHERE status=$1")
            .bind(state)
            .fetch_one(db)
            .await
            .and_then(|r| r.try_get(0))
            .map_err(crate::internal)?;
        writeln!(out, "okf_jobs{{status=\"{state}\"}} {count}").unwrap();
    }
    out.push_str("# HELP okf_conversions_total Intentos terminados por formato y resultado.\n# TYPE okf_conversions_total counter\n");
    let rows: Vec<(String, String, i64)> =
        sqlx::query_as("SELECT format,result,count FROM conversion_metrics ORDER BY format,result")
            .fetch_all(db)
            .await
            .map_err(crate::internal)?;
    for (format, result, count) in rows {
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
        let (count, total, peak, retries): (i64, i64, i64, i64) = sqlx::query(
            "SELECT COALESCE(SUM(count),0)::bigint,COALESCE(SUM(total_ms),0)::bigint,\
            COALESCE(MAX(peak_mb),0),COALESCE(SUM(retries),0)::bigint FROM \
            conversion_metrics WHERE format=$1",
        )
        .bind(format)
        .fetch_one(db)
        .await
        .and_then(|r| Ok((r.try_get(0)?, r.try_get(1)?, r.try_get(2)?, r.try_get(3)?)))
        .map_err(crate::internal)?;
        writeln!(out,"okf_conversion_duration_seconds_sum{{format=\"{format}\"}} {:.3}\nokf_conversion_duration_seconds_count{{format=\"{format}\"}} {count}\nokf_conversion_peak_memory_bytes{{format=\"{format}\"}} {}\nokf_retries_total{{format=\"{format}\"}} {retries}",total as f64/1000.0,peak.saturating_mul(1024*1024)).unwrap();
        let (n, read, assemble, validate): (i64, i64, i64, i64) = sqlx::query(
            "SELECT COALESCE(SUM(count),0)::bigint,COALESCE(SUM(read_ms),0)::bigint,\
            COALESCE(SUM(assemble_ms),0)::bigint,COALESCE(SUM(validate_ms),0)::bigint FROM \
            conversion_metrics WHERE format=$1 AND result IN ('complete','partial')",
        )
        .bind(format)
        .fetch_one(db)
        .await
        .and_then(|r| Ok((r.try_get(0)?, r.try_get(1)?, r.try_get(2)?, r.try_get(3)?)))
        .map_err(crate::internal)?;
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
        let mut a = crate::app(tmp.path().to_owned()).await.unwrap();
        assert_eq!(get(&a, "/health").await.0, StatusCode::SERVICE_UNAVAILABLE);
        let heartbeat = Heartbeat::start(&a).await.unwrap();
        assert_eq!(get(&a, "/health").await.0, StatusCode::OK);
        sqlx::query("UPDATE worker_heartbeats SET seen_at=$1")
            .bind(now() - a.cfg.worker_health_s - 1)
            .execute(&a.db)
            .await
            .unwrap();
        assert_eq!(get(&a, "/health").await.0, StatusCode::SERVICE_UNAVAILABLE);
        sqlx::query("UPDATE worker_heartbeats SET seen_at=$1")
            .bind(now())
            .execute(&a.db)
            .await
            .unwrap();
        std::sync::Arc::make_mut(&mut a.cfg).min_free_mb = u64::MAX;
        assert_eq!(get(&a, "/health").await.0, StatusCode::SERVICE_UNAVAILABLE);
        std::sync::Arc::make_mut(&mut a.cfg).min_free_mb = 0;
        heartbeat.stop().await;
        assert_eq!(get(&a, "/health").await.0, StatusCode::SERVICE_UNAVAILABLE);
        sqlx::query("DROP TABLE worker_heartbeats")
            .execute(&a.db)
            .await
            .unwrap();
        assert!(get(&a, "/health").await.1.contains("\"database\":false"));
    }
    #[tokio::test]
    async fn metrics_are_persistent_and_include_real_conversion_stages() {
        let tmp = tempfile::tempdir().unwrap();
        let a = crate::app(tmp.path().to_owned()).await.unwrap();
        std::fs::copy(
            "examples/docx/breve.docx",
            tmp.path().join("originals/a.docx"),
        )
        .unwrap();
        sqlx::query(
            "INSERT INTO jobs(id,filename,original,sha256,status,created_at,attempt) \
            VALUES('a','a.docx','originals/a.docx','-','queued',0,1)",
        )
        .execute(&a.db)
        .await
        .unwrap();
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
        let reopened = crate::app(tmp.path().to_owned()).await.unwrap();
        assert_eq!(body, get(&reopened, "/metrics").await.1);
    }
}
