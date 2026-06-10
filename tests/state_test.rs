use vollminputd::state::{transition, AppEvent, AppState};

#[test]
fn test_idle_to_recording() {
    let result = transition(AppState::Idle, AppEvent::ToggleRecording);
    assert_eq!(result, Ok(AppState::Recording));
}

#[test]
fn test_recording_to_transcribing() {
    let result = transition(AppState::Recording, AppEvent::ToggleRecording);
    assert_eq!(result, Ok(AppState::Transcribing));
}

#[test]
fn test_transcribing_to_idle_complete() {
    let result = transition(
        AppState::Transcribing,
        AppEvent::TranscriptionComplete("你好".to_string()),
    );
    assert_eq!(result, Ok(AppState::Idle));
}

#[test]
fn test_transcribing_to_idle_failed() {
    let result = transition(
        AppState::Transcribing,
        AppEvent::TranscriptionFailed("网络错误".to_string()),
    );
    assert_eq!(result, Ok(AppState::Idle));
}

#[test]
fn test_illegal_idle_complete() {
    let result = transition(
        AppState::Idle,
        AppEvent::TranscriptionComplete("text".to_string()),
    );
    assert!(result.is_err());
    let err_msg = result.unwrap_err();
    assert!(err_msg.contains("Invalid transition"));
    assert!(err_msg.contains("Idle"));
}

#[test]
fn test_illegal_recording_complete() {
    let result = transition(
        AppState::Recording,
        AppEvent::TranscriptionComplete("text".to_string()),
    );
    assert!(result.is_err());
    let err_msg = result.unwrap_err();
    assert!(err_msg.contains("Invalid transition"));
    assert!(err_msg.contains("Recording"));
}

#[test]
fn test_illegal_transcribing_toggle() {
    let result = transition(AppState::Transcribing, AppEvent::ToggleRecording);
    assert!(result.is_err());
    let err_msg = result.unwrap_err();
    assert!(err_msg.contains("Invalid transition"));
    assert!(err_msg.contains("Transcribing"));
}