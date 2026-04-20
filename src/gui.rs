use crate::state::AppState;
use anyhow::Result;

slint::include_modules!();

pub struct VoiceInputGui {
    app: VoiceInputApp,
}

impl VoiceInputGui {
    pub fn new() -> Result<Self> {
        let app = VoiceInputApp::new()?;
        Ok(Self { app })
    }

    pub fn show(&self) -> Result<()> {
        self.app.window().set_position(slint::LogicalPosition::new(
            self.app.window().size().width as f32 / 2.0,
            100.0,
        ));
        self.app.show()?;
        Ok(())
    }

    pub fn hide(&self) {
        let _ = self.app.hide();
    }

    pub fn update_state(&self, state: &AppState) {
        let state_str = match state {
            AppState::Idle => "idle",
            AppState::Recording => "recording",
            AppState::Transcribing => "transcribing",
            AppState::Result(_) => "result",
            AppState::Error(_) => "error",
        };
        self.app.set_current_state(state_str.into());
    }

    pub fn set_result_text(&self, text: &str) {
        self.app.set_result_text(text.into());
    }

    pub fn set_error_message(&self, msg: &str) {
        self.app.set_error_message(msg.into());
    }

    pub fn set_recording_duration(&self, seconds: u64) {
        self.app.set_recording_seconds(seconds as i32);
    }

    pub fn on_start_recording(&self, callback: impl Fn() + 'static) {
        self.app.on_start_recording(callback);
    }

    pub fn on_finish_recording(&self, callback: impl Fn() + 'static) {
        self.app.on_finish_recording(callback);
    }

    pub fn on_cancel(&self, callback: impl Fn() + 'static) {
        self.app.on_cancel(callback);
    }

    pub fn on_retry(&self, callback: impl Fn() + 'static) {
        self.app.on_retry(callback);
    }

    pub fn on_accept_result(&self, callback: impl Fn() + 'static) {
        self.app.on_accept_result(callback);
    }

    pub fn app(&self) -> &VoiceInputApp {
        &self.app
    }
}
