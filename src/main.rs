mod audio;
mod clipboard;
mod config;
mod gui;
mod state;
mod asr;

use crate::audio::{AudioCapture, CpalAudioCapture};
use crate::asr::{AsrEngine, DashScopeAsrEngine};
use crate::clipboard::{Clipboard, WlCopyClipboard};
use crate::config::Config;
use crate::gui::VoiceInputGui;
use crate::state::{transition, AppEvent, AppState};
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

    let config = Config::from_yaml("conf.yaml")
        .unwrap_or_else(|e| {
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
    let mut audio = CpalAudioCapture::new();
    let clipboard = WlCopyClipboard::new();

    let (fifo_tx, mut fifo_rx) = mpsc::channel::<ImeCommand>(10);

    thread::spawn(move || {
        loop {
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
        }
    });

    let (gui_tx, mut gui_rx) = mpsc::channel::<AppEvent>(10);

    {
        let tx = gui_tx.clone();
        gui.on_start_recording(move || { let _ = tx.blocking_send(AppEvent::StartRecording); });
    }
    {
        let tx = gui_tx.clone();
        gui.on_finish_recording(move || { let _ = tx.blocking_send(AppEvent::FinishRecording); });
    }
    {
        let tx = gui_tx.clone();
        gui.on_cancel(move || { let _ = tx.blocking_send(AppEvent::Cancel); });
    }
    {
        let tx = gui_tx.clone();
        gui.on_retry(move || { let _ = tx.blocking_send(AppEvent::Retry); });
    }
    {
        let tx = gui_tx.clone();
        gui.on_accept_result(move || { let _ = tx.blocking_send(AppEvent::Accept); });
    }

    let asr_gui_tx = gui_tx.clone();
    let asr_api_key = config.DASHSCOPE_API_KEY.clone();

    let mut state = AppState::Idle;
    let mut recording_start: Option<tokio::time::Instant> = None;

    gui.show()?;
    gui.update_state(&state);

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

        if matches!(state, AppState::Recording) {
            if let Some(start) = recording_start {
                let elapsed = start.elapsed().as_secs();
                gui.set_recording_duration(elapsed);
                if elapsed >= config.max_recording_seconds {
                    event = Some(AppEvent::FinishRecording);
                }
            }
        }

        if let Some(incoming) = event {
            match transition(state.clone(), incoming.clone()) {
                Ok(new_state) => {
                    let old_state = std::mem::replace(&mut state, new_state);
                    gui.update_state(&state);

                    match &state {
                        AppState::Recording => {
                            if let Err(e) = audio.start_capture().await {
                                state = AppState::Error(format!("无法访问音频设备: {}", e));
                                gui.update_state(&state);
                            } else {
                                recording_start = Some(tokio::time::Instant::now());
                            }
                        }
                        AppState::Transcribing => {
                            let pcm_data = match audio.stop_capture().await {
                                Ok(data) => data,
                                Err(e) => {
                                    state = AppState::Error(format!("音频采集失败: {}", e));
                                    gui.update_state(&state);
                                    recording_start = None;
                                    continue;
                                }
                            };
                            recording_start = None;

                            let tx = asr_gui_tx.clone();
                            let key = asr_api_key.clone();
                            tokio::spawn(async move {
                                let engine = DashScopeAsrEngine::new(crate::asr::AsrConfig {
                                    api_key: key,
                                    ..Default::default()
                                });
                                match engine.recognize(&pcm_data).await {
                                    Ok(text) => {
                                        if text.is_empty() {
                                            let _ = tx.send(AppEvent::TranscriptionFailed("未检测到语音".to_string())).await;
                                        } else {
                                            let _ = tx.send(AppEvent::TranscriptionComplete(text)).await;
                                        }
                                    }
                                    Err(e) => {
                                        let _ = tx.send(AppEvent::TranscriptionFailed(format!("识别失败: {}", e))).await;
                                    }
                                }
                            });
                        }
                        AppState::Result(text) => {
                            gui.set_result_text(text);
                        }
                        AppState::Error(msg) => {
                            gui.set_error_message(msg);
                        }
                        AppState::Idle => {
                            recording_start = None;
                            if let AppState::Result(text) = old_state {
                                if let Err(e) = clipboard.copy_text(&text) {
                                    eprintln!("剪贴板写入失败: {}", e);
                                }
                            }
                            gui.hide();
                        }
                    }
                }
                Err(e) => {
                    eprintln!("非法状态转换: {:?} -> {:?} : {}", state, incoming, e);
                }
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
