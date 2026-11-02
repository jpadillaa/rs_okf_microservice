//! Señales reales durante una lectura detenida, sin disparadores en producción.
use rusqlite::{Connection, params};
use std::{
    fs,
    io::Write,
    path::Path,
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
fn wait_until(mut condition: impl FnMut() -> bool) {
    let start = Instant::now();
    while !condition() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "plazo de la prueba agotado"
        );
        thread::sleep(Duration::from_millis(10));
    }
}
fn state(root: &Path) -> (String, u32) {
    Connection::open(root.join("jobs.sqlite3"))
        .unwrap()
        .query_row(
            "SELECT status,attempt FROM jobs WHERE id='signal'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap()
}
#[test]
fn shutdown_requeues_without_attempt_or_finishes_within_grace() {
    for complete in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        assert!(
            Command::new(BIN)
                .args(["worker", "--once"])
                .env("OKF_DATA_DIR", root)
                .output()
                .unwrap()
                .status
                .success()
        );
        let original = root.join("originals/signal.docx");
        let cpath = std::ffi::CString::new(original.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(cpath.as_ptr(), 0o600) }, 0);
        Connection::open(root.join("jobs.sqlite3")).unwrap().execute("INSERT INTO jobs(id,filename,original,sha256,status,created_at) VALUES('signal','sample.docx',?1,'-','queued',0)", params!["originals/signal.docx"]).unwrap();
        let mut worker = Running(
            Command::new(BIN)
                .arg("worker")
                .env("OKF_DATA_DIR", root)
                .env("OKF_SHUTDOWN_GRACE_S", if complete { "5" } else { "0" })
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        wait_until(|| state(root).0 == "processing");
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
        wait_until(|| worker.0.try_wait().unwrap().is_some());
        assert!(worker.0.wait().unwrap().success());
        assert_eq!(
            state(root),
            if complete {
                ("complete".into(), 1)
            } else {
                ("queued".into(), 0)
            }
        );
        assert_eq!(root.join("published/signal/bundle.zip").exists(), complete);
    }
}
