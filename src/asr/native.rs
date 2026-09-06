use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use std::time::Duration;

use crate::audio::pcm_to_wav;
use super::engine::AsrEngine;

pub const DEFAULT_ASR_ENDPOINT: &str = "https://llm-y3exskfcgxgxzn23.cn-beijing.maas.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation";
pub const DEFAULT_ASR_MODEL: &str = "qwen-audio-3.0-asr-flash";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const TOTAL_TIMEOUT: Duration = Duration::from_secs(120);

/// 原生 DashScope HTTP 多模态生成 ASR 引擎（单次 POST，无 WebSocket、无流式）
pub struct NativeHttpAsrEngine {
    api_key: String,
    endpoint: String,
    model: String,
    client: reqwest::Client,
}

impl NativeHttpAsrEngine {
    pub fn new(api_key: impl Into<String>, endpoint: impl Into<String>, model: impl Into<String>) -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(TOTAL_TIMEOUT)
            .build()
            .expect("构建 HTTP 客户端失败");
        Self {
            api_key: api_key.into(),
            endpoint: endpoint.into(),
            model: model.into(),
            client,
        }
    }
}

#[async_trait::async_trait]
impl AsrEngine for NativeHttpAsrEngine {
    async fn recognize(&self, audio_data: &[u8]) -> Result<String> {
        // 1. PCM(16bit 单声道 16kHz) 转 WAV
        let wav_data = pcm_to_wav(audio_data, 16000, 1);

        // 2. WAV 转 base64 data URL
        let audio_data_url = format!("data:audio/wav;base64,{}", BASE64.encode(&wav_data));

        // 3. 构造请求体（DashScope 多模态生成格式）
        let request_body = serde_json::json!({
            "model": self.model,
            "input": {
                "messages": [
                    {
                        "role": "user",
                        "content": [
                            {
                                "type": "input_audio",
                                "input_audio": {
                                    "data": audio_data_url
                                }
                            }
                        ]
                    }
                ]
            },
            "parameters": {
                "format": "wav",
                "sample_rate": "16000"
            }
        });

        // 4. 发送 POST 请求
        println!("[INFO] 发送原生 HTTP ASR 请求: {}", self.endpoint);
        let response = self.client
            .post(&self.endpoint)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .header("X-DashScope-SSE", "disable")
            .body(serde_json::to_string(&request_body)?)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            println!("[ERROR] ASR API 返回错误 - 状态码: {}, 响应: {}", status, error_text);
            return Err(anyhow::anyhow!("ASR API 错误 [{}]: {}", status, error_text));
        }

        // 5. 解析响应：顶层 text，缺失时回退 output.text
        let response_json: serde_json::Value = match response.json().await {
            Ok(json) => json,
            Err(e) => {
                println!("[ERROR] 解析响应 JSON 失败: {}", e);
                return Err(anyhow::anyhow!("解析响应失败: {}", e));
            }
        };

        let result_text = response_json
            .get("text")
            .and_then(|v| v.as_str())
            .or_else(|| {
                response_json
                    .get("output")
                    .and_then(|o| o.get("text"))
                    .and_then(|v| v.as_str())
            })
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("响应缺少识别文本字段: {}", response_json))?;

        if result_text.is_empty() {
            println!("[WARN] 识别结果为空");
        }

        Ok(result_text)
    }
}
