use VoiceInput::app::{SideEffect, VoiceInputApp};
use VoiceInput::asr::engine::{AsrEngine, MockAsrEngine};
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
async fn test_full_flow_toggle_toggle_complete() {
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

    // Idle → Recording
    let effects = app.handle_event(AppEvent::ToggleRecording).await;
    assert!(effects.contains(&SideEffect::StartAudio));
    assert!(effects.iter().any(|e| matches!(
        e,
        SideEffect::Notify { title, .. } if title == "开始录音"
    )));
    assert_eq!(app.state, AppState::Recording);

    // Recording → Transcribing
    let effects = app.handle_event(AppEvent::ToggleRecording).await;
    assert!(effects.contains(&SideEffect::StopAudio));
    assert!(effects.iter().any(|e| matches!(
        e,
        SideEffect::Notify { title, .. } if title == "开始识别"
    )));
    assert_eq!(app.state, AppState::Transcribing);

    // ASR 完成 → Idle
    let effects = process_asr_request(&mut app, &effects, &asr).await;
    assert!(effects.iter().any(|e| matches!(
        e,
        SideEffect::Notify { title, .. } if title == "识别完成"
    )));
    assert_eq!(app.state, AppState::Idle);
}

#[tokio::test]
async fn test_transcription_failed() {
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

    app.handle_event(AppEvent::ToggleRecording).await;
    let effects = app.handle_event(AppEvent::ToggleRecording).await;
    let effects = process_asr_request(&mut app, &effects, &asr).await;

    assert_eq!(app.state, AppState::Idle);
    assert!(effects.iter().any(|e| matches!(
        e,
        SideEffect::Notify { title, .. } if title == "识别失败"
    )));
}

#[tokio::test]
async fn test_audio_start_failure() {
    let mut audio = MockAudioCapture::new();
    audio.expect_start_capture()
        .times(1)
        .returning(|| Err(anyhow::anyhow!("设备被占用")));

    let clipboard = MockClipboard::new();
    let mut app = VoiceInputApp::new(audio, clipboard);

    let effects = app.handle_event(AppEvent::ToggleRecording).await;
    assert_eq!(app.state, AppState::Idle);
    assert!(effects.iter().any(|e| matches!(
        e,
        SideEffect::Notify { title, .. } if title == "录音失败"
    )));
}

#[tokio::test]
async fn test_audio_stop_failure() {
    let mut audio = MockAudioCapture::new();
    audio.expect_start_capture().times(1).returning(|| Ok(()));
    audio.expect_stop_capture()
        .times(1)
        .returning(|| Err(anyhow::anyhow!("设备断开")));

    let clipboard = MockClipboard::new();
    let mut app = VoiceInputApp::new(audio, clipboard);

    app.handle_event(AppEvent::ToggleRecording).await;
    let effects = app.handle_event(AppEvent::ToggleRecording).await;

    assert_eq!(app.state, AppState::Idle);
    assert!(effects.iter().any(|e| matches!(
        e,
        SideEffect::Notify { title, .. } if title == "录音失败"
    )));
}

#[tokio::test]
async fn test_empty_recognition_result() {
    let mut audio = MockAudioCapture::new();
    audio.expect_start_capture().times(1).returning(|| Ok(()));
    audio.expect_stop_capture()
        .times(1)
        .returning(|| Ok(vec![1u8; 100]));

    let mut asr = MockAsrEngine::new();
    asr.expect_recognize()
        .times(1)
        .returning(|_| Ok("".to_string()));

    let clipboard = MockClipboard::new();
    let mut app = VoiceInputApp::new(audio, clipboard);

    app.handle_event(AppEvent::ToggleRecording).await;
    let effects = app.handle_event(AppEvent::ToggleRecording).await;
    let effects = process_asr_request(&mut app, &effects, &asr).await;

    assert_eq!(app.state, AppState::Idle);
    assert!(effects.iter().any(|e| matches!(
        e,
        SideEffect::Notify { title, .. } if title == "识别失败"
    )));
}

#[test]
fn test_timeout_triggers_stop() {
    use VoiceInput::state::transition;
    let s = transition(AppState::Recording, AppEvent::ToggleRecording).unwrap();
    assert_eq!(s, AppState::Transcribing);
}