use anyhow::{Context, Result};
use std::env;

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub dashscope_api_key: String,
    pub max_recording_seconds: u64,
    pub audio_sample_rate: u32,
    pub audio_channels: u16,
    pub asr_strategy: AsrStrategy,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AsrStrategy {
    DashscopeRealtime,
    OmniPlus,
}

impl Default for AsrStrategy {
    fn default() -> Self {
        AsrStrategy::DashscopeRealtime
    }
}

impl Config {
    /// 从环境变量加载配置
    pub fn from_env() -> Result<Self> {
        let dashscope_api_key = env::var("VOICEINPUT_DASHSCOPE_API_KEY")
            .context("环境变量 VOICEINPUT_DASHSCOPE_API_KEY 未设置")?;

        let asr_strategy = match env::var("VOICEINPUT_ASR_STRATEGY").ok().as_deref() {
            Some("omni_plus") => AsrStrategy::OmniPlus,
            Some("dashscope_realtime") | None => AsrStrategy::DashscopeRealtime,
            Some(other) => {
                return Err(anyhow::anyhow!(
                    "无效的环境变量 VOICEINPUT_ASR_STRATEGY: {}。可选值: dashscope_realtime, omni_plus",
                    other
                ));
            }
        };

        let max_recording_seconds = env::var("VOICEINPUT_MAX_RECORDING_SECONDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(60);

        let audio_sample_rate = env::var("VOICEINPUT_AUDIO_SAMPLE_RATE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(16000);

        let audio_channels = env::var("VOICEINPUT_AUDIO_CHANNELS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1);

        Ok(Config {
            dashscope_api_key,
            max_recording_seconds,
            audio_sample_rate,
            audio_channels,
            asr_strategy,
        })
    }
}