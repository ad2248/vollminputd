pub mod engine;
pub mod factory;
pub mod native;

// 为了保持向后兼容，重新导出主要类型
pub use engine::{AsrEngine, MockAsrEngine};
pub use factory::create_asr_engine;
pub use native::{NativeHttpAsrEngine, DEFAULT_ASR_ENDPOINT, DEFAULT_ASR_MODEL};
