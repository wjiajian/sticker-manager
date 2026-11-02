use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use sticker_core::{ImportProgress, ImportReport, Library};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportStatus {
    pub batch: u64,
    pub running: bool,
    pub progress: ImportProgress,
    pub report: Option<ImportReport>,
    pub error: Option<String>,
}
#[derive(Default)]
pub struct ImportState {
    status: Mutex<ImportStatus>,
    cancelled: AtomicBool,
    ball_started: Mutex<Option<Instant>>,
}
impl ImportState {
    fn begin(&self, ball: bool) -> Result<ImportStatus, String> {
        let mut status = self.status.lock().unwrap();
        if status.running {
            return Err("正在导入另一批表情，请等它完成".into());
        }
        self.cancelled.store(false, Ordering::Relaxed);
        *self.ball_started.lock().unwrap() = ball.then(Instant::now);
        *status = ImportStatus {
            batch: status.batch + 1,
            running: true,
            ..ImportStatus::default()
        };
        Ok(status.clone())
    }
}

pub fn status(app: &AppHandle) -> ImportStatus {
    app.state::<ImportState>().status.lock().unwrap().clone()
}
pub fn cancel(app: &AppHandle) {
    app.state::<ImportState>()
        .cancelled
        .store(true, Ordering::Relaxed);
}
pub fn ball_progress(app: &AppHandle) -> Option<String> {
    let state = app.try_state::<ImportState>()?;
    let started = *state.ball_started.lock().unwrap();
    if started.is_none_or(|s| s.elapsed() < Duration::from_millis(500)) {
        return None;
    }
    let status = state.status.lock().unwrap();
    status.running.then(|| {
        format!(
            "正在导入 {}/{}",
            status.progress.processed, status.progress.total
        )
    })
}

pub async fn run(app: AppHandle, paths: Vec<PathBuf>, ball: bool) -> Result<ImportReport, String> {
    let first = app.state::<ImportState>().begin(ball)?;
    let _ = app.emit("import-status-changed", first);
    let handle = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<ImportState>();
        let library = handle.state::<Mutex<Library>>();
        let mut progress_event = Instant::now() - Duration::from_secs(1);
        let mut changed_event = Instant::now();
        sticker_core::import_paths_with_progress(
            &library,
            &paths,
            &|| state.cancelled.load(Ordering::Relaxed),
            &mut |progress| {
                if progress_event.elapsed() >= Duration::from_millis(150)
                    || progress.processed == progress.total
                {
                    let mut status = state.status.lock().unwrap();
                    status.progress = progress;
                    let _ = handle.emit("import-status-changed", status.clone());
                    progress_event = Instant::now();
                }
                if changed_event.elapsed() >= Duration::from_secs(1) {
                    crate::sync::local_changed(&handle);
                    crate::commands::notify_library_changed(&handle);
                    changed_event = Instant::now();
                }
            },
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);
    let state = app.state::<ImportState>();
    let mut status = state.status.lock().unwrap();
    status.running = false;
    match &result {
        Ok(report) => status.report = Some(report.clone()),
        Err(error) => status.error = Some(error.clone()),
    }
    *state.ball_started.lock().unwrap() = None;
    let _ = app.emit("import-status-changed", status.clone());
    drop(status);
    crate::sync::local_changed(&app);
    crate::commands::notify_library_changed(&app);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_one_batch_and_cancel_resets_at_next_start() {
        let state = ImportState::default();
        assert!(state.begin(false).is_ok());
        state.cancelled.store(true, Ordering::Relaxed);
        assert_eq!(
            state.begin(true).err().unwrap(),
            "正在导入另一批表情，请等它完成"
        );
        state.status.lock().unwrap().running = false;
        assert_eq!(state.begin(false).unwrap().batch, 2);
        assert!(!state.cancelled.load(Ordering::Relaxed));
    }
}
