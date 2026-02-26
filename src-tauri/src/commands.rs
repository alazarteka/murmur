use crate::audio;
use crate::db;
use crate::models;
use crate::settings;
use crate::state::{AppStatus, SharedState};
use crate::whisper;
use anyhow::Result;
use serde::Serialize;
use std::str::FromStr;
use std::sync::atomic::Ordering;
use std::time::Instant;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Modifiers, Shortcut};

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
    auto_copied: bool,
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

#[derive(Debug, Clone, Serialize)]
struct HotkeyUpdatedPayload {
    hotkey: String,
}

#[derive(Debug, Clone, Serialize)]
struct AutoCopyUpdatedPayload {
    auto_copy: bool,
}

#[derive(Debug, Clone, Serialize)]
struct NoticePayload {
    message: String,
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
pub fn cancel_transcription(app: AppHandle, state: State<'_, SharedState>) -> Result<bool, String> {
    let requested = state
        .request_cancel_processing()
        .map_err(|err| err.to_string())?;
    if requested {
        emit_notice(&app, "Cancelling transcription...");
    }
    Ok(requested)
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
pub fn get_hotkey(state: State<'_, SharedState>) -> String {
    state.hotkey()
}

#[tauri::command]
pub fn get_auto_copy(state: State<'_, SharedState>) -> bool {
    state.auto_copy()
}

#[tauri::command]
pub fn set_hotkey(
    app: AppHandle,
    state: State<'_, SharedState>,
    hotkey: String,
) -> Result<String, String> {
    set_hotkey_impl(app, state.inner().clone(), hotkey).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_auto_copy(
    app: AppHandle,
    state: State<'_, SharedState>,
    enabled: bool,
) -> Result<bool, String> {
    state
        .set_auto_copy(enabled)
        .map_err(|err| err.to_string())?;

    let _ = app.emit(
        "auto-copy-updated",
        AutoCopyUpdatedPayload { auto_copy: enabled },
    );
    Ok(enabled)
}

#[tauri::command]
pub fn get_audio_input_status() -> audio::AudioInputStatus {
    audio::input_status()
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

fn set_hotkey_impl(app: AppHandle, state: SharedState, hotkey: String) -> Result<String> {
    let new_shortcut = parse_hotkey(&hotkey)?;
    let old_hotkey = state.hotkey();
    let old_shortcut = parse_hotkey(&old_hotkey)
        .or_else(|_| parse_hotkey(settings::DEFAULT_HOTKEY))
        .map_err(|e| anyhow::anyhow!("Invalid current hotkey: {e}"))?;

    if new_shortcut.id() == old_shortcut.id() {
        let canonical = new_shortcut.to_string();
        state
            .set_hotkey(canonical.clone())
            .map_err(|e| anyhow::anyhow!(e))?;
        return Ok(canonical);
    }

    // Unregistering may fail if the old shortcut was not registered. Continue so
    // users can recover by selecting a new valid shortcut.
    let _ = app.global_shortcut().unregister(old_shortcut);

    if let Err(err) = app.global_shortcut().register(new_shortcut) {
        let _ = app.global_shortcut().register(old_shortcut);
        anyhow::bail!("Failed to register hotkey (possibly used by another app): {err}");
    }

    let canonical = new_shortcut.to_string();
    if let Err(err) = state.set_hotkey(canonical.clone()) {
        let _ = app.global_shortcut().unregister(new_shortcut);
        let _ = app.global_shortcut().register(old_shortcut);
        anyhow::bail!(err);
    }

    let _ = app.emit(
        "hotkey-updated",
        HotkeyUpdatedPayload {
            hotkey: canonical.clone(),
        },
    );

    Ok(canonical)
}

fn parse_hotkey(raw: &str) -> Result<Shortcut> {
    let shortcut =
        Shortcut::from_str(raw.trim()).map_err(|e| anyhow::anyhow!("Invalid hotkey: {e}"))?;
    let required_mods = Modifiers::SHIFT | Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER;
    if !shortcut.mods.intersects(required_mods) {
        anyhow::bail!("Hotkey must include at least one modifier key");
    }
    Ok(shortcut)
}

pub async fn toggle_recording_impl(app: AppHandle, state: SharedState) -> Result<()> {
    match state.status() {
        AppStatus::Idle => {
            start_recording_impl(app, state)?;
            Ok(())
        }
        AppStatus::Recording => stop_recording_impl(app, state).await,
        AppStatus::Processing | AppStatus::Cancelling => {
            emit_notice(&app, "Transcription is still running. Please wait.");
            Ok(())
        }
    }
}

pub fn emit_error(app: &AppHandle, message: impl Into<String>) {
    let payload = ErrorPayload {
        message: message.into(),
    };
    let _ = app.emit("transcription-error", payload);
}

pub fn emit_notice(app: &AppHandle, message: impl Into<String>) {
    let payload = NoticePayload {
        message: message.into(),
    };
    let _ = app.emit("app-notice", payload);
}

fn start_recording_impl(app: AppHandle, state: SharedState) -> Result<()> {
    let session = audio::start_capture(30)?;
    state
        .set_recording(session)
        .map_err(|e| anyhow::anyhow!(e))?;
    crate::set_tray_listening(&app, true);
    let _ = app.emit("recording-started", ());
    Ok(())
}

async fn stop_recording_impl(app: AppHandle, state: SharedState) -> Result<()> {
    let (session, cancel_requested) = state.take_recording().map_err(|e| anyhow::anyhow!(e))?;
    let _ = app.emit("recording-stopped", ());
    crate::set_tray_listening(&app, false);

    let result: Result<()> = async {
        let captured = audio::stop_capture(session);

        if captured.truncated {
            emit_notice(
                &app,
                "Recording exceeded 30 seconds. Only the first 30 seconds were transcribed.",
            );
        }

        if captured.duration_ms < 200 {
            emit_error(&app, "Recording too short");
            return Ok(());
        }

        let db_path = state.db_path();
        let models_dir = state.models_dir();
        let mut model_name = state.active_model_name();
        let mut model_path = state.active_model_path();

        if !model_path.exists() {
            let fallback = models::pick_default_model(&models_dir);
            let fallback_path = models_dir.join(&fallback);
            if fallback_path.exists() {
                if fallback != model_name {
                    emit_notice(
                        &app,
                        format!(
                            "Active model '{}' is missing. Switched to '{}'.",
                            model_name, fallback
                        ),
                    );
                }
                let _ = state.set_active_model(fallback.clone());
                model_name = fallback;
                model_path = fallback_path;
            }
        }

        if !model_path.exists() {
            anyhow::bail!(
                "No installed model available. Download a model or add a .bin file in the models directory."
            );
        }

        let cancel_for_worker = cancel_requested.clone();
        let transcribe_started = Instant::now();
        let transcription = tauri::async_runtime::spawn_blocking(move || {
            whisper::transcribe(
                &model_path,
                &captured.samples,
                captured.sample_rate,
                Some(cancel_for_worker),
            )
                .map(|text| (text, captured.duration_ms))
        })
        .await?;
        let transcribe_ms = transcribe_started.elapsed().as_millis() as u64;

        if cancel_requested.load(Ordering::Relaxed) {
            emit_notice(&app, "Transcription cancelled.");
            let _ = app.emit("transcription-cancelled", ());
            return Ok(());
        }

        let (text, duration_ms) = match transcription {
            Ok(value) => value,
            Err(err) => {
                if cancel_requested.load(Ordering::Relaxed) {
                    emit_notice(&app, "Transcription cancelled.");
                    let _ = app.emit("transcription-cancelled", ());
                    return Ok(());
                }
                return Err(err);
            }
        };
        let normalized = if text.trim().is_empty() {
            "(No speech detected)".to_string()
        } else {
            text.trim().to_string()
        };

        let id = db::insert(&db_path, &normalized, duration_ms, &model_name)?;

        let auto_copied = if state.auto_copy() {
            app.clipboard().write_text(normalized.clone())?;
            true
        } else {
            false
        };

        if transcribe_ms > 15_000 {
            emit_notice(
                &app,
                format!(
                    "Transcription took {:.1}s. Consider a smaller model for faster response.",
                    transcribe_ms as f64 / 1000.0
                ),
            );
        }

        let payload = TranscriptionCompletePayload {
            id,
            text: normalized,
            duration_ms,
            model: model_name,
            auto_copied,
        };
        let _ = app.emit("transcription-complete", payload);
        Ok(())
    }
    .await;

    state.set_idle();
    crate::set_tray_listening(&app, false);
    if let Err(err) = &result {
        emit_error(&app, err.to_string());
    }
    result
}
