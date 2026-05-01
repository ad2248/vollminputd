use anyhow::Result;
use futures_util::StreamExt;
use reqwest::multipart;
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
        let url = format!("{}/chat/completions", self.config.base_url);
        println!("[INFO] 发送 OmniPlus 识别请求到: {}", url);
        println!("[DEBUG] 请求模型: {}", self.config.model);
        println!("[DEBUG] API Key 前8位: {}...", &self.config.api_key[..self.config.api_key.len().min(8)]);
        
        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .multipart(form)
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
        println!("[DEBUG] HTTP 响应头: {:?}", response.headers());

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|e| {
                println!("[WARN] 无法读取错误响应体: {}", e);
                "无法读取错误内容".to_string()
            });
            println!("[ERROR] OmniPlus API 返回错误 - 状态码: {}, 响应: {}", status, error_text);
            return Err(anyhow::anyhow!("OmniPlus API 错误 [{}]: {}", status, error_text));
        }

        // 4. 解析 SSE 流
        let mut stream = response.bytes_stream();
        let mut result_text = String::new();
        let mut chunk_count = 0;

        println!("[INFO] 开始解析 SSE 流...");

        while let Some(chunk_result) = stream.next().await {
            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) => {
                    println!("[ERROR] 读取 SSE 流失败: {}", e);
                    break;
                }
            };
            
            chunk_count += 1;
            let text = String::from_utf8_lossy(&chunk);

            // 解析 SSE 格式：data: {json}
            for line in text.lines() {
                let line = line.trim();
                
                if line.is_empty() {
                    continue;
                }
                
                if line.starts_with("data: ") {
                    let json_str = &line[6..];
                    
                    // 检查是否结束
                    if json_str == "[DONE]" {
                        println!("[INFO] SSE 流结束标记 [DONE]");
                        break;
                    }

                    // 解析 JSON
                    match serde_json::from_str::<serde_json::Value>(json_str) {
                        Ok(json_value) => {
                            // 打印原始 JSON 用于调试
                            if chunk_count <= 3 {
                                println!("[DEBUG] SSE 原始数据 (chunk {}): {}", chunk_count, json_value.to_string());
                            }
                            
                            // 检查是否有错误
                            if let Some(error) = json_value.get("error") {
                                println!("[ERROR] API 返回错误: {}", error);
                                continue;
                            }
                            
                            // 提取内容
                            if let Some(choices) = json_value.get("choices") {
                                if let Some(choice) = choices.as_array().and_then(|arr| arr.first()) {
                                    if let Some(delta) = choice.get("delta") {
                                        if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                                            if !content.is_empty() {
                                                result_text.push_str(content);
                                                println!("[INFO] 中间结果：{}", result_text);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            println!("[WARN] 无法解析 JSON (chunk {}): {} - 原始数据: {}", chunk_count, e, json_str);
                        }
                    }
                } else if line.starts_with("event:") || line.starts_with("id:") || line.starts_with(":") {
                    // SSE 控制行，忽略
                    continue;
                } else {
                    println!("[DEBUG] 未知 SSE 行: {}", line);
                }
            }
        }

        println!("[INFO] OmniPlus 识别完成，总 chunk 数: {}，结果: {}", chunk_count, result_text);
        
        if result_text.is_empty() {
            println!("[WARN] 识别结果为空");
        }
        
        Ok(result_text)
    }
}