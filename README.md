# Murmur (Phase 1 Scaffold)

Local-first menu bar speech-to-text app scaffold for macOS using Tauri + Svelte + Rust.

## Included in this scaffold

- Menu-bar tray app with hidden main panel window.
- Fixed global toggle hotkey: `Ctrl+Shift+S`.
- Audio capture via `cpal` (default microphone).
- Whisper transcription path via `whisper-rs`.
- Auto-copy transcription to clipboard.
- SQLite history persistence (recent list + delete).
- Frontend panel showing status, result editor, and recent history.

## Model file requirement

Place Whisper model file here before first transcription:

`~/Library/Application Support/com.alazar.murmur/models/ggml-base.en.bin`

## Run locally

1. Install JS deps:
   - `npm install`
2. Run app in dev:
   - `npm run tauri:dev`

## Current limitations (intentional for Phase 1)

- No settings window yet.
- No VAD auto-stop yet.
- No model downloader yet.
- No push-to-talk key-up detection yet.

## Notes

- This environment could not reach `crates.io` or npm registries, so compile/runtime validation must be done locally on your machine with network access.
- If Tauri 2 API changes require small fixes, most likely files are:
  - `src-tauri/src/main.rs`
  - `src-tauri/src/commands.rs`
