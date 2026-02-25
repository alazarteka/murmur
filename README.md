# Murmur

Local-first menu bar speech-to-text app for macOS, built with Tauri + Svelte + Rust.

## Current behavior

- Menu bar app with global hotkey to start/stop recording (default: `Ctrl+Shift+S`).
- Audio capture from the default microphone with CPAL.
- Local Whisper transcription via `whisper-rs` (English transcription mode).
- Recording hard limit: 30 seconds (if exceeded, the app transcribes the first 30s and shows a notice).
- SQLite history with list + delete.
- Model selector UI with known-model auto-download (Hugging Face), progress UI, retries, and fallback to installed models.
- Clipboard behavior is configurable:
  - `Auto-copy transcripts` setting is persisted.
  - Default is **off**.
  - Manual `Copy` is always available.

## Tray and window UX

- App starts as a menu bar accessory and keeps the main window hidden by default.
- Left-click tray icon toggles window visibility.
- Tray menu includes `Open Murmur` and `Quit Murmur`.
- Closing the window hides it instead of quitting.

## Model files

Put `.bin` whisper models in:

`~/Library/Application Support/com.alazar.murmur/models/`

Preferred model order (best available installed model is auto-selected):

1. `ggml-large-v3-turbo-q5_0.bin`
2. `ggml-large-v3-turbo.bin`
3. `ggml-large-v3.bin`
4. `ggml-medium.en.bin`
5. `ggml-small.en.bin`
6. `ggml-base.en.bin`
7. `ggml-tiny.en.bin`

If you select a known model that is not installed, Murmur tries to download it automatically.

## Local run

1. Install dependencies:
   - `npm install`
2. Start dev app:
   - `npm run tauri:dev`

## Build and checks

- Frontend build: `npm run build`
- Rust check: `cd src-tauri && cargo check`
- Local production bundle: `npm run tauri:build`

## Data and settings paths

Under app data directory (`com.alazar.murmur`):

- `models/` for model binaries
- `murmur.db` for transcription history
- `settings.json` for hotkey + auto-copy preference

## GitHub Actions release flow

- Workflow: `.github/workflows/main-release.yml`
- Runs on `push` to `main` only when build-relevant files change (plus manual dispatch).
- Builds macOS bundles and updates a rolling prerelease tag: `main-latest`.
- Uploads:
  - `.dmg`
  - `Murmur.app.zip`
