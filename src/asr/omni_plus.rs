use anyhow::Result;
use futures_util::StreamExt;
use reqwest::multipart;
use serde::Deserialize;

use super::engine::AsrEngine;
use crate::audio::pcm_to_wav;

/// OmniPlus ASR 配置
#[derive(Debug, Clone)]
pub struct OmniPlusConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub prompt: String,
}

impl Default for OmniPlusConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string(),
            model: "qwen3.5-omni-plus".to_string(),
            prompt: "请转录这段音频。注意理解用户说的话，不是机械地逐字转录，而是在不改变用户意思的情况下稍加润色".to_string(),
        }
    }
}

/// SSE 事件数据结构
#[derive(Debug, Deserialize)]
struct SseChoice {
    delta: SseDelta,
}

#[derive(Debug, Deserialize)]
struct SseDelta {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SseEvent {
    choices: Vec<SseChoice>,
}

/// OmniPlus ASR 引擎（HTTP SSE 流式识别）
pub struct OmniPlusAsrEngine {
    config: OmniPlusConfig,
    client: reqwest::Client,
}

impl OmniPlusAsrEngine {
    pub fn new(config: OmniPlusConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl AsrEngine for OmniPlusAsrEngine {
    async fn recognize(&self, audio_data: &[u8]) -> Result<String> {
        // 1. PCM 转 WAV
        let wav_data = pcm_to_wav(audio_data, 16000, 1);
        println!("[INFO] PCM({} 字节) → WAV({} 字节)", audio_data.len(), wav_data.len());

        // 2. 构造 multipart form
        let form = multipart::Form::new()
            .text("model", self.config.model.clone())
            .text("messages", format!(
                "[{{\"role\": \"user\", \"content\": \"{}\"}}]",
                self.config.prompt
            ))
            .text("modalities", "[\"text\"]")
            .text("stream", "true")
            .part("audio", multipart::Part::bytes(wav_data)
                .file_name("audio.wav")
                .mime_str("audio/wav")?);

        // 3. 发送 POST 请求
        println!("[INFO] 发送 OmniPlus 识别请求...");
        let response = self.client
            .post(&self.config.base_url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .multipart(form)
            .send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("OmniPlus API 错误: {}", error_text));
        }

        // 4. 解析 SSE 流
        let mut stream = response.bytes_stream();
        let mut result_text = String::new();
        let mut last_content = String::new();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            let text = String::from_utf8_lossy(&chunk);

            // 解析 SSE 格式：data: {json}
            for line in text.lines() {
                let line = line.trim();
                if line.starts_with("data: ") {
                    let json_str = &line[6..];
                    
                    // 检查是否结束
                    if json_str == "[DONE]" {
                        println!("[INFO] SSE 流结束");
                        break;
                    }

                    // 解析 JSON
                    if let Ok(event) = serde_json::from_str::<SseEvent>(json_str) {
                        if let Some(choice) = event.choices.first() {
                            if let Some(content) = &choice.delta.content {
                                if !content.is_empty() && content != &last_content {
                                    // 追加新内容
                                    result_text.push_str(content);
                                    last_content = content.clone();
                                    println!("[INFO] 中间结果：{}", result_text);
                                }
                            }
                        }
                    }
                }
            }
        }

        println!("[INFO] OmniPlus 识别完成：{}", result_text);
        Ok(result_text)
    }
}