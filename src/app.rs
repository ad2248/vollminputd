use crate::audio::AudioCapture;
use crate::clipboard::Clipboard;
use crate::state::{transition, AppEvent, AppState};

#[derive(Debug, PartialEq)]
pub enum SideEffect {
    StartAudio,
    StopAudio,
    RequestAsr { pcm_data: Vec<u8> },
    Notify { title: String, body: String, timeout_secs: u32 },
    CopyToClipboard(String),
}

pub struct VoiceInputApp<A: AudioCapture, C: Clipboard> {
    pub state: AppState,
    audio: A,
    clipboard: C,
    recording_start: Option<tokio::time::Instant>,
}

impl<A: AudioCapture, C: Clipboard> VoiceInputApp<A, C> {
    pub fn new(audio: A, clipboard: C) -> Self {
        Self {
            state: AppState::Idle,
            audio,
            clipboard,
            recording_start: None,
        }
    }

    pub async fn handle_event(&mut self, event: AppEvent) -> Vec<SideEffect> {
        let mut effects = Vec::new();

        let new_state = match transition(self.state.clone(), event.clone()) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[WARN] Invalid state transition: {}", e);
                return effects;
            }
        };

        self.state = new_state;

        match &self.state {
            AppState::Recording => {
                println!("[INFO] 状态: Idle → Recording");
                if let Err(e) = self.audio.start_capture().await {
                    let msg = format!("无法访问音频设备: {}", e);
                    eprintln!("[ERROR] {}", msg);
                    effects.push(SideEffect::Notify {
                        title: "录音失败".to_string(),
                        body: msg,
                        timeout_secs: 5,
                    });
                    self.state = AppState::Idle;
                } else {
                    self.recording_start = Some(tokio::time::Instant::now());
                    effects.push(SideEffect::StartAudio);
                    effects.push(SideEffect::Notify {
                        title: "开始录音".to_string(),
                        body: "正在录音，请说话...".to_string(),
                        timeout_secs: 5,
                    });
                }
            }
            AppState::Transcribing => {
                println!("[INFO] 状态: Recording → Transcribing");
                match self.audio.stop_capture().await {
                    Ok(data) => {
                        self.recording_start = None;
                        effects.push(SideEffect::StopAudio);
                        effects.push(SideEffect::Notify {
                            title: "开始识别".to_string(),
                            body: "正在处理录音...".to_string(),
                            timeout_secs: 5,
                        });
                        effects.push(SideEffect::RequestAsr { pcm_data: data });
                    }
                    Err(e) => {
                        let msg = format!("音频采集失败: {}", e);
                        eprintln!("[ERROR] {}", msg);
                        effects.push(SideEffect::Notify {
                            title: "录音失败".to_string(),
                            body: msg,
                            timeout_secs: 5,
                        });
                        self.state = AppState::Idle;
                    }
                }
            }
            AppState::Idle => {
                println!("[INFO] 状态: Transcribing → Idle");
                self.recording_start = None;
                // 根据事件类型处理结果
                match event {
                    AppEvent::TranscriptionComplete(text) => {
                        if let Err(e) = self.clipboard.copy_text(&text) {
                            eprintln!("[ERROR] 剪贴板复制失败: {}", e);
                            effects.push(SideEffect::Notify {
                                title: "剪贴板错误".to_string(),
                                body: format!("无法复制: {}", e),
                                timeout_secs: 5,
                            });
                        } else {
                            effects.push(SideEffect::Notify {
                                title: "识别完成".to_string(),
                                body: text.clone(),
                                timeout_secs: 10,
                            });
                        }
                    }
                    AppEvent::TranscriptionFailed(msg) => {
                        effects.push(SideEffect::Notify {
                            title: "识别失败".to_string(),
                            body: msg,
                            timeout_secs: 5,
                        });
                    }
                    _ => {}
                }
            }
        }

        effects
    }

    pub fn poll_recording(&self, max_seconds: u64) -> (Vec<SideEffect>, bool) {
        if let Some(start) = self.recording_start {
            let elapsed = start.elapsed().as_secs();
            let effects = vec![SideEffect::Notify {
                title: "录音中".to_string(),
                body: format!("已录制 {} 秒", elapsed),
                timeout_secs: 1,
            }];
            (effects, elapsed >= max_seconds)
        } else {
            (vec![], false)
        }
    }
}