use anyhow::{Context, Result};
use std::env;

use crate::asr::{DEFAULT_ASR_ENDPOINT, DEFAULT_ASR_MODEL};

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub dashscope_api_key: String,
    pub max_recording_seconds: u64,
    pub audio_sample_rate: u32,
    pub audio_channels: u16,
    /// ASR 服务完整端点 URL
    pub asr_endpoint: String,
    /// ASR 模型名称
    pub asr_model: String,
}

impl Config {
    /// 从环境变量加载配置
    pub fn from_env() -> Result<Self> {
        let dashscope_api_key = env::var("VOLLMINPUTD_DASHSCOPE_API_KEY")
            .context("环境变量 VOLLMINPUTD_DASHSCOPE_API_KEY 未设置")?;

        let max_recording_seconds = env::var("VOLLMINPUTD_MAX_RECORDING_SECONDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(60);

        let audio_sample_rate = env::var("VOLLMINPUTD_AUDIO_SAMPLE_RATE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(16000);

        let audio_channels = env::var("VOLLMINPUTD_AUDIO_CHANNELS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1);

        let asr_endpoint = env::var("VOLLMINPUTD_ASR_ENDPOINT")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_ASR_ENDPOINT.to_string());

        let asr_model = env::var("VOLLMINPUTD_ASR_MODEL")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_ASR_MODEL.to_string());

        Ok(Config {
            dashscope_api_key,
            max_recording_seconds,
            audio_sample_rate,
            audio_channels,
            asr_endpoint,
            asr_model,
        })
    }
}
