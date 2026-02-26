# Murmur Roadmap

Source of truth for delivery phases and release gates.

Date updated: 2026-02-26

## Phase Plan

| Phase | Target | Features | Exit Criteria |
| --- | --- | --- | --- |
| 1. Update Pipeline First | 1-2 weeks | In-app updater, code signing | App can check/download/install update from in-app UI; artifact is signed/notarized; rollback path validated on one prior version |
| 2. Core Runtime UX | 1 week | Launch at login, active model persistence, transcription cancel | App auto-starts on login (toggleable), selected model survives restart, user can cancel active transcription safely |
| 3. Data + Input UX | 1-2 weeks | History search, input device picker, history export/import | Search is fast on large history, user can choose non-default mic, history round-trips via export/import without data loss |
| 4. Reliability/Operability | 1 week | Diagnostic logging + export, test coverage + CI gates | One-click diagnostics bundle exists; CI blocks merge on failing tests/build/lint; smoke tests cover hot paths |
| 5. Language Expansion | 1-2 weeks | Language selection / auto-detect | User can choose language or auto-detect; model/language compatibility is enforced in UI; quality baseline documented |

## Phase 1 (Updater Quick) Breakdown

1. Switch release channel model from rolling-only to updater-friendly stable metadata (optionally keep `main-latest` for internal use).
2. Add signing/notarization secrets and CI signing steps.
3. Integrate updater in app with manual `Check for updates` first, then optional startup check.
4. Add post-update relaunch flow and failure messaging.
5. Validate full upgrade path from an older installed version.

## Why This Order

1. Updater has highest user value but is blocked by signing/notarization.
2. Runtime UX items are low-risk and high-impact.
3. Data/input features add clear daily-use value after core UX is stable.
4. Logging/tests reduce regression risk before language complexity.
5. Language expansion comes after reliability and operational guardrails are in place.

## Tracking Status

- Phase 1: In progress
- Phase 2: In progress
- Phase 3: Not started
- Phase 4: Not started
- Phase 5: Not started

## Execution Rule

Any new feature request must be mapped to one of the phases above before implementation, unless it is a production bug fix.
