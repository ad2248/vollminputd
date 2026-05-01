use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio_tungstenite::{
    connect_async,
    tungstenite::http::Request,
    tungstenite::Message,
    WebSocketStream,
};
use tokio::net::TcpStream;
use url::Url;

use super::engine::AsrEngine;

/// DashScope ASR 配置
#[derive(Debug, Clone)]
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
    #[serde(default)]
    stash: Option<String>,
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

/// ASR 识别会话
pub struct RecognitionSession {
    write: futures_util::stream::SplitSink<WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>, Message>,
    read: futures_util::stream::SplitStream<WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>>,
    event_counter: u64,
}

/// DashScope 实时 ASR 引擎
pub struct DashScopeRealtimeAsrEngine {
    config: AsrConfig,
}

impl DashScopeRealtimeAsrEngine {
    pub fn new(config: AsrConfig) -> Self {
        Self { config }
    }

    /// 启动一个识别会话
    pub async fn start_recognition(&self) -> Result<RecognitionSession> {
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
        let (write, read) = ws_stream.split();

        let mut session = RecognitionSession {
            write,
            read,
            event_counter: 0,
        };

        // 发送会话配置
        session.init_session().await?;

        Ok(session)
    }
}

#[async_trait::async_trait]
impl AsrEngine for DashScopeRealtimeAsrEngine {
    async fn recognize(&self, audio_data: &[u8]) -> Result<String> {
        let mut session = self.start_recognition().await?;
        
        // 分块发送音频，避免单条 WebSocket 消息过大
        // 每块 6400 字节原始 PCM = 200ms @ 16kHz 16bit mono
        // Base64 后约 8533 字节
        const CHUNK_SIZE: usize = 6400;
        let total_chunks = audio_data.len().div_ceil(CHUNK_SIZE);
        
        println!("[INFO] 发送音频数据：{} 字节，分 {} 块", audio_data.len(), total_chunks);
        
        for (i, chunk) in audio_data.chunks(CHUNK_SIZE).enumerate() {
            session.send_audio_chunk(chunk).await?;
            if (i + 1) % 10 == 0 || i + 1 == total_chunks {
                println!("[INFO] 已发送 {} / {} 块", i + 1, total_chunks);
            }
            // 每块间隔 50ms，给服务器处理时间
            if i + 1 < total_chunks {
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            }
        }
        
        println!("[INFO] 音频发送完成，等待识别结果...");
        
        session.finish_and_wait_result(audio_data.len()).await
    }
}

impl RecognitionSession {
    /// 初始化会话配置
    async fn init_session(&mut self) -> Result<()> {
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
        self.write.send(Message::Text(event_json.into())).await?;
        println!("[INFO] ASR 会话配置已发送");

        Ok(())
    }

    /// 发送音频数据片段
    pub async fn send_audio_chunk(&mut self, chunk: &[u8]) -> Result<()> {
        if chunk.is_empty() {
            return Ok(());
        }

        let encoded = BASE64.encode(chunk);

        let audio_event = AudioAppendEvent {
            event_id: format!("event_{}", self.event_counter),
            event_type: "input_audio_buffer.append".to_string(),
            audio: encoded,
        };

        let event_json = serde_json::to_string(&audio_event)?;
        self.write.send(Message::Text(event_json.into())).await?;

        self.event_counter += 1;

        Ok(())
    }

    /// 计算动态超时时间
    /// - 音频长度 × 3
    /// - 下限 20 秒
    /// - 上限 300 秒
    fn calculate_timeout(audio_bytes: usize) -> u64 {
        // 16kHz, 16bit, mono = 32000 bytes/second
        let audio_seconds = audio_bytes as f64 / 32000.0;
        let timeout = (audio_seconds * 3.0).ceil() as u64;
        timeout.clamp(20, 300)
    }

    /// 完成音频发送并等待识别结果
    pub async fn finish_and_wait_result(mut self, audio_bytes: usize) -> Result<String> {
        let timeout_secs = Self::calculate_timeout(audio_bytes);
        println!("[INFO] 等待识别结果（超时：{} 秒）", timeout_secs);

        let mut result_text = String::new();
        let mut last_update = tokio::time::Instant::now();
        let mut check_interval = tokio::time::interval(Duration::from_secs(1));
        let timeout = tokio::time::sleep(Duration::from_secs(timeout_secs));
        tokio::pin!(timeout);

        loop {
            tokio::select! {
                _ = &mut timeout => {
                    println!("[WARN] 等待识别结果超时（{} 秒）", timeout_secs);
                    break;
                }
                _ = check_interval.tick() => {
                    // 定期检查：如果结果已稳定 3 秒，提前退出
                    if !result_text.is_empty() && last_update.elapsed().as_secs() >= 3 {
                        println!("[INFO] 结果已稳定 {} 秒，提前结束", last_update.elapsed().as_secs());
                        break;
                    }
                }
                msg = self.read.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            if let Ok(server_event) = serde_json::from_str::<ServerEvent>(&text) {
                                // 打印所有非 session 事件（用于调试）
                                if !server_event.event_type.contains("session.") {
                                    println!("[DEBUG] 服务器事件：{}", server_event.event_type);
                                    // 打印 error 事件的详细信息
                                    if server_event.event_type == "error" {
                                        println!("[WARN] 服务器错误：{}", text);
                                    }
                                }

                                // 处理文本结果
                                if server_event.event_type == "conversation.item.input_audio_transcription.text" {
                                    let text = server_event.text.unwrap_or_default();
                                    let stash = server_event.stash.unwrap_or_default();
                                    let full_text = format!("{}{}", text, stash);
                                    if !full_text.is_empty() && full_text != result_text {
                                        result_text = full_text;
                                        last_update = tokio::time::Instant::now();
                                        println!("[INFO] 中间结果：{}", result_text);
                                    }
                                }
                                
                                // 检测完成事件（尝试多种可能的事件名）
                                if server_event.event_type.contains("completed")
                                    || server_event.event_type.contains("finished")
                                    || server_event.event_type.contains("done") {
                                    println!("[INFO] 收到完成事件：{}", server_event.event_type);
                                    break;
                                }
                            }
                        }
                        Some(Ok(Message::Close(_))) => {
                            println!("[INFO] 服务器关闭连接");
                            break;
                        }
                        Some(Err(e)) => {
                            println!("[ERROR] WebSocket 错误：{}", e);
                            break;
                        }
                        None => {
                            println!("[INFO] 连接已关闭");
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }

        println!("[INFO] 最终识别结果：{}", result_text);
        Ok(result_text)
    }
}