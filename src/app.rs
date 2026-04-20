use crate::audio::AudioCapture;
use crate::clipboard::Clipboard;
use crate::state::{transition, AppEvent, AppState};

#[derive(Debug, PartialEq)]
pub enum SideEffect {
    UpdateState(AppState),
    SetResultText(String),
    SetErrorMessage(String),
    SetRecordingDuration(u64),
    Hide,
    RequestAsr { pcm_data: Vec<u8> },
}

pub struct VoiceInputApp<A: AudioCapture, C: Clipboard> {
    pub state: AppState,
    audio: A,
    clipboard: C,
    recording_start: Option<tokio::time::Instant>,
}

impl<A: AudioCapture, C: Clipboard> VoiceInputApp<A, C> {
    pub fn new(audio: A, clipboard: C) -> Self {
        Self {
            state: AppState::Idle,
            audio,
            clipboard,
            recording_start: None,
        }
    }

    pub async fn handle_event(&mut self, event: AppEvent) -> Vec<SideEffect> {
        let mut effects = Vec::new();

        let new_state = match transition(self.state.clone(), event) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Invalid state transition: {}", e);
                return effects;
            }
        };

        let old_state = std::mem::replace(&mut self.state, new_state);
        effects.push(SideEffect::UpdateState(self.state.clone()));

        match &self.state {
            AppState::Recording => {
                if let Err(e) = self.audio.start_capture().await {
                    let msg = format!("无法访问音频设备: {}", e);
                    self.state = AppState::Error(msg.clone());
                    effects.push(SideEffect::UpdateState(self.state.clone()));
                    effects.push(SideEffect::SetErrorMessage(msg));
                } else {
                    self.recording_start = Some(tokio::time::Instant::now());
                }
            }
            AppState::Transcribing => {
                let pcm_data = match self.audio.stop_capture().await {
                    Ok(data) => data,
                    Err(e) => {
                        let msg = format!("音频采集失败: {}", e);
                        self.state = AppState::Error(msg.clone());
                        effects.push(SideEffect::UpdateState(self.state.clone()));
                        effects.push(SideEffect::SetErrorMessage(msg));
                        self.recording_start = None;
                        return effects;
                    }
                };
                self.recording_start = None;
                effects.push(SideEffect::RequestAsr { pcm_data });
            }
            AppState::Result(text) => {
                effects.push(SideEffect::SetResultText(text.clone()));
            }
            AppState::Error(msg) => {
                effects.push(SideEffect::SetErrorMessage(msg.clone()));
            }
            AppState::Idle => {
                self.recording_start = None;
                if let AppState::Result(text) = old_state {
                    if let Err(e) = self.clipboard.copy_text(&text) {
                        eprintln!("Clipboard write failed: {}", e);
                    }
                }
                effects.push(SideEffect::Hide);
            }
        }

        effects
    }

    pub fn poll_recording(&self, max_seconds: u64) -> (Vec<SideEffect>, bool) {
        if let Some(start) = self.recording_start {
            let elapsed = start.elapsed().as_secs();
            let effects = vec![SideEffect::SetRecordingDuration(elapsed)];
            (effects, elapsed >= max_seconds)
        } else {
            (vec![], false)
        }
    }
}
