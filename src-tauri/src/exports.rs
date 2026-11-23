use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use sticker_core::{
    ExportFormat, ExportProgress, ExportReport, ExportRequest, ExportScope, Library,
};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportStatus {
    pub batch: u64,
    pub running: bool,
    pub progress: ExportProgress,
    pub report: Option<ExportReport>,
    pub error: Option<String>,
}
#[derive(Default)]
pub struct ExportState {
    status: Mutex<ExportStatus>,
    cancelled: AtomicBool,
}
impl ExportState {
    fn begin(&self) -> Result<ExportStatus, String> {
        let mut status = self.status.lock().unwrap();
        if status.running {
            return Err("正在导出另一批表情，请等它完成".into());
        }
        self.cancelled.store(false, Ordering::Relaxed);

        *status = ExportStatus {
            batch: status.batch + 1,
            running: true,
            ..ExportStatus::default()
        };
        Ok(status.clone())
    }
}

#[tauri::command]
pub fn export_status(app: AppHandle) -> ExportStatus {
    app.state::<ExportState>().status.lock().unwrap().clone()
}
#[tauri::command]
pub fn cancel_export(app: AppHandle) {
    app.state::<ExportState>()
        .cancelled
        .store(true, Ordering::Relaxed);
}
#[tauri::command]
pub async fn export_stickers(
    app: AppHandle,
    scope: ExportScope,
    format: ExportFormat,
    destination: PathBuf,
    name: String,
    utc_offset_minutes: i32,
) -> Result<ExportReport, String> {
    let jobs = app.state::<crate::jobs::LibraryJobs>();
    let _job = jobs.begin(crate::jobs::Job::Export)?;
    let first = app.state::<ExportState>().begin()?;
    let _ = app.emit("export-status-changed", first);
    let handle = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<ExportState>();
        let library = handle.state::<Mutex<Library>>();
        let mut progress_event = Instant::now() - Duration::from_secs(1);
        let request = ExportRequest {
            scope,
            format,
            destination,
            name,
            utc_offset_minutes,
            forbidden: crate::location::sync_folder(&handle).into_iter().collect(),
        };
        sticker_core::export_with_progress(
            &library,
            &request,
            &|| state.cancelled.load(Ordering::Relaxed),
            &mut |progress| {
                if progress_event.elapsed() >= Duration::from_millis(150)
                    || progress.processed == progress.total
                {
                    let mut status = state.status.lock().unwrap();
                    status.progress = progress;
                    let _ = handle.emit("export-status-changed", status.clone());
                    progress_event = Instant::now();
                }
            },
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);
    let state = app.state::<ExportState>();
    let mut status = state.status.lock().unwrap();
    status.running = false;
    match &result {
        Ok(report) => status.report = Some(report.clone()),
        Err(error) => status.error = Some(error.clone()),
    }

    let _ = app.emit("export-status-changed", status.clone());
    drop(status);

    result
}

#[tauri::command]
pub async fn reveal_export(app: AppHandle) -> Result<(), String> {
    let path = export_status(app)
        .report
        .and_then(|r| r.output)
        .ok_or("没有可显示的导出文件")?;
    tauri_plugin_opener::reveal_item_in_dir(path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_one_batch_and_cancel_resets_at_next_start() {
        let state = ExportState::default();
        assert!(state.begin().is_ok());
        state.cancelled.store(true, Ordering::Relaxed);
        assert_eq!(
            state.begin().err().unwrap(),
            "正在导出另一批表情，请等它完成"
        );
        state.status.lock().unwrap().running = false;
        assert_eq!(state.begin().unwrap().batch, 2);
        assert!(!state.cancelled.load(Ordering::Relaxed));
    }
}
