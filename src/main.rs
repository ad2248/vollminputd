use VoiceInput::app::{SideEffect, VoiceInputApp};
use VoiceInput::asr::{AsrConfig, AsrEngine, DashScopeAsrEngine};
use VoiceInput::audio::CpalAudioCapture;
use VoiceInput::clipboard::WlCopyClipboard;
use VoiceInput::config::Config;
use VoiceInput::gui::VoiceInputGui;
use VoiceInput::state::{AppEvent, AppState};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::Command;
use std::thread;
use tokio::sync::mpsc;

const FIFO_PATH: &str = "/tmp/amao_voice_ime.fifo";

enum ImeCommand {
    Start,
    Stop,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("喵！阿猫的 Rust 语音输入法守护进程启动啦！🐾");

    let config = Config::from_yaml("conf.yaml").unwrap_or_else(|e| {
        eprintln!("警告：无法加载配置文件: {}, 使用默认配置", e);
        Config {
            DASHSCOPE_API_KEY: String::new(),
            max_recording_seconds: 60,
            audio_sample_rate: 16000,
            audio_channels: 1,
        }
    });

    setup_fifo(FIFO_PATH)?;

    let gui = VoiceInputGui::new()?;
    let audio = CpalAudioCapture::new();
    let clipboard = WlCopyClipboard::new();
    let mut app = VoiceInputApp::new(audio, clipboard);

    let (fifo_tx, mut fifo_rx) = mpsc::channel::<ImeCommand>(10);

    thread::spawn(move || loop {
        if let Ok(file) = File::open(FIFO_PATH) {
            let reader = BufReader::new(file);
            for line in reader.lines().flatten() {
                let cmd = line.trim();
                if cmd == "START" {
                    let _ = fifo_tx.blocking_send(ImeCommand::Start);
                } else if cmd == "STOP" {
                    let _ = fifo_tx.blocking_send(ImeCommand::Stop);
                }
            }
        }
    });

    let (gui_tx, mut gui_rx) = mpsc::channel::<AppEvent>(10);

    {
        let tx = gui_tx.clone();
        gui.on_start_recording(move || {
            let _ = tx.blocking_send(AppEvent::StartRecording);
        });
    }
    {
        let tx = gui_tx.clone();
        gui.on_finish_recording(move || {
            let _ = tx.blocking_send(AppEvent::FinishRecording);
        });
    }
    {
        let tx = gui_tx.clone();
        gui.on_cancel(move || {
            let _ = tx.blocking_send(AppEvent::Cancel);
        });
    }
    {
        let tx = gui_tx.clone();
        gui.on_retry(move || {
            let _ = tx.blocking_send(AppEvent::Retry);
        });
    }
    {
        let tx = gui_tx.clone();
        gui.on_accept_result(move || {
            let _ = tx.blocking_send(AppEvent::Accept);
        });
    }

    let asr_api_key = config.DASHSCOPE_API_KEY.clone();
    let max_recording_seconds = config.max_recording_seconds;

    gui.show()?;
    gui.update_state(&app.state);

    loop {
        let mut event: Option<AppEvent> = None;

        tokio::select! {
            Some(cmd) = fifo_rx.recv() => {
                match cmd {
                    ImeCommand::Start => event = Some(AppEvent::StartRecording),
                    ImeCommand::Stop => event = Some(AppEvent::FinishRecording),
                }
            }
            Some(gui_event) = gui_rx.recv() => {
                event = Some(gui_event);
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {}
        }

        if matches!(app.state, AppState::Recording) {
            let (poll_effects, timeout) = app.poll_recording(max_recording_seconds);
            execute_effects(&gui, &poll_effects);
            if timeout {
                event = Some(AppEvent::FinishRecording);
            }
        }

        if let Some(incoming) = event {
            let effects = app.handle_event(incoming).await;
            execute_effects_with_asr(
                &gui,
                &effects,
                &asr_api_key,
                &gui_tx,
            );
        }
    }
}

fn execute_effects(gui: &VoiceInputGui, effects: &[SideEffect]) {
    for effect in effects {
        match effect {
            SideEffect::UpdateState(state) => gui.update_state(state),
            SideEffect::SetResultText(text) => gui.set_result_text(text),
            SideEffect::SetErrorMessage(msg) => gui.set_error_message(msg),
            SideEffect::SetRecordingDuration(secs) => gui.set_recording_duration(*secs),
            SideEffect::Hide => gui.hide(),
            SideEffect::RequestAsr { .. } => {}
        }
    }
}

fn execute_effects_with_asr(
    gui: &VoiceInputGui,
    effects: &[SideEffect],
    api_key: &str,
    tx: &mpsc::Sender<AppEvent>,
) {
    for effect in effects {
        match effect {
            SideEffect::UpdateState(state) => gui.update_state(state),
            SideEffect::SetResultText(text) => gui.set_result_text(text),
            SideEffect::SetErrorMessage(msg) => gui.set_error_message(msg),
            SideEffect::SetRecordingDuration(secs) => gui.set_recording_duration(*secs),
            SideEffect::Hide => gui.hide(),
            SideEffect::RequestAsr { pcm_data } => {
                let tx = tx.clone();
                let key = api_key.to_string();
                let pcm = pcm_data.clone();
                tokio::spawn(async move {
                    let engine = DashScopeAsrEngine::new(AsrConfig {
                        api_key: key,
                        ..Default::default()
                    });
                    match engine.recognize(&pcm).await {
                        Ok(text) if !text.is_empty() => {
                            let _ = tx
                                .send(AppEvent::TranscriptionComplete(text))
                                .await;
                        }
                        Ok(_) => {
                            let _ = tx
                                .send(AppEvent::TranscriptionFailed(
                                    "未检测到语音".to_string(),
                                ))
                                .await;
                        }
                        Err(e) => {
                            let _ = tx
                                .send(AppEvent::TranscriptionFailed(format!(
                                    "识别失败: {}",
                                    e
                                )))
                                .await;
                        }
                    }
                });
            }
        }
    }
}

fn setup_fifo(path: &str) -> anyhow::Result<()> {
    let path_obj = Path::new(path);
    if path_obj.exists() {
        fs::remove_file(path_obj)?;
    }
    Command::new("mkfifo").arg(path).status()?;
    Command::new("chmod").arg("0666").arg(path).status()?;
    Ok(())
}
