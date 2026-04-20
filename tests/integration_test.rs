use VoiceInput::app::{SideEffect, VoiceInputApp};
use VoiceInput::asr::{AsrEngine, MockAsrEngine};
use VoiceInput::audio::MockAudioCapture;
use VoiceInput::clipboard::MockClipboard;
use VoiceInput::state::{AppEvent, AppState};

async fn process_asr_request(
    app: &mut VoiceInputApp<MockAudioCapture, MockClipboard>,
    effects: &[SideEffect],
    asr: &MockAsrEngine,
) -> Vec<SideEffect> {
    for effect in effects {
        if let SideEffect::RequestAsr { pcm_data } = effect {
            let asr_event = match asr.recognize(pcm_data).await {
                Ok(text) if !text.is_empty() => AppEvent::TranscriptionComplete(text),
                Ok(_) => AppEvent::TranscriptionFailed("未检测到语音".to_string()),
                Err(e) => AppEvent::TranscriptionFailed(format!("识别失败: {}", e)),
            };
            return app.handle_event(asr_event).await;
        }
    }
    vec![]
}

#[tokio::test]
async fn test_full_flow_start_finish_accept() {
    let mut audio = MockAudioCapture::new();
    audio.expect_start_capture().times(1).returning(|| Ok(()));
    audio.expect_stop_capture()
        .times(1)
        .returning(|| Ok(vec![1u8; 100]));

    let mut asr = MockAsrEngine::new();
    asr.expect_recognize()
        .times(1)
        .returning(|_| Ok("你好".to_string()));

    let mut clipboard = MockClipboard::new();
    clipboard.expect_copy_text()
        .with(mockall::predicate::eq("你好"))
        .times(1)
        .returning(|_| Ok(()));

    let mut app = VoiceInputApp::new(audio, clipboard);

    let effects = app.handle_event(AppEvent::StartRecording).await;
    assert!(effects.contains(&SideEffect::UpdateState(AppState::Recording)));
    assert_eq!(app.state, AppState::Recording);

    let effects = app.handle_event(AppEvent::FinishRecording).await;
    assert!(effects.contains(&SideEffect::UpdateState(AppState::Transcribing)));

    let effects = process_asr_request(&mut app, &effects, &asr).await;
    assert!(effects.contains(&SideEffect::SetResultText("你好".to_string())));
    assert_eq!(app.state, AppState::Result("你好".to_string()));

    let effects = app.handle_event(AppEvent::Accept).await;
    assert!(effects.contains(&SideEffect::UpdateState(AppState::Idle)));
    assert!(effects.contains(&SideEffect::Hide));
    assert_eq!(app.state, AppState::Idle);
}

#[tokio::test]
async fn test_cancel_from_recording() {
    let mut audio = MockAudioCapture::new();
    audio.expect_start_capture().times(1).returning(|| Ok(()));

    let asr = MockAsrEngine::new();
    let clipboard = MockClipboard::new();

    let mut app = VoiceInputApp::new(audio, clipboard);

    app.handle_event(AppEvent::StartRecording).await;
    assert_eq!(app.state, AppState::Recording);

    let effects = app.handle_event(AppEvent::Cancel).await;
    assert!(effects.contains(&SideEffect::UpdateState(AppState::Idle)));
    assert!(effects.contains(&SideEffect::Hide));
    assert_eq!(app.state, AppState::Idle);
}

#[tokio::test]
async fn test_retry_flow() {
    let mut audio = MockAudioCapture::new();
    audio.expect_start_capture().times(2).returning(|| Ok(()));
    audio.expect_stop_capture()
        .times(2)
        .returning(|| Ok(vec![1u8; 100]));

    let mut asr = MockAsrEngine::new();
    asr.expect_recognize()
        .times(2)
        .returning(|_| Ok("结果".to_string()));

    let mut clipboard = MockClipboard::new();
    clipboard.expect_copy_text()
        .with(mockall::predicate::eq("结果"))
        .times(1)
        .returning(|_| Ok(()));

    let mut app = VoiceInputApp::new(audio, clipboard);

    app.handle_event(AppEvent::StartRecording).await;
    let effects = app.handle_event(AppEvent::FinishRecording).await;
    let effects = process_asr_request(&mut app, &effects, &asr).await;
    assert_eq!(app.state, AppState::Result("结果".to_string()));

    let effects = app.handle_event(AppEvent::Retry).await;
    assert!(effects.contains(&SideEffect::UpdateState(AppState::Recording)));

    let effects = app.handle_event(AppEvent::FinishRecording).await;
    let effects = process_asr_request(&mut app, &effects, &asr).await;
    assert_eq!(app.state, AppState::Result("结果".to_string()));

    let effects = app.handle_event(AppEvent::Accept).await;
    assert!(effects.contains(&SideEffect::Hide));
    assert_eq!(app.state, AppState::Idle);
}

