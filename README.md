# Murmur (Current Build)

Local-first menu bar speech-to-text app for macOS using Tauri + Svelte + Rust.

## What works now

- Menu bar tray app with toggle hotkey (`Ctrl+Shift+S`).
- Audio capture via `cpal` from default input device.
- Whisper transcription through `whisper-rs`.
- Auto-copy transcription to clipboard.
- SQLite history (list + delete).
- Editable result area and installed-model selector UI.

## UI behavior

- The app does **not** auto-pop open while recording/transcribing.
- Open/close from tray icon left click.
- Tray menu includes `Open Murmur` and `Quit Murmur`.

## Model directory

Place model files in:

`~/Library/Application Support/com.alazar.murmur/models/`

The app auto-picks the best installed model by preference:

1. `ggml-large-v3-turbo-q5_0.bin`
2. `ggml-large-v3-turbo.bin`
3. `ggml-large-v3.bin`
4. `ggml-medium.en.bin`
5. `ggml-small.en.bin`
6. `ggml-base.en.bin`
7. `ggml-tiny.en.bin`

You can switch the active model from the UI dropdown.

## Run locally

1. `npm install`
2. `npm run tauri:dev`

## Build checks

- Frontend: `npm run build`
- Rust backend: `cd src-tauri && cargo check`
