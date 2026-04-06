use serde::Deserialize;
use std::fs;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub DASHSCOPE_API_KEY: String,
}

impl Config {
    /// 从 conf.yaml 加载配置
    pub fn from_yaml(path: &str) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&content)?;
        Ok(config)
    }
}
