use anyhow::Result;
use futures_util::StreamExt;
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

/// OmniPlus ASR 引擎（单步直接识别）
/// 
/// 直接将音频发送给多模态大模型，由模型完成转录和润色。
/// 参考官方示例，使用 OpenAI 兼容接口，JSON 格式，input_audio 类型。
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

        // 2. WAV 转 base64，构造 data URL（格式: data:;base64,...）
        use base64::{engine::general_purpose::STANDARD, Engine};
        let audio_base64 = STANDARD.encode(&wav_data);
        let audio_data_url = format!("data:;base64,{}", audio_base64);
        println!("[INFO] Base64 编码后长度: {} 字符", audio_base64.len());
        println!("[DEBUG] Data URL 前 100 字符: {}", &audio_data_url[..100.min(audio_data_url.len())]);

        // 3. 构造 JSON 请求体（严格参考官方 curl 示例格式）
        let request_body = serde_json::json!({
            "model": self.config.model,
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "text",
                            "text": self.config.prompt
                        },
                        {
                            "type": "input_audio",
                            "input_audio": {
                                "data": audio_data_url
                            }
                        }
                    ]
                }
            ]
        });

        // 4. 发送 POST 请求
        let url = format!("{}/chat/completions", self.config.base_url);
        println!("[INFO] 发送 OmniPlus 识别请求到: {}", url);
        println!("[DEBUG] 请求模型: {}", self.config.model);
        
        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send().await;

        let response = match response {
            Ok(resp) => resp,
            Err(e) => {
                println!("[ERROR] 请求发送失败: {}", e);
                if e.is_timeout() {
                    return Err(anyhow::anyhow!("请求超时"));
                }
                if e.is_connect() {
                    return Err(anyhow::anyhow!("连接失败: {}", e));
                }
                return Err(anyhow::anyhow!("请求失败: {}", e));
            }
        };

        println!("[DEBUG] HTTP 状态码: {}", response.status());

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|e| {
                println!("[WARN] 无法读取错误响应体: {}", e);
                "无法读取错误内容".to_string()
            });
            println!("[ERROR] OmniPlus API 返回错误 - 状态码: {}, 响应: {}", status, error_text);
            return Err(anyhow::anyhow!("OmniPlus API 错误 [{}]: {}", status, error_text));
        }

        // 5. 解析响应（非流式）
        let response_json: serde_json::Value = match response.json().await {
            Ok(json) => json,
            Err(e) => {
                println!("[ERROR] 解析响应 JSON 失败: {}", e);
                return Err(anyhow::anyhow!("解析响应失败: {}", e));
            }
        };

        println!("[DEBUG] 响应 JSON: {}", response_json.to_string());

        // 检查错误
        if let Some(error) = response_json.get("error") {
            println!("[ERROR] API 返回错误: {}", error);
            return Err(anyhow::anyhow!("API 错误: {}", error));
        }

        // 提取文本结果
        let result_text = response_json
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|msg| msg.get("content"))
            .and_then(|content| {
                // 内容可能是字符串或数组
                if let Some(text) = content.as_str() {
                    Some(text.to_string())
                } else if let Some(arr) = content.as_array() {
                    let mut texts = Vec::new();
                    for item in arr {
                        if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                            texts.push(text.to_string());
                        }
                    }
                    Some(texts.join(""))
                } else {
                    None
                }
            })
            .unwrap_or_default();

        println!("[INFO] OmniPlus 识别完成，结果: {}", result_text);
        
        if result_text.is_empty() {
            println!("[WARN] 识别结果为空");
        }
        
        Ok(result_text)
    }
}