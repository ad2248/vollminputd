use anyhow::Result;
use std::fs;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub DASHSCOPE_API_KEY: String,

    #[serde(default = "default_max_recording_seconds")]
    pub max_recording_seconds: u64,

    #[serde(default = "default_audio_sample_rate")]
    pub audio_sample_rate: u32,

    #[serde(default = "default_audio_channels")]
    pub audio_channels: u16,

    #[serde(default)]
    pub asr_strategy: AsrStrategy,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AsrStrategy {
    DashscopeRealtime,
    OmniPlus,
}

impl Default for AsrStrategy {
    fn default() -> Self {
        AsrStrategy::DashscopeRealtime
    }
}

fn default_max_recording_seconds() -> u64 {
    60
}

fn default_audio_sample_rate() -> u32 {
    16000
}

fn default_audio_channels() -> u16 {
    1
}

impl Config {
    /// 从 conf.yaml 加载配置
    pub fn from_yaml(path: &str) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&content)?;
        Ok(config)
    }
}