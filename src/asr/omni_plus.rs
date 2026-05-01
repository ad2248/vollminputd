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
            model: "qwen-plus".to_string(),
            prompt: "请对以下文本进行润色。注意理解原意，不是机械地逐字修改，而是在不改变原意的情况下让表达更自然流畅。只输出润色后的文本，不要添加任何解释：".to_string(),
        }
    }
}

/// OmniPlus ASR 引擎（两步式：先转录，后润色）
/// 
/// 由于 OmniPlus 模型在 OpenAI 兼容端点不支持直接上传 base64 音频，
/// 我们采用两步策略：
/// 1. 使用 DashScope Realtime API 进行音频转录
/// 2. 使用文本模型对转录结果进行润色
pub struct OmniPlusAsrEngine {
    config: OmniPlusConfig,
    client: reqwest::Client,
    // 内置 realtime 引擎用于第一步转录
    realtime_engine: super::realtime::DashScopeRealtimeAsrEngine,
}

impl OmniPlusAsrEngine {
    pub fn new(config: OmniPlusConfig) -> Self {
        // 创建 realtime 引擎用于转录
        let realtime_config = super::realtime::AsrConfig {
            api_key: config.api_key.clone(),
            base_url: "wss://dashscope.aliyuncs.com/api-ws/v1/realtime".to_string(),
            model: "qwen3-asr-flash-realtime".to_string(),
        };
        let realtime_engine = super::realtime::DashScopeRealtimeAsrEngine::new(realtime_config);
        
        Self {
            config,
            client: reqwest::Client::new(),
            realtime_engine,
        }
    }

    /// 第二步：使用文本模型润色转录结果
    async fn polish_text(&self, raw_text: &str) -> Result<String> {
        if raw_text.trim().is_empty() {
            return Ok(raw_text.to_string());
        }

        let request_body = serde_json::json!({
            "model": self.config.model,
            "messages": [
                {
                    "role": "system",
                    "content": self.config.prompt
                },
                {
                    "role": "user",
                    "content": raw_text
                }
            ],
            "stream": false
        });

        let url = format!("{}/chat/completions", self.config.base_url);
        println!("[INFO] 发送润色请求到: {}", url);
        println!("[DEBUG] 润色模型: {}", self.config.model);
        
        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send().await?;

        println!("[DEBUG] 润色 HTTP 状态码: {}", response.status());

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "无法读取错误内容".to_string());
            println!("[ERROR] 润色 API 错误 [{}]: {}", status, error_text);
            // 如果润色失败，返回原始文本
            return Ok(raw_text.to_string());
        }

        let response_json: serde_json::Value = response.json().await?;
        
        // 提取润色后的文本
        let polished = response_json
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|msg| msg.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or(raw_text);

        println!("[INFO] 润色完成");
        println!("[DEBUG] 原始文本: {}", raw_text);
        println!("[DEBUG] 润色结果: {}", polished);

        Ok(polished.to_string())
    }
}

#[async_trait::async_trait]
impl AsrEngine for OmniPlusAsrEngine {
    async fn recognize(&self, audio_data: &[u8]) -> Result<String> {
        println!("[INFO] OmniPlus 两步识别开始...");
        
        // 第一步：使用 realtime 引擎转录
        println!("[INFO] 步骤 1/2: 音频转录...");
        let raw_text = match self.realtime_engine.recognize(audio_data).await {
            Ok(text) => {
                println!("[INFO] 转录完成: {}", text);
                text
            }
            Err(e) => {
                println!("[ERROR] 转录失败: {}", e);
                return Err(e);
            }
        };

        if raw_text.trim().is_empty() {
            println!("[WARN] 转录结果为空，跳过润色");
            return Ok(raw_text);
        }

        // 第二步：润色
        println!("[INFO] 步骤 2/2: 文本润色...");
        let polished_text = self.polish_text(&raw_text).await?;
        
        println!("[INFO] OmniPlus 识别完成，最终结果: {}", polished_text);
        Ok(polished_text)
    }
}