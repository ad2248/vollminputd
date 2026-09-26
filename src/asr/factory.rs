use crate::config::Config;

use super::engine::AsrEngine;
use super::log::RequestLogger;
use super::native::NativeHttpAsrEngine;

/// 根据配置创建 ASR 引擎
pub fn create_asr_engine(config: &Config) -> Box<dyn AsrEngine> {
    println!("[INFO] 使用 ASR 策略：原生 HTTP 识别");
    let engine = NativeHttpAsrEngine::new(
        config.dashscope_api_key.clone(),
        config.asr_endpoint.clone(),
        config.asr_model.clone(),
    );
    if config.max_log_entries == 0 {
        Box::new(engine)
    } else {
        Box::new(engine.with_logger(RequestLogger::new("/var/log/vollminputd", config.max_log_entries)))
    }
}
