use vollminputd::app::{SideEffect, VollminputdApp};
use vollminputd::asr::engine::{AsrEngine, MockAsrEngine};
use vollminputd::audio::MockAudioCapture;
use vollminputd::clipboard::MockClipboard;
use vollminputd::state::{AppEvent, AppState};

async fn process_asr_request(
    app: &mut VollminputdApp<MockAudioCapture, MockClipboard>,
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
    audio.expect_device_name().returning(|| None);
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

    let mut app = VollminputdApp::new(audio, clipboard);

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
    audio.expect_device_name().returning(|| None);
    audio.expect_stop_capture()
        .times(1)
        .returning(|| Ok(vec![1u8; 100]));

    let mut asr = MockAsrEngine::new();
    asr.expect_recognize()
        .times(1)
        .returning(|_| Err(anyhow::anyhow!("网络超时")));

    let clipboard = MockClipboard::new();
    let mut app = VollminputdApp::new(audio, clipboard);

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
    let mut app = VollminputdApp::new(audio, clipboard);

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
    audio.expect_device_name().returning(|| None);
    audio.expect_stop_capture()
        .times(1)
        .returning(|| Err(anyhow::anyhow!("设备断开")));

    let clipboard = MockClipboard::new();
    let mut app = VollminputdApp::new(audio, clipboard);

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
    audio.expect_device_name().returning(|| None);
    audio.expect_stop_capture()
        .times(1)
        .returning(|| Ok(vec![1u8; 100]));

    let mut asr = MockAsrEngine::new();
    asr.expect_recognize()
        .times(1)
        .returning(|_| Ok("".to_string()));

    let clipboard = MockClipboard::new();
    let mut app = VollminputdApp::new(audio, clipboard);

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
async fn test_poll_recording_max_seconds_zero_triggers_timeout() {
    let mut audio = MockAudioCapture::new();
    audio.expect_start_capture().times(1).returning(|| Ok(()));
    audio.expect_device_name().times(2).returning(|| None);
    audio.expect_stop_capture()
        .times(1)
        .returning(|| Ok(vec![1u8; 100]));

    let clipboard = MockClipboard::new();
    let mut app = VollminputdApp::new(audio, clipboard);

    // Idle → Recording
    app.handle_event(AppEvent::ToggleRecording).await;
    assert_eq!(app.state, AppState::Recording);

    // max_seconds=0：任何已录时长都应立即超时
    let (effects, timeout) = app.poll_recording(0);
    assert!(timeout, "max_seconds=0 应立即触发超时");
    assert!(effects.iter().any(|e| matches!(
        e,
        SideEffect::Notify { title, .. } if title == "录音中"
    )));

    // 超时后应与手动停止一致：停止录音并请求识别
    let effects = app.handle_event(AppEvent::ToggleRecording).await;
    assert_eq!(app.state, AppState::Transcribing);
    assert!(effects.contains(&SideEffect::StopAudio));
    assert!(effects.iter().any(|e| matches!(e, SideEffect::RequestAsr { .. })));
}

#[tokio::test]
async fn test_poll_recording_no_timeout_right_after_start() {
    let mut audio = MockAudioCapture::new();
    audio.expect_start_capture().times(1).returning(|| Ok(()));
    audio.expect_device_name().times(2).returning(|| None);

    let clipboard = MockClipboard::new();
    let mut app = VollminputdApp::new(audio, clipboard);

    app.handle_event(AppEvent::ToggleRecording).await;

    let (effects, timeout) = app.poll_recording(60);
    assert!(!timeout, "刚录音 0 秒不应超时");
    assert!(effects.iter().any(|e| matches!(
        e,
        SideEffect::Notify { title, .. } if title == "录音中"
    )));
}

#[tokio::test]
async fn test_poll_recording_ignores_when_not_recording() {
    let audio = MockAudioCapture::new();
    let clipboard = MockClipboard::new();
    let mut app = VollminputdApp::new(audio, clipboard);

    let (effects, timeout) = app.poll_recording(60);
    assert!(!timeout);
    assert!(effects.is_empty());
}