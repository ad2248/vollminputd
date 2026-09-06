use vollminputd::asr::{DEFAULT_ASR_ENDPOINT, DEFAULT_ASR_MODEL};
use vollminputd::config::Config;
use std::env;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn clear_env_vars() {
    for key in [
        "VOLLMINPUTD_DASHSCOPE_API_KEY",
        "VOLLMINPUTD_ASR_ENDPOINT",
        "VOLLMINPUTD_ASR_MODEL",
        "VOLLMINPUTD_MAX_RECORDING_SECONDS",
        "VOLLMINPUTD_AUDIO_SAMPLE_RATE",
        "VOLLMINPUTD_AUDIO_CHANNELS",
    ] {
        unsafe { env::remove_var(key); }
    }
}

#[test]
fn test_load_full_config() {
    let _guard = ENV_LOCK.lock().unwrap();
    clear_env_vars();
    unsafe { env::set_var("VOLLMINPUTD_DASHSCOPE_API_KEY", "test-key"); }
    unsafe { env::set_var("VOLLMINPUTD_MAX_RECORDING_SECONDS", "120"); }
    unsafe { env::set_var("VOLLMINPUTD_AUDIO_SAMPLE_RATE", "44100"); }
    unsafe { env::set_var("VOLLMINPUTD_AUDIO_CHANNELS", "2"); }
    unsafe { env::set_var("VOLLMINPUTD_ASR_ENDPOINT", "http://127.0.0.1:18903/generation"); }
    unsafe { env::set_var("VOLLMINPUTD_ASR_MODEL", "custom-asr-model"); }

    let config = Config::from_env().unwrap();
    assert_eq!(config.dashscope_api_key, "test-key");
    assert_eq!(config.max_recording_seconds, 120);
    assert_eq!(config.audio_sample_rate, 44100);
    assert_eq!(config.audio_channels, 2);
    assert_eq!(config.asr_endpoint, "http://127.0.0.1:18903/generation");
    assert_eq!(config.asr_model, "custom-asr-model");
}

#[test]
fn test_load_minimal_config() {
    let _guard = ENV_LOCK.lock().unwrap();
    clear_env_vars();
    unsafe { env::set_var("VOLLMINPUTD_DASHSCOPE_API_KEY", "minimal-key"); }

    let config = Config::from_env().unwrap();
    assert_eq!(config.dashscope_api_key, "minimal-key");
    assert_eq!(config.max_recording_seconds, 60);
    assert_eq!(config.audio_sample_rate, 16000);
    assert_eq!(config.audio_channels, 1);
    assert_eq!(config.asr_endpoint, DEFAULT_ASR_ENDPOINT);
    assert_eq!(config.asr_model, DEFAULT_ASR_MODEL);
}

#[test]
fn test_missing_api_key_returns_error() {
    let _guard = ENV_LOCK.lock().unwrap();
    clear_env_vars();

    let result = Config::from_env();
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("VOLLMINPUTD_DASHSCOPE_API_KEY"));
}

#[test]
fn test_partial_config_with_defaults() {
    let _guard = ENV_LOCK.lock().unwrap();
    clear_env_vars();
    unsafe { env::set_var("VOLLMINPUTD_DASHSCOPE_API_KEY", "partial-key"); }
    unsafe { env::set_var("VOLLMINPUTD_MAX_RECORDING_SECONDS", "90"); }

    let config = Config::from_env().unwrap();
    assert_eq!(config.dashscope_api_key, "partial-key");
    assert_eq!(config.max_recording_seconds, 90);
    assert_eq!(config.audio_sample_rate, 16000);
    assert_eq!(config.audio_channels, 1);
    assert_eq!(config.asr_endpoint, DEFAULT_ASR_ENDPOINT);
    assert_eq!(config.asr_model, DEFAULT_ASR_MODEL);
}

#[test]
fn test_asr_endpoint_blank_falls_back_to_default() {
    let _guard = ENV_LOCK.lock().unwrap();
    clear_env_vars();
    unsafe { env::set_var("VOLLMINPUTD_DASHSCOPE_API_KEY", "test-key"); }
    unsafe { env::set_var("VOLLMINPUTD_ASR_ENDPOINT", "   "); }

    let config = Config::from_env().unwrap();
    assert_eq!(config.asr_endpoint, DEFAULT_ASR_ENDPOINT);
}

#[test]
fn test_asr_model_blank_falls_back_to_default() {
    let _guard = ENV_LOCK.lock().unwrap();
    clear_env_vars();
    unsafe { env::set_var("VOLLMINPUTD_DASHSCOPE_API_KEY", "test-key"); }
    unsafe { env::set_var("VOLLMINPUTD_ASR_MODEL", "  "); }

    let config = Config::from_env().unwrap();
    assert_eq!(config.asr_model, DEFAULT_ASR_MODEL);
}

#[test]
fn test_default_constants_match_verified_service() {
    assert_eq!(DEFAULT_ASR_ENDPOINT, "https://llm-y3exskfcgxgxzn23.cn-beijing.maas.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation");
    assert_eq!(DEFAULT_ASR_MODEL, "qwen-audio-3.0-asr-flash");
}
