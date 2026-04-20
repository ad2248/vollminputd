# Decisions

## 2026-04-15 Architecture Decisions
- Trait-based modules: AudioCapture, AsrEngine, Clipboard (all mockable via mockall)
- State machine: pure function transition(state, event) -> Result<AppState, String>
- FIFO: Hyprland binds Win+Space → echo "START" > FIFO; GUI handles F/C/R/Enter internally
- No wtype, no X11, no unsafe, no external state machine lib
