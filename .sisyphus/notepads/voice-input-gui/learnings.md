# Learnings

## 2026-04-15 Session Start
- Project: Rust (edition 2024), existing modules: asr.rs, config.rs, main.rs
- ASR: DashScope WebSocket API (qwen3-asr-flash-realtime)
- Desktop: Wayland (Hyprland)
- GUI: Slint (floating panel, frameless, always-on-top)
- Audio: cpal (replacing pw-record external process)
- Clipboard: wl-copy only (removing wtype)
- FIFO preserved at /tmp/amao_voice_ime.fifo

## 2026-04-17 State Machine Implementation (TDD)
- Pattern: Define enums (AppState, AppEvent) + pure transition function with exhaustive match
- Return type: `Result<AppState, String>` for testable error cases
- TDD approach: 12 tests written first (RED), then minimal implementation (GREEN)
- Edge cases covered: Illegal transitions return Err with descriptive message
- Variants with data: `Result(String)` and `Error(String)` store transcription text / error message
- Slint naming gotcha: `accept` is both a callback name and an event handler keyword; rename callback to avoid collision

## ASR Module Refactoring (2026-04-17)

- Extracted `AsrEngine` trait with `#[mockall::automock]` and `#[async_trait::async_trait]` to make the ASR backend mockable.
- Renamed `AsrClient` to `DashScopeAsrEngine` to clarify it's a specific backend implementation.
- Implemented `AsrEngine::recognize()` for `DashScopeAsrEngine`, which wraps the existing WebSocket session methods (`start_recognition`, `send_audio_chunk`, `finish_and_wait_result`) into a single-call interface.
- Marked existing real integration test with `#[ignore = "Requires real API key and audio file"]`.
- Created `tests/asr_test.rs` with 4 mock-based tests covering: happy path, error path, empty result, and config defaults.
- `mockall` must be in `[dependencies]` (not just `[dev-dependencies]`) when `#[mockall::automock]` is applied to traits in library source code outside `#[cfg(test)]`.
- `cargo test --test asr_test` passes with 4/4 tests.
## Slint Integration Patterns

- `slint::include_modules!()` generates Rust types from compiled `.slint` files at compile time via build.rs.
- The generated component name matches the exported component name in `.slint` (e.g., `VoiceInputApp`).
- For each `in-out property <T> name:` in Slint, Rust gets `set_name(value)` and `name()` getters.
- For each `callback name();` in Slint, Rust gets `on_name(callback: impl Fn() + 'static)`.
- Slint normalizes kebab-case names to snake_case in Rust (e.g., `accept-result` -> `on_accept_result`).
- Window positioning uses `slint::LogicalPosition::new(x, y)` on `app.window()`.
- `app.show()` and `app.hide()` control visibility.

## Audio Capture Module with cpal (2026-04-17)

- cpal 0.17 trait imports required: `use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};`
- `SampleRate` is a type alias for `u32` in cpal 0.17, not a struct: use `sample_rate: 16000` directly
- `mockall::automock` on async traits requires `async-trait = "0.1"` crate
- `mockall` must be in `[dependencies]` (not dev-dependencies) when `#[mockall::automock]` is used on public library traits
- Arc<Mutex<Vec<u8>>> pattern for cross-thread audio buffer sharing between callback and consumer
- cpal captures i16 samples; convert to bytes with `sample.to_ne_bytes()` for PCM format
- 16kHz mono PCM matches DashScope ASR input format
- Mock-based TDD: write tests first with MockAudioCapture, then implement CpalAudioCapture

## GUI + Async ASR Integration (2026-04-17)

- Use a single `tokio::select!` loop to multiplex FIFO commands and GUI events into a unified `AppEvent` stream.
- GUI callbacks capture a clone of an `mpsc::Sender<AppEvent>`; `blocking_send` is safe because they run on the GUI thread.
- The FIFO listener runs in a dedicated `std::thread` and forwards via `mpsc` to avoid blocking the async runtime.
- Recording timeout is checked on the idle tick (`tokio::time::sleep`) and injects `AppEvent::FinishRecording` when exceeded.
- ASR work is spawned in a `tokio::task` so the main loop stays responsive; the task sends its result back through the same GUI event channel.
- Clipboard copy happens on the transition from `Result` to `Idle` (via `Accept`), not inside the ASR task.
- `std::mem::replace` is useful for tracking `old_state` when transitioning into `Idle` (to decide whether to copy to clipboard).
- When calling trait methods on concrete types inside `main.rs`, the trait itself must be in scope (e.g., `use crate::asr::AsrEngine;`) even though the concrete type implements it.
- Integration tests should exercise mocks by actually calling trait methods; otherwise `times(N)` expectations will fail on drop.
- A `simulate_core_loop` helper using `&mut dyn AudioCapture` / `&dyn AsrEngine` / `&dyn Clipboard` lets tests inject mocks while replicating the real main loop logic.
