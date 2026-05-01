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

#[mockall::automock]
#[async_trait::async_trait]
pub trait AsrEngine: Send + Sync {
    async fn recognize(&self, audio_data: &[u8]) -> anyhow::Result<String>;
}

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
pub struct DashScopeAsrEngine {
    config: AsrConfig,
}

/// ASR 识别会话
pub struct RecognitionSession {
    write: futures_util::stream::SplitSink<WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>, Message>,
    read: futures_util::stream::SplitStream<WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>>,
    event_counter: u64,
}

#[async_trait::async_trait]
impl AsrEngine for DashScopeAsrEngine {
    async fn recognize(&self, audio_data: &[u8]) -> anyhow::Result<String> {
        let mut session = self.start_recognition().await?;
        
        // Send audio in chunks (3200 bytes ≈ 200ms at 16kHz 16bit mono)
        let chunk_size = 3200;
        for chunk in audio_data.chunks(chunk_size) {
            session.send_audio_chunk(chunk).await?;
            // Small delay to simulate streaming
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
        
        session.finish_and_wait_result().await
    }
}

impl DashScopeAsrEngine {
    /// 创建新的 ASR 客户端
    pub fn new(config: AsrConfig) -> Self {
        Self { config }
    }

    /// 启动一个识别会话
    ///
    /// # 示例
    /// ```no_run
    /// use VoiceInput::asr::{DashScopeAsrEngine, AsrConfig, RecognitionSession};
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let config = AsrConfig::default();
    /// let client = DashScopeAsrEngine::new(config);
    ///
    /// // 启动会话
    /// let mut session: RecognitionSession = client.start_recognition().await?;
    ///
    /// // 发送音频片段
    /// let audio_chunk = vec![0u8; 3200];
    /// session.send_audio_chunk(&audio_chunk).await?;
    ///
    /// // 完成并获取结果
    /// let result = session.finish_and_wait_result().await?;
    /// # Ok(())
    /// # }
    /// ```
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
        println!("喵！会话配置已发送~ 🐾");

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

    /// 完成音频发送并等待识别结果
    pub async fn finish_and_wait_result(mut self) -> Result<String> {
        println!("喵！音频数据流发送完毕~");

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
                msg = self.read.next() => {
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
    #[ignore = "Requires real API key"]
    async fn test_asr_real_recognition() {
        println!("[INFO] 开始真实 ASR 识别测试");

        // 加载配置
        let config = Config::from_yaml("conf.yaml")
            .expect("无法加载配置文件 conf.yaml");

        if config.DASHSCOPE_API_KEY.is_empty() {
            panic!("conf.yaml 中 DASHSCOPE_API_KEY 不能为空");
        }

        // 创建 ASR 客户端
        let asr_config = AsrConfig {
            api_key: config.DASHSCOPE_API_KEY.clone(),
            ..Default::default()
        };
        let client = DashScopeAsrEngine::new(asr_config);

        // 读取测试音频
        let audio_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/test_audio.wav");
        let audio_data = std::fs::read(audio_path)
            .expect("无法读取测试音频文件");

        println!("[INFO] 音频文件大小：{} 字节", audio_data.len());

        // 执行识别
        let result = client.recognize(&audio_data).await
            .expect("识别失败");

        println!("[INFO] 识别结果：{}", result);

        // 验证结果
        assert!(!result.is_empty(), "识别结果不应为空");
        
        // 预期结果包含这些关键词
        let expected_keywords = ["对", "账单", "处理"];
        for keyword in &expected_keywords {
            assert!(
                result.contains(keyword),
                "识别结果应包含 '{}'，实际结果：{}",
                keyword,
                result
            );
        }

        println!("[INFO] ASR 测试通过");
    }
}
