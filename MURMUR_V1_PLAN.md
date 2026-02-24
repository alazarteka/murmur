# Murmur v1 Implementation Plan (Reliability + Low Footprint)

## Goal
Ship a personal macOS menu-bar dictation app that is dependable under daily use, starts fast, and stays small.

## Non-negotiables
- Fully local speech-to-text (no network for transcription).
- Menu bar only (`LSUIElement = true`), no dock icon.
- Two hotkey modes: push-to-talk and toggle.
- Clipboard-first output with optional auto-paste.
- Crash-safe history and settings persistence.

## Reliability-first scope cuts for v1
Keep these out of initial release to reduce moving parts:
- Multiple VAD engines: use only `webrtc-vad` initially.
- Multiple model downloads in UI: ship with `base.en` support first; add others after core flow is stable.
- Complex waveform rendering: simple RMS bars, 20 FPS max.
- Welcome wizard polish: keep first-run flow minimal but robust.

## Recommended stack adjustments
Your stack is solid; these tweaks reduce size/risk:
- Keep `tauri 2`, `svelte 5`, `rusqlite`, `cpal`, `whisper-rs`, `webrtc-vad`.
- Replace `tokio = { features = ["full"] }` with minimal needed features (`rt-multi-thread`, `macros`, `sync`, `time`) to reduce binary size and startup overhead.
- Prefer one background worker thread for transcription jobs over broad async fanout.
- Use SQLite WAL mode + `busy_timeout` for stability with concurrent reads/writes.

## Hotkey implementation recommendation
- Use `tauri-plugin-global-shortcut` for trigger registration.
- For push-to-talk release detection, use a macOS event tap (`CGEventTap`) narrowly scoped to the configured combo.
- Treat event tap failures as non-fatal: automatically degrade to toggle-only mode and notify in panel.

This avoids app-wide keylog-style monitoring and keeps failure domains small.

## Audio/transcription pipeline hardening
- Capture at device sample rate, then resample to 16k mono using a deterministic resampler path.
- Use fixed-size ring buffer (30s cap) with overwrite prevention and explicit stop reason.
- Reject recordings below `min_speech_ms` before whisper to avoid garbage outputs.
- Add transcription timeout guard (e.g., 25s). If exceeded, fail gracefully and reset state.
- Always transition state through a single serialized state machine function.

## State model
Use strict, serial state transitions:
- `Idle`
- `Recording { mode, started_at, sample_count }`
- `Processing { started_at, audio_ms }`
- `Result { id, text, created_at }`
- `Error { message, recoverable }`

Rule: all transitions happen on one command queue to prevent race conditions.

## Storage design
SQLite setup:
- `PRAGMA journal_mode=WAL;`
- `PRAGMA synchronous=NORMAL;`
- `PRAGMA foreign_keys=ON;`
- `PRAGMA busy_timeout=2500;`

Schema additions:
- Keep your `transcriptions` table and FTS5.
- Add `copied_at TEXT NULL` (optional analytics for personal usage/debug).
- Add migration table and explicit schema versioning from day 1.

## Settings robustness
- Keep TOML config, but write atomically:
  1. write `config.toml.tmp`
  2. fsync
  3. rename to `config.toml`
- Validate hotkeys and bounds at load.
- On invalid config, back up and regenerate defaults; never block app launch.

## First-run flow (minimal reliable)
1. Ensure app dirs exist.
2. Ensure model file exists; if missing, download with resume support.
3. Request microphone permission on first record attempt.
4. Validate one 1-second test transcription in background.
5. Mark setup complete.

If model download fails, app remains usable UI-wise with clear retry action.

## Performance budget (targets)
- Cold launch to tray ready: < 700ms on Apple Silicon.
- Start recording after hotkey press: < 80ms.
- Stop-to-text latency for 3-5s utterance (`base.en`): < 1.5s typical.
- Idle CPU: ~0%.
- Idle memory footprint (without model loaded): < 120MB RSS.

## Model lifecycle strategy
- v1: lazy-load model on first transcription request and keep warm for N minutes (e.g., 10) after last use.
- Unload model when idle timeout hits to reduce memory footprint.
- Setting: `keep_model_warm = true/false` for user control later.

## Observability (local only)
- Structured logs to file in app support dir.
- Rotate logs (e.g., 5 files x 1MB).
- Include state transitions, hotkey events, audio start/stop reasons, transcription duration, error codes.

No telemetry upload.

## Security/privacy guardrails
- Explicitly block any network calls except model download endpoint.
- Redact transcription text from logs by default.
- Auto-paste requires explicit opt-in and accessibility permission check.

## Project layout

```
src-tauri/
  src/
    main.rs
    app_paths.rs
    state.rs
    state_machine.rs
    hotkeys.rs
    audio/
      mod.rs
      capture.rs
      resample.rs
      level.rs
    stt/
      mod.rs
      whisper.rs
      model_manager.rs
    storage/
      mod.rs
      db.rs
      migrations.rs
      history_repo.rs
    settings/
      mod.rs
      schema.rs
      store.rs
    ui_events.rs
    commands.rs
    errors.rs
  migrations/
    0001_init.sql
  tauri.conf.json

src/
  app.css
  App.svelte
  lib/
    stores.ts
    api.ts
  components/
    Panel.svelte
    MicArea.svelte
    ResultCard.svelte
    HistoryList.svelte
  settings/
    Settings.svelte
```

## Phased build plan

### Phase 1: Core loop (must pass before anything else)
- Tray app with panel open/close.
- One fixed hotkey (toggle only).
- Record -> transcribe -> show result -> copy.
- Persist history in SQLite.

### Phase 2: Reliability baseline
- Add push-to-talk with release detection.
- Add strict state machine queue.
- Add crash-safe config writes and DB migrations.
- Add error surfaces + recovery paths.

### Phase 3: Settings and device/model management
- Hotkey rebinding + conflict checks.
- Audio device selector.
- Basic model download manager (start/retry/progress).

### Phase 4: UX polish
- Recent history search + item actions.
- Auto-dismiss/auto-paste behavior.
- First-run guided flow.

## Acceptance checklist
- 50 repeated record/stop cycles without stuck state.
- Hotkey rebind works without restart.
- Model missing/corrupt paths recover cleanly.
- App relaunch preserves settings/history.
- No network requests during normal transcription.
- Toggle and push-to-talk both recover after runtime errors.

## Immediate next build step
Scaffold Phase 1 with end-to-end wiring first (even with placeholder UI styling), then lock reliability tests before adding settings complexity.
