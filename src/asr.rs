use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::time::Duration;
use tokio_tungstenite::{connect_async, tungstenite::http::Request, tungstenite::Message};
use url::Url;

/// DashScope ASR 配置
pub struct AsrConfig {
    pub api_key: String,
    pub model: String,
    pub base_url: String,
}

impl Default for AsrConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: "qwen3-asr-flash-realtime".to_string(),
            base_url: "wss://dashscope.aliyuncs.com/api-ws/v1/realtime".to_string(),
        }
    }
}

/// 会话更新事件
#[derive(Debug, Serialize)]
struct SessionUpdateEvent {
    event_id: String,
    #[serde(rename = "type")]
    event_type: String,
    session: SessionConfig,
}

/// 音频数据追加事件
#[derive(Debug, Serialize)]
struct AudioAppendEvent {
    event_id: String,
    #[serde(rename = "type")]
    event_type: String,
    audio: String,
}

/// 会话配置
#[derive(Debug, Serialize)]
struct SessionConfig {
    modalities: Vec<String>,
    input_audio_format: String,
    sample_rate: u32,
    input_audio_transcription: InputAudioTranscription,
    turn_detection: TurnDetection,
}

#[derive(Debug, Serialize)]
struct InputAudioTranscription {
    language: String,
}

#[derive(Debug, Serialize)]
struct TurnDetection {
    #[serde(rename = "type")]
    detection_type: String,
    threshold: f32,
    silence_duration_ms: u64,
}

/// 服务端返回的事件
#[derive(Debug, Deserialize)]
struct ServerEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    item: Option<Item>,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Item {
    #[serde(default)]
    transcripts: Vec<Transcript>,
}

#[derive(Debug, Deserialize)]
struct Transcript {
    #[serde(default)]
    text: String,
}

/// ASR 客户端
pub struct AsrClient {
    config: AsrConfig,
}

impl AsrClient {
    /// 创建新的 ASR 客户端
    pub fn new(config: AsrConfig) -> Self {
        Self { config }
    }

    /// 从音频文件识别语音
    pub async fn recognize_file(&self, audio_path: &str) -> Result<String> {
        let url_str = format!("{}?model={}", self.config.base_url, self.config.model);
        let url = Url::parse(&url_str)?;

        let host = url.host_str().ok_or_else(|| anyhow::anyhow!("Invalid URL: missing host"))?;

        let request = Request::builder()
            .method("GET")
            .uri(url.as_str())
            .header("Host", host)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("OpenAI-Beta", "realtime=v1")
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==")
            .body(())?;

        let (ws_stream, _) = connect_async(request).await?;
        let (mut write, mut read) = ws_stream.split();

        // 发送会话配置
        let session_config = SessionConfig {
            modalities: vec!["text".to_string()],
            input_audio_format: "pcm".to_string(),
            sample_rate: 16000,
            input_audio_transcription: InputAudioTranscription {
                language: "zh".to_string(),
            },
            turn_detection: TurnDetection {
                detection_type: "server_vad".to_string(),
                threshold: 0.2,
                silence_duration_ms: 800,
            },
        };

        let session_event = SessionUpdateEvent {
            event_id: "event_init".to_string(),
            event_type: "session.update".to_string(),
            session: session_config,
        };

        let event_json = serde_json::to_string(&session_event)?;
        write.send(Message::Text(event_json.into())).await?;
        println!("喵！会话配置已发送~ 🐾");

        // 读取并发送音频数据
        let mut audio_file = File::open(audio_path)?;
        let mut buffer = vec![0u8; 3200]; // 每次读取 3200 字节
        let mut event_counter = 0u64;

        loop {
            let n = audio_file.read(&mut buffer)?;
            if n == 0 {
                println!("喵！音频文件读取完毕~");
                break;
            }

            let audio_data = &buffer[..n];
            let encoded = BASE64.encode(audio_data);

            let audio_event = AudioAppendEvent {
                event_id: format!("event_{}", event_counter),
                event_type: "input_audio_buffer.append".to_string(),
                audio: encoded,
            };

            let event_json = serde_json::to_string(&audio_event)?;
            write.send(Message::Text(event_json.into())).await?;

            event_counter += 1;

            // 模拟实时音频采集，发送间隔 100ms
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // 等待识别结果
        let mut result_text = String::new();
        let timeout = tokio::time::sleep(Duration::from_secs(30));
        tokio::pin!(timeout);

        loop {
            tokio::select! {
                _ = &mut timeout => {
                    println!("喵！等待识别结果超时...");
                    break;
                }
                msg = read.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            if let Ok(server_event) = serde_json::from_str::<ServerEvent>(&text) {
                                // 只打印重要的事件
                                if !server_event.event_type.contains("session.") {
                                    println!("喵！收到服务器消息：{} (๑•̀ㅂ•́)و✧", text);
                                }

                                if server_event.event_type == "conversation.item.input_audio_transcription.text" {
                                    if let Some(text) = server_event.text {
                                        if !text.is_empty() {
                                            result_text = text;
                                            println!("喵！识别到文本：{} (=・ω・=)", result_text);
                                        }
                                    }
                                }
                            }
                        }
                        Some(Ok(Message::Close(_))) => {
                            println!("喵！服务器关闭了连接~");
                            break;
                        }
                        Some(Err(e)) => {
                            println!("喵！WebSocket 错误：{} (⊙x⊙;)", e);
                            break;
                        }
                        None => {
                            println!("喵！连接已关闭~");
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(result_text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[tokio::test]
    async fn test_asr_recognition() {
        println!("🐾 喵！开始 ASR 单元测试~");

        // 1. 加载配置
        let config = Config::from_yaml("conf.yaml")
            .expect("喵！无法加载配置文件 conf.yaml (⊙x⊙;)");

        // 2. 创建 ASR 客户端
        let asr_config = AsrConfig {
            api_key: config.DASHSCOPE_API_KEY.clone(),
            ..Default::default()
        };

        let client = AsrClient::new(asr_config);

        // 3. 识别音频文件
        let audio_path = "/home/kals/下载/zh_prompt.wav";
        let result = client.recognize_file(audio_path).await;

        // 4. 验证结果
        assert!(result.is_ok(), "喵！ASR 识别失败了呜呜呜...");

        let text = result.unwrap();
        println!("🐾 喵！识别结果：{}", text);

        // 验证识别到了中文文本
        assert!(!text.is_empty(), "喵！识别结果为空...");
        assert!(text.contains("对") || text.contains("账单") || text.contains("处理"),
                "喵！识别结果不符合预期，应该包含相关词汇");

        println!("🐾 喵！测试通过啦！ (=・ω・=)");
    }
}
