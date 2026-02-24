use crate::audio::RecordingSession;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AppStatus {
    Idle,
    Recording,
    Processing,
}

struct Inner {
    status: AppStatus,
    recording: Option<RecordingSession>,
}

#[derive(Clone)]
pub struct SharedState {
    inner: Arc<Mutex<Inner>>,
    db_path: Arc<PathBuf>,
    models_dir: Arc<PathBuf>,
    active_model: Arc<RwLock<String>>,
}

impl SharedState {
    pub fn new(db_path: PathBuf, models_dir: PathBuf, active_model: String) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                status: AppStatus::Idle,
                recording: None,
            })),
            db_path: Arc::new(db_path),
            models_dir: Arc::new(models_dir),
            active_model: Arc::new(RwLock::new(active_model)),
        }
    }

    pub fn status(&self) -> AppStatus {
        self.inner
            .lock()
            .map(|inner| inner.status)
            .unwrap_or(AppStatus::Idle)
    }

    pub fn set_recording(&self, session: RecordingSession) -> Result<(), &'static str> {
        let mut guard = self.inner.lock().map_err(|_| "State lock poisoned")?;
        if guard.status != AppStatus::Idle {
            return Err("App is not idle");
        }
        guard.recording = Some(session);
        guard.status = AppStatus::Recording;
        Ok(())
    }

    pub fn take_recording(&self) -> Result<RecordingSession, &'static str> {
        let mut guard = self.inner.lock().map_err(|_| "State lock poisoned")?;
        if guard.status != AppStatus::Recording {
            return Err("App is not recording");
        }
        guard.status = AppStatus::Processing;
        guard.recording.take().ok_or("Recording session missing")
    }

    pub fn set_idle(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.status = AppStatus::Idle;
            guard.recording = None;
        }
    }

    pub fn db_path(&self) -> PathBuf {
        (*self.db_path).clone()
    }

    pub fn models_dir(&self) -> PathBuf {
        (*self.models_dir).clone()
    }

    pub fn active_model_name(&self) -> String {
        self.active_model
            .read()
            .map(|value| value.clone())
            .unwrap_or_else(|_| "ggml-base.en.bin".to_string())
    }

    pub fn active_model_path(&self) -> PathBuf {
        self.models_dir().join(self.active_model_name())
    }

    pub fn set_active_model(&self, file_name: String) -> Result<(), &'static str> {
        if file_name.trim().is_empty() {
            return Err("Model file name cannot be empty");
        }

        let model_path = self.models_dir().join(&file_name);
        if !model_path.exists() {
            return Err("Selected model is not installed in the models directory");
        }

        let mut guard = self
            .active_model
            .write()
            .map_err(|_| "Model lock poisoned")?;
        *guard = file_name;
        Ok(())
    }
}
