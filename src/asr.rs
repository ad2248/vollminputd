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
    use std::io::Read;

    #[tokio::test]
    #[ignore = "Requires real API key and audio file"]
    async fn test_asr_stream_recognition() {
        println!("🐾 喵！开始 ASR 流式识别单元测试~");

        // 1. 加载配置
        let config = Config::from_yaml("conf.yaml")
            .expect("喵！无法加载配置文件 conf.yaml (⊙x⊙;)");

        // 2. 创建 ASR 客户端
        let asr_config = AsrConfig {
            api_key: config.DASHSCOPE_API_KEY.clone(),
            ..Default::default()
        };

        let client = DashScopeAsrEngine::new(asr_config);

        // 3. 读取音频文件到内存
        let audio_path = "/home/kals/下载/zh_prompt.wav";
        let mut audio_file = std::fs::File::open(audio_path)
            .expect("喵！无法打开音频文件 (⊙x⊙;)");
        let mut audio_buffer = Vec::new();
        audio_file.read_to_end(&mut audio_buffer)
            .expect("喵！无法读取音频文件 (⊙x⊙;)");

        println!("🐾 喵！音频文件大小：{} 字节", audio_buffer.len());

        // 4. 模拟实时音频采集：将音频数据分成小块，每次发送 3200 字节（约 200ms）
        let chunk_size = 3200; // 16kHz 采样率，16bit，单声道，200ms = 3200 字节
        let audio_chunks: Vec<Vec<u8>> = audio_buffer
            .chunks(chunk_size)
            .map(|chunk| chunk.to_vec())
            .collect();

        println!("🐾 喵！音频数据分成 {} 块，准备发送~", audio_chunks.len());

        // 5. 流式识别 - 启动会话
        let mut session = client.start_recognition().await
            .expect("喵！无法启动识别会话 (⊙x⊙;)");

        // 6. 逐个发送音频片段
        for (i, chunk) in audio_chunks.iter().enumerate() {
            session.send_audio_chunk(chunk).await
                .expect("喵！发送音频片段失败 (⊙x⊙;)");
            println!("🐾 喵！已发送第 {} 个片段", i + 1);

            // 模拟实时音频采集，发送间隔 100ms
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // 7. 完成并获取识别结果
        let result = session.finish_and_wait_result().await
            .expect("喵！获取识别结果失败 (⊙x⊙;)");

        println!("🐾 喵！识别结果：{}", result);

        // 8. 验证结果
        assert!(!result.is_empty(), "喵！识别结果为空...");
        assert!(result.contains("对") || result.contains("账单") || result.contains("处理"),
                "喵！识别结果不符合预期，应该包含相关词汇");

        println!("🐾 喵！测试通过啦！ (=・ω・=)");
    }
}
