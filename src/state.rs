use anyhow::Result;

#[derive(Debug, Clone, PartialEq)]
pub enum AppState {
    Idle,
    Recording,
    Transcribing,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppEvent {
    ToggleRecording,
    TranscriptionComplete(String),
    TranscriptionFailed(String),
}

pub fn transition(state: AppState, event: AppEvent) -> Result<AppState, String> {
    use AppEvent::*;
    use AppState::*;

    match (state, event) {
        (Idle, ToggleRecording) => Ok(Recording),
        (Recording, ToggleRecording) => Ok(Transcribing),
        (Transcribing, TranscriptionComplete(_)) => Ok(Idle),
        (Transcribing, TranscriptionFailed(_)) => Ok(Idle),
        (state, event) => Err(format!("Invalid transition: {:?} + {:?}", state, event)),
    }
}