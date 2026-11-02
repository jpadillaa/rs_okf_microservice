//! Señales compartidas por API y worker (plan 2, C6).
use std::{
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};
use tokio::sync::Notify;

#[derive(Default)]
pub(crate) struct Shutdown {
    started: OnceLock<Instant>,
    notify: Notify,
}
impl Shutdown {
    pub(crate) fn requested(&self) -> bool {
        self.started.get().is_some()
    }
    pub(crate) fn elapsed(&self) -> Option<Duration> {
        self.started.get().map(Instant::elapsed)
    }
    fn request(&self) {
        let _ = self.started.set(Instant::now());
        self.notify.notify_waiters();
    }
    pub(crate) async fn wait(&self) {
        // Registrar la espera antes de comprobar el estado evita perder una señal.
        let notified = self.notify.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        if !self.requested() {
            notified.await;
        }
    }
}

pub(crate) fn install() -> std::io::Result<Arc<Shutdown>> {
    use tokio::signal::unix::{SignalKind, signal};
    let mut term = signal(SignalKind::terminate())?;
    let mut interrupt = signal(SignalKind::interrupt())?;
    let shutdown = Arc::new(Shutdown::default());
    let watched = shutdown.clone();
    tokio::spawn(async move {
        tokio::select! { _ = term.recv() => {}, _ = interrupt.recv() => {} }
        tracing::info!("señal recibida; comienza el apagado ordenado");
        watched.request();
    });
    Ok(shutdown)
}
