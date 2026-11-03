//! Expiración optativa y limpieza reanudable (plan 2, C8).
use crate::{App, now};
use anyhow::{Result, bail};
use rusqlite::params;
use std::{fs, io::ErrorKind, path::Path, time::Duration};

fn remove(path: &Path, directory: bool) -> std::io::Result<()> {
    let result = if directory {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };
    match result {
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

/// Marca antes de borrar: ni una descarga ni la recuperación pueden republicar
/// archivos mientras la limpieza avanza. Los fallos se reintentan sin reactivar el job.
pub(crate) async fn sweep(a: &App) -> Result<usize> {
    if a.cfg.retention_days == 0 {
        return Ok(0);
    }
    let seconds = i64::try_from(a.cfg.retention_days.saturating_mul(86_400)).unwrap_or(i64::MAX);
    let cutoff = now().saturating_sub(seconds);
    let pending: Vec<(String, String)> = {
        let mut db = a.db.lock().await;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        tx.execute("UPDATE jobs SET status='expired',expired_at=?2,bundle=NULL,lease_until=NULL,retry_at=NULL WHERE status IN ('complete','partial','failed') AND finished_at<=?1", params![cutoff,now()])?;
        let rows = tx
            .prepare("SELECT id,original FROM jobs WHERE status='expired' AND purged_at IS NULL")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        tx.commit()?;
        rows
    };
    let mut purged = 0;
    for (id, original) in pending {
        let cleanup = || -> Result<()> {
            // La base contiene rutas internas, pero una fila dañada no debe salir del almacén.
            if !crate::valid_name(&id) {
                bail!("identificador inválido al expirar")
            }
            let parts: Vec<_> = Path::new(&original).components().collect();
            if parts.len() != 2
                || parts[0].as_os_str() != "originals"
                || !crate::validate::safe_name(&original)
            {
                bail!("ruta de original inválida al expirar")
            }
            remove(&a.root.join("attempts").join(&id), true)?;
            remove(&a.root.join("published").join(&id), true)?;
            remove(&a.root.join("published").join(format!(".{id}.tmp")), true)?;
            remove(&a.root.join(&original), false)?;
            Ok(())
        };
        if let Err(error) = cleanup() {
            tracing::warn!(%id,%error,"trabajo expirado; limpieza pendiente");
            continue;
        }
        a.db.lock().await.execute(
            "UPDATE jobs SET purged_at=?2 WHERE id=?1 AND status='expired'",
            params![id, now()],
        )?;
        purged += 1;
        tracing::info!(%id,"archivos del trabajo expirado eliminados");
    }
    Ok(purged)
}

pub(crate) async fn start(a: &App) -> Result<Option<tokio::task::JoinHandle<()>>> {
    if a.cfg.retention_days == 0 {
        return Ok(None);
    }
    sweep(a).await?;
    let app = a.clone();
    Ok(Some(tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(3600)).await;
            if let Err(error) = sweep(&app).await {
                tracing::warn!(%error,"falló el mantenimiento de retención");
            }
        }
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    async fn seed(a: &App, id: &str, state: &str, finished: i64) {
        fs::write(a.root.join(format!("originals/{id}.docx")), b"original").unwrap();
        fs::create_dir_all(a.root.join("published").join(id)).unwrap();
        fs::write(
            a.root.join("published").join(id).join("bundle.zip"),
            b"bundle",
        )
        .unwrap();
        fs::create_dir_all(a.root.join("attempts").join(id)).unwrap();
        a.db.lock().await.execute("INSERT INTO jobs(id,filename,original,sha256,status,created_at,finished_at,bundle) VALUES(?1,'a.docx',?2,'-',?3,0,?4,?5)", params![id,format!("originals/{id}.docx"),state,finished,format!("published/{id}/bundle.zip")]).unwrap();
    }
    #[tokio::test]
    async fn retention_is_opt_in_and_only_expires_old_finished_jobs() {
        let tmp = tempfile::tempdir().unwrap();
        let mut a = crate::app(tmp.path().to_owned()).unwrap();
        for (id, state, finished) in [
            ("complete", "complete", 1),
            ("partial", "partial", 1),
            ("failed", "failed", 1),
            ("queued", "queued", 1),
            ("active", "processing", 1),
            ("recent", "complete", now()),
        ] {
            seed(&a, id, state, finished).await;
        }
        assert_eq!(sweep(&a).await.unwrap(), 0);
        assert!(a.root.join("originals/complete.docx").exists());
        std::sync::Arc::make_mut(&mut a.cfg).retention_days = 1;
        assert_eq!(sweep(&a).await.unwrap(), 3);
        assert_eq!(sweep(&a).await.unwrap(), 0);
        for id in ["complete", "partial", "failed"] {
            assert!(!a.root.join(format!("originals/{id}.docx")).exists());
            assert!(!a.root.join("published").join(id).exists());
            assert!(!a.root.join("attempts").join(id).exists());
            let r = crate::router(a.clone())
                .oneshot(
                    Request::builder()
                        .uri(format!("/jobs/{id}/bundle"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(r.status(), StatusCode::GONE);
        }
        for id in ["queued", "active", "recent"] {
            assert!(a.root.join(format!("originals/{id}.docx")).exists());
        }
        // La recuperación al arrancar nunca devuelve un expirado a la cola.
        crate::worker::reconcile(&a).await.unwrap();
        let status: String =
            a.db.lock()
                .await
                .query_row("SELECT status FROM jobs WHERE id='complete'", [], |r| {
                    r.get(0)
                })
                .unwrap();
        assert_eq!(status, "expired");
    }
    #[tokio::test]
    async fn interrupted_cleanup_is_resumed_without_resetting_expiration() {
        let tmp = tempfile::tempdir().unwrap();
        let mut a = crate::app(tmp.path().to_owned()).unwrap();
        std::sync::Arc::make_mut(&mut a.cfg).retention_days = 1;
        seed(&a, "retry", "failed", 1).await;
        let original = a.root.join("originals/retry.docx");
        fs::remove_file(&original).unwrap();
        fs::create_dir(&original).unwrap(); // Fuerza un fallo de limpieza del original.
        assert_eq!(sweep(&a).await.unwrap(), 0);
        let expired: (String, Option<i64>) =
            a.db.lock()
                .await
                .query_row(
                    "SELECT status,purged_at FROM jobs WHERE id='retry'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
        assert_eq!(expired, ("expired".into(), None));
        fs::remove_dir(&original).unwrap();
        fs::write(&original, b"original").unwrap();
        assert_eq!(sweep(&a).await.unwrap(), 1);
        assert!(!original.exists());
    }
}
