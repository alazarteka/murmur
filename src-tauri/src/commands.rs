use crate::audio;
use crate::db;
use crate::models;
use crate::state::{AppStatus, SharedState};
use crate::whisper;
use anyhow::Result;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_clipboard_manager::ClipboardExt;

#[derive(Debug, Clone, Serialize)]
struct ErrorPayload {
    message: String,
}

#[derive(Debug, Clone, Serialize)]
struct TranscriptionCompletePayload {
    id: i64,
    text: String,
    duration_ms: i64,
    model: String,
}

#[derive(Debug, Clone, Serialize)]
struct ModelDownloadProgressPayload {
    file_name: String,
    percent: u8,
}

#[derive(Debug, Clone, Serialize)]
struct ModelDownloadCompletePayload {
    file_name: String,
}

#[tauri::command]
pub fn get_app_state(state: State<'_, SharedState>) -> AppStatus {
    state.status()
}

#[tauri::command]
pub fn start_recording(app: AppHandle, state: State<'_, SharedState>) -> Result<(), String> {
    start_recording_impl(app, state.inner().clone()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn stop_recording(app: AppHandle, state: State<'_, SharedState>) -> Result<(), String> {
    stop_recording_impl(app, state.inner().clone())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn toggle_recording(app: AppHandle, state: State<'_, SharedState>) -> Result<(), String> {
    toggle_recording_impl(app, state.inner().clone())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_history(
    state: State<'_, SharedState>,
    limit: Option<i64>,
) -> Result<Vec<db::HistoryEntry>, String> {
    let count = limit.unwrap_or(15).clamp(1, 500);
    db::list(&state.db_path(), count).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_transcription(state: State<'_, SharedState>, id: i64) -> Result<(), String> {
    db::delete(&state.db_path(), id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn copy_text(app: AppHandle, text: String) -> Result<(), String> {
    app.clipboard()
        .write_text(text)
        .map_err(|e| format!("Failed to copy text: {e}"))
}

#[tauri::command]
pub fn list_models(state: State<'_, SharedState>) -> Result<Vec<models::ModelInfo>, String> {
    models::list_models(&state.models_dir(), &state.active_model_name()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_active_model(
    app: AppHandle,
    state: State<'_, SharedState>,
    file_name: String,
) -> Result<(), String> {
    let model_path = state.models_dir().join(&file_name);

    if !model_path.exists() {
        let app_for_progress = app.clone();
        let models_dir = state.models_dir();
        let file_name_for_download = file_name.clone();

        let download_result = tauri::async_runtime::spawn_blocking(move || {
            let mut last_emitted: Option<u8> = None;
            models::download_model(&models_dir, &file_name_for_download, |percent| {
                if last_emitted == Some(percent) {
                    return;
                }
                last_emitted = Some(percent);
                let payload = ModelDownloadProgressPayload {
                    file_name: file_name_for_download.clone(),
                    percent,
                };
                let _ = app_for_progress.emit("model-download-progress", payload);
            })
        })
        .await
        .map_err(|err| format!("Model download task failed: {err}"))?;

        download_result.map_err(|err| err.to_string())?;
        let _ = app.emit(
            "model-download-complete",
            ModelDownloadCompletePayload {
                file_name: file_name.clone(),
            },
        );
    }

    state
        .set_active_model(file_name)
        .map_err(|err| err.to_string())
}

pub async fn toggle_recording_impl(app: AppHandle, state: SharedState) -> Result<()> {
    match state.status() {
        AppStatus::Idle => {
            start_recording_impl(app, state)?;
            Ok(())
        }
        AppStatus::Recording => stop_recording_impl(app, state).await,
        AppStatus::Processing => Ok(()),
    }
}

pub fn emit_error(app: &AppHandle, message: impl Into<String>) {
    let payload = ErrorPayload {
        message: message.into(),
    };
    let _ = app.emit("transcription-error", payload);
}

fn start_recording_impl(app: AppHandle, state: SharedState) -> Result<()> {
    let session = audio::start_capture(30)?;
    state
        .set_recording(session)
        .map_err(|e| anyhow::anyhow!(e))?;
    let _ = app.emit("recording-started", ());
    Ok(())
}

async fn stop_recording_impl(app: AppHandle, state: SharedState) -> Result<()> {
    let session = state.take_recording().map_err(|e| anyhow::anyhow!(e))?;
    let _ = app.emit("recording-stopped", ());

    let result: Result<()> = async {
        let captured = audio::stop_capture(session);

        if captured.duration_ms < 200 {
            emit_error(&app, "Recording too short");
            return Ok(());
        }

        let db_path = state.db_path();
        let model_path = state.active_model_path();
        let model_name = state.active_model_name();

        let transcription = tauri::async_runtime::spawn_blocking(move || {
            whisper::transcribe(&model_path, &captured.samples, captured.sample_rate)
                .map(|text| (text, captured.duration_ms))
        })
        .await??;

        let (text, duration_ms) = transcription;
        let normalized = if text.trim().is_empty() {
            "(No speech detected)".to_string()
        } else {
            text.trim().to_string()
        };

        let id = db::insert(&db_path, &normalized, duration_ms, &model_name)?;

        app.clipboard().write_text(normalized.clone())?;

        let payload = TranscriptionCompletePayload {
            id,
            text: normalized,
            duration_ms,
            model: model_name,
        };
        let _ = app.emit("transcription-complete", payload);
        Ok(())
    }
    .await;

    state.set_idle();
    if let Err(err) = &result {
        emit_error(&app, err.to_string());
    }
    result
}
