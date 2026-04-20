use VoiceInput::audio::{AudioCapture, MockAudioCapture};
use VoiceInput::asr::{AsrEngine, MockAsrEngine};
use VoiceInput::clipboard::{Clipboard, MockClipboard};
use VoiceInput::state::{transition, AppEvent, AppState};

/// Simulates the core application loop logic from main.rs
/// using trait objects so mocks can be injected.
async fn simulate_core_loop(
    audio: &mut dyn AudioCapture,
    asr: &dyn AsrEngine,
    clipboard: &dyn Clipboard,
    events: Vec<AppEvent>,
) -> AppState {
    let mut state = AppState::Idle;

    for incoming in events {
        let new_state = match transition(state.clone(), incoming) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let old_state = std::mem::replace(&mut state, new_state);

        match &state {
            AppState::Recording => {
                let _ = audio.start_capture().await;
            }
            AppState::Transcribing => {
                let pcm_data = match audio.stop_capture().await {
                    Ok(data) => data,
                    Err(e) => {
                        state = AppState::Error(format!("音频采集失败: {}", e));
                        continue;
                    }
                };
                let asr_event = match asr.recognize(&pcm_data).await {
                    Ok(text) if !text.is_empty() => {
                        AppEvent::TranscriptionComplete(text)
                    }
                    Ok(_) => AppEvent::TranscriptionFailed("未检测到语音".to_string()),
                    Err(e) => AppEvent::TranscriptionFailed(format!("识别失败: {}", e)),
                };
                state = transition(state.clone(), asr_event).unwrap_or(state);
            }
            AppState::Idle => {
                if let AppState::Result(text) = old_state {
                    let _ = clipboard.copy_text(&text);
                }
            }
            _ => {}
        }
    }

    state
}

#[tokio::test]
async fn test_full_flow_start_finish_accept() {
    let mut audio = MockAudioCapture::new();
    audio.expect_start_capture()
        .times(1)
        .returning(|| Ok(()));
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

    let final_state = simulate_core_loop(
        &mut audio,
        &asr,
        &clipboard,
        vec![
            AppEvent::StartRecording,
            AppEvent::FinishRecording,
            AppEvent::Accept,
        ],
    )
    .await;

    assert_eq!(final_state, AppState::Idle);
}

#[tokio::test]
async fn test_cancel_from_recording() {
    let mut audio = MockAudioCapture::new();
    audio.expect_start_capture()
        .times(1)
        .returning(|| Ok(()));

    let asr = MockAsrEngine::new();
    let clipboard = MockClipboard::new();

    let final_state = simulate_core_loop(
        &mut audio,
        &asr,
        &clipboard,
        vec![AppEvent::StartRecording, AppEvent::Cancel],
    )
    .await;

    assert_eq!(final_state, AppState::Idle);
}

#[tokio::test]
async fn test_retry_flow() {
    let mut audio = MockAudioCapture::new();
    audio.expect_start_capture()
        .times(2)
        .returning(|| Ok(()));
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

    let final_state = simulate_core_loop(
        &mut audio,
        &asr,
        &clipboard,
        vec![
            AppEvent::StartRecording,
            AppEvent::FinishRecording,
            AppEvent::Retry,
            AppEvent::FinishRecording,
            AppEvent::Accept,
        ],
    )
    .await;

    assert_eq!(final_state, AppState::Idle);
}

#[tokio::test]
async fn test_transcription_failed_to_error() {
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

    let final_state = simulate_core_loop(
        &mut audio,
        &asr,
        &clipboard,
        vec![
            AppEvent::StartRecording,
            AppEvent::FinishRecording,
            AppEvent::Retry,
        ],
    )
    .await;

    assert_eq!(final_state, AppState::Recording);
}

#[test]
fn test_timeout_event_triggers_finish() {
    // In the real main loop, elapsed >= max_recording_seconds causes
    // an automatic AppEvent::FinishRecording to be injected.
    // We verify the state transition path here.
    let s = transition(AppState::Recording, AppEvent::FinishRecording).unwrap();
    assert_eq!(s, AppState::Transcribing);
}
