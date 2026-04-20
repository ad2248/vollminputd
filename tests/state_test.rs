use VoiceInput::state::{transition, AppEvent, AppState};

#[test]
fn test_idle_to_recording() {
    let result = transition(AppState::Idle, AppEvent::StartRecording);
    assert_eq!(result, Ok(AppState::Recording));
}

#[test]
fn test_recording_to_transcribing() {
    let result = transition(AppState::Recording, AppEvent::FinishRecording);
    assert_eq!(result, Ok(AppState::Transcribing));
}

#[test]
fn test_transcribing_to_result() {
    let text = "Hello world".to_string();
    let result = transition(
        AppState::Transcribing,
        AppEvent::TranscriptionComplete(text.clone()),
    );
    assert_eq!(result, Ok(AppState::Result(text)));
}

#[test]
fn test_result_to_idle_accept() {
    let result = transition(AppState::Result("some text".to_string()), AppEvent::Accept);
    assert_eq!(result, Ok(AppState::Idle));
}

#[test]
fn test_recording_to_idle_cancel() {
    let result = transition(AppState::Recording, AppEvent::Cancel);
    assert_eq!(result, Ok(AppState::Idle));
}

#[test]
fn test_transcribing_to_idle_cancel() {
    let result = transition(AppState::Transcribing, AppEvent::Cancel);
    assert_eq!(result, Ok(AppState::Idle));
}

#[test]
fn test_result_to_recording_retry() {
    let result = transition(
        AppState::Result("previous text".to_string()),
        AppEvent::Retry,
    );
    assert_eq!(result, Ok(AppState::Recording));
}

#[test]
fn test_error_to_recording_retry() {
    let result = transition(AppState::Error("some error".to_string()), AppEvent::Retry);
    assert_eq!(result, Ok(AppState::Recording));
}

#[test]
fn test_transcribing_to_error() {
    let error_msg = "Network timeout".to_string();
    let result = transition(
        AppState::Transcribing,
        AppEvent::TranscriptionFailed(error_msg.clone()),
    );
    assert_eq!(result, Ok(AppState::Error(error_msg)));
}

#[test]
fn test_illegal_idle_accept() {
    let result = transition(AppState::Idle, AppEvent::Accept);
    assert!(result.is_err());
    let err_msg = result.unwrap_err();
    assert!(err_msg.contains("Invalid transition"));
    assert!(err_msg.contains("Idle"));
    assert!(err_msg.contains("Accept"));
}

#[test]
fn test_error_to_idle_cancel() {
    let result = transition(AppState::Error("some error".to_string()), AppEvent::Cancel);
    assert_eq!(result, Ok(AppState::Idle));
}

#[test]
fn test_result_to_idle_cancel() {
    let result = transition(AppState::Result("some text".to_string()), AppEvent::Cancel);
    assert_eq!(result, Ok(AppState::Idle));
}
