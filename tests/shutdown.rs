//! Señales reales durante una lectura detenida, sin disparadores en producción.
mod support;
use sqlx::PgPool;
use std::{
    fs,
    io::Write,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
const BIN: &str = env!("CARGO_BIN_EXE_okf-microservice");
struct Running(Child);
impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
async fn wait_until<F: FnMut() -> Fut, Fut: std::future::Future<Output = bool>>(mut condition: F) {
    let start = Instant::now();
    while !condition().await {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "plazo de la prueba agotado"
        );
        thread::sleep(Duration::from_millis(10));
    }
}
async fn state(db: &PgPool) -> (String, i64) {
    sqlx::query_as("SELECT status,attempt FROM jobs WHERE id='signal'")
        .fetch_one(db)
        .await
        .unwrap()
}
#[tokio::test]
async fn shutdown_requeues_without_attempt_or_finishes_within_grace() {
    for complete in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let db = support::pool(root).await;
        assert!(
            Command::new(BIN)
                .args(["worker", "--once"])
                .env("OKF_DATA_DIR", root)
                .env("OKF_OCR_ENABLED", "false")
                .env("OKF_DATABASE_URL", support::database_url(root))
                .output()
                .unwrap()
                .status
                .success()
        );
        let original = root.join("originals/signal.docx");
        let cpath = std::ffi::CString::new(original.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(cpath.as_ptr(), 0o600) }, 0);
        sqlx::query("INSERT INTO jobs(id,filename,original,sha256,status,created_at) VALUES('signal','sample.docx','originals/signal.docx','-','queued',0)")
            .execute(&db).await.unwrap();
        let mut worker = Running(
            Command::new(BIN)
                .arg("worker")
                .env("OKF_DATA_DIR", root)
                .env("OKF_OCR_ENABLED", "false")
                .env("OKF_DATABASE_URL", support::database_url(root))
                .env("OKF_SHUTDOWN_GRACE_S", if complete { "5" } else { "0" })
                .stdout(Stdio::null())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap(),
        );
        wait_until(|| async { state(&db).await.0 == "processing" }).await;
        let writer = if complete {
            let bytes = fs::read("examples/docx/breve.docx").unwrap();
            let (tx, rx) = std::sync::mpsc::channel();
            let (release, hold) = std::sync::mpsc::channel();
            let writer = thread::spawn(move || {
                let mut fifo = fs::OpenOptions::new().write(true).open(original).unwrap();
                tx.send(()).unwrap();
                hold.recv_timeout(Duration::from_secs(10)).unwrap();
                fifo.write_all(&bytes).unwrap();
            });
            rx.recv_timeout(Duration::from_secs(10)).unwrap();
            Some((release, writer))
        } else {
            None
        };
        let signal = if complete {
            libc::SIGINT
        } else {
            libc::SIGTERM
        };
        assert_eq!(unsafe { libc::kill(worker.0.id() as i32, signal) }, 0);
        if let Some((release, writer)) = writer {
            thread::sleep(Duration::from_millis(100));
            release.send(()).unwrap();
            writer.join().unwrap();
        }
        wait_until(|| std::future::ready(worker.0.try_wait().unwrap().is_some())).await;
        assert!(worker.0.wait().unwrap().success());
        assert_eq!(
            state(&db).await,
            if complete {
                ("complete".into(), 1)
            } else {
                ("queued".into(), 0)
            }
        );
        assert_eq!(root.join("published/signal/bundle.zip").exists(), complete);
    }
}
