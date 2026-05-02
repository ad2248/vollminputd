use crate::config::{AsrStrategy, Config};

use super::engine::AsrEngine;
use super::omni_plus::{OmniPlusAsrEngine, OmniPlusConfig};
use super::realtime::{AsrConfig, DashScopeRealtimeAsrEngine};

/// 根据配置创建 ASR 引擎
pub fn create_asr_engine(config: &Config) -> Box<dyn AsrEngine> {
    match config.asr_strategy {
        AsrStrategy::DashscopeRealtime => {
            println!("[INFO] 使用 ASR 策略：DashScope 实时识别");
            Box::new(DashScopeRealtimeAsrEngine::new(AsrConfig {
                api_key: config.dashscope_api_key.clone(),
                ..Default::default()
            }))
        }
        AsrStrategy::OmniPlus => {
            println!("[INFO] 使用 ASR 策略：OmniPlus（直接识别）");
            Box::new(OmniPlusAsrEngine::new(OmniPlusConfig {
                api_key: config.dashscope_api_key.clone(),
                ..Default::default()
            }))
        }
    }
}