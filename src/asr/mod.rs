pub mod engine;
pub mod factory;
pub mod omni_plus;
pub mod realtime;

// 为了保持向后兼容，重新导出主要类型
pub use engine::{AsrEngine, MockAsrEngine};
pub use factory::create_asr_engine;
pub use omni_plus::{OmniPlusAsrEngine, OmniPlusConfig};
pub use realtime::{AsrConfig, DashScopeRealtimeAsrEngine, RecognitionSession};