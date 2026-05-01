use VoiceInput::state::{transition, AppEvent, AppState};

#[test]
fn test_asr_timeout_error_state() {
    let s = transition(AppState::Idle, AppEvent::ToggleRecording).unwrap();
    let s = transition(s, AppEvent::ToggleRecording).unwrap();

    let s = transition(
        s,
        AppEvent::TranscriptionFailed("ASR timeout after 30s".to_string()),
    )
    .unwrap();
    assert_eq!(s, AppState::Idle);
}

#[test]
fn test_empty_recognition_result() {
    let s = transition(AppState::Idle, AppEvent::ToggleRecording).unwrap();
    let s = transition(s, AppEvent::ToggleRecording).unwrap();

    let s = transition(s, AppEvent::TranscriptionFailed("未检测到语音".to_string())).unwrap();
    assert_eq!(s, AppState::Idle);
}

#[test]
fn test_max_recording_timeout_transition() {
    let s = transition(AppState::Recording, AppEvent::ToggleRecording).unwrap();
    assert_eq!(s, AppState::Transcribing);
}

#[test]
fn test_double_toggle_idempotency() {
    let s = transition(AppState::Idle, AppEvent::ToggleRecording).unwrap();
    assert_eq!(s, AppState::Recording);

    let result = transition(s, AppEvent::ToggleRecording);
    assert_eq!(result, Ok(AppState::Transcribing));
}

#[test]
fn test_toggle_without_start() {
    // 在 Idle 状态下直接 Toggle 会开始录音
    let result = transition(AppState::Idle, AppEvent::ToggleRecording);
    assert_eq!(result, Ok(AppState::Recording));
}

#[test]
fn test_illegal_transcribing_toggle() {
    let result = transition(AppState::Transcribing, AppEvent::ToggleRecording);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid transition"));
}