#[tokio::test]
async fn test_transcription_failed_to_error() {
    let mut audio = MockAudioCapture::new();
    audio.expect_start_capture().times(1).returning(|| Ok(()));
    audio.expect_stop_capture()
        .times(1)
        .returning(|| Ok(vec![1u8; 100]));

    let mut asr = MockAsrEngine::new();
    asr.expect_recognize()
        .times(1)
        .returning(|_| Err(anyhow::anyhow!("网络超时")));

    let clipboard = MockClipboard::new();
    let mut app = VoiceInputApp::new(audio, clipboard);

    app.handle_event(AppEvent::StartRecording).await;
    let effects = app.handle_event(AppEvent::FinishRecording).await;
    let effects = process_asr_request(&mut app, &effects, &asr).await;

    assert!(matches!(app.state, AppState::Error(_)));
    assert!(effects.iter().any(|e| matches!(
        e,
        SideEffect::SetErrorMessage(msg) if msg.contains("网络超时")
    )));
}

#[tokio::test]
async fn test_error_retry_goes_to_recording() {
    let mut audio = MockAudioCapture::new();
    audio.expect_start_capture()
        .times(2)
        .returning(|| Ok(()));
    audio.expect_stop_capture()
        .times(1)
        .returning(|| Ok(vec![1u8; 100]));

    let mut asr = MockAsrEngine::new();
    asr.expect_recognize()
        .times(1)
        .returning(|_| Err(anyhow::anyhow!("网络超时")));

    let clipboard = MockClipboard::new();
    let mut app = VoiceInputApp::new(audio, clipboard);

    app.handle_event(AppEvent::StartRecording).await;
    let effects = app.handle_event(AppEvent::FinishRecording).await;
    let _ = process_asr_request(&mut app, &effects, &asr).await;
    assert!(matches!(app.state, AppState::Error(_)));

    let effects = app.handle_event(AppEvent::Retry).await;
    assert!(effects.contains(&SideEffect::UpdateState(AppState::Recording)));
    assert_eq!(app.state, AppState::Recording);
}

#[tokio::test]
async fn test_audio_start_failure() {
    let mut audio = MockAudioCapture::new();
    audio.expect_start_capture()
        .times(1)
        .returning(|| Err(anyhow::anyhow!("设备被占用")));

    let asr = MockAsrEngine::new();
    let clipboard = MockClipboard::new();
    let mut app = VoiceInputApp::new(audio, clipboard);

    let effects = app.handle_event(AppEvent::StartRecording).await;
    assert!(matches!(app.state, AppState::Error(_)));
    assert!(effects.iter().any(|e| matches!(
        e,
        SideEffect::UpdateState(AppState::Error(_))
    )));
    assert!(effects.iter().any(|e| matches!(
        e,
        SideEffect::SetErrorMessage(msg) if msg.contains("设备被占用")
    )));
}

#[tokio::test]
async fn test_audio_stop_failure() {
    let mut audio = MockAudioCapture::new();
    audio.expect_start_capture().times(1).returning(|| Ok(()));
    audio.expect_stop_capture()
        .times(1)
        .returning(|| Err(anyhow::anyhow!("设备断开")));

    let asr = MockAsrEngine::new();
    let clipboard = MockClipboard::new();
    let mut app = VoiceInputApp::new(audio, clipboard);

    app.handle_event(AppEvent::StartRecording).await;
    let effects = app.handle_event(AppEvent::FinishRecording).await;

    assert!(matches!(app.state, AppState::Error(_)));
    assert!(effects.iter().any(|e| matches!(
        e,
        SideEffect::SetErrorMessage(msg) if msg.contains("设备断开")
    )));
}

#[test]
fn test_timeout_event_triggers_finish() {
    use VoiceInput::state::transition;
    let s = transition(AppState::Recording, AppEvent::FinishRecording).unwrap();
    assert_eq!(s, AppState::Transcribing);
}
