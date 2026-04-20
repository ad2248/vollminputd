use VoiceInput::state::{transition, AppEvent, AppState};

#[test]
fn test_asr_timeout_error_state() {
    let s = transition(AppState::Idle, AppEvent::StartRecording).unwrap();
    let s = transition(s, AppEvent::FinishRecording).unwrap();

    let s = transition(
        s,
        AppEvent::TranscriptionFailed("ASR timeout after 30s".to_string()),
    )
    .unwrap();
    assert_eq!(s, AppState::Error("ASR timeout after 30s".to_string()));

    let s = transition(s, AppEvent::Retry).unwrap();
    assert_eq!(s, AppState::Recording);
}

#[test]
fn test_empty_recognition_result() {
    let s = transition(AppState::Idle, AppEvent::StartRecording).unwrap();
    let s = transition(s, AppEvent::FinishRecording).unwrap();

    let s = transition(s, AppEvent::TranscriptionFailed("未检测到语音".to_string())).unwrap();
    assert_eq!(s, AppState::Error("未检测到语音".to_string()));
}

#[test]
fn test_audio_device_unavailable() {
    let s = AppState::Error("无法访问音频设备: No input device available".to_string());
    assert!(matches!(s, AppState::Error(_)));

    let s = transition(s, AppEvent::Retry).unwrap();
    assert_eq!(s, AppState::Recording);
}

#[test]
fn test_max_recording_timeout_transition() {
    let s = transition(AppState::Recording, AppEvent::FinishRecording).unwrap();
    assert_eq!(s, AppState::Transcribing);
}

#[test]
fn test_double_start_idempotency() {
    let s = transition(AppState::Idle, AppEvent::StartRecording).unwrap();
    assert_eq!(s, AppState::Recording);

    let result = transition(s, AppEvent::StartRecording);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid transition"));
}

#[test]
fn test_stop_without_start() {
    let result = transition(AppState::Idle, AppEvent::FinishRecording);
    assert!(result.is_err());
}
