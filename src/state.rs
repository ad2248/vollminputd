use anyhow::Result;

#[derive(Debug, Clone, PartialEq)]
pub enum AppState {
    Idle,
    Recording,
    Transcribing,
    Result(String),
    Error(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppEvent {
    StartRecording,
    FinishRecording,
    TranscriptionComplete(String),
    TranscriptionFailed(String),
    Accept,
    Cancel,
    Retry,
}

pub fn transition(state: AppState, event: AppEvent) -> Result<AppState, String> {
    use AppEvent::*;
    use AppState::*;

    match (state, event) {
        (Idle, StartRecording) => Ok(Recording),
        (Recording, FinishRecording) => Ok(Transcribing),
        (Recording, Cancel) => Ok(Idle),
        (Transcribing, TranscriptionComplete(text)) => Ok(Result(text)),
        (Transcribing, TranscriptionFailed(msg)) => Ok(Error(msg)),
        (Transcribing, Cancel) => Ok(Idle),
        (Result(_), Accept) => Ok(Idle),
        (Result(_), Cancel) => Ok(Idle),
        (Result(_), Retry) => Ok(Recording),
        (Error(_), Retry) => Ok(Recording),
        (Error(_), Cancel) => Ok(Idle),
        (state, event) => Err(format!("Invalid transition: {:?} + {:?}", state, event)),
    }
}
