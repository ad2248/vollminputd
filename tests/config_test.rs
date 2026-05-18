use VoiceInput::config::{AsrStrategy, Config};
use std::env;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn clear_env_vars() {
    for key in [
        "VOICEINPUT_DASHSCOPE_API_KEY",
        "VOICEINPUT_ASR_STRATEGY",
        "VOICEINPUT_MAX_RECORDING_SECONDS",
        "VOICEINPUT_AUDIO_SAMPLE_RATE",
        "VOICEINPUT_AUDIO_CHANNELS",
    ] {
        unsafe { env::remove_var(key); }
    }
}

#[test]
fn test_load_full_config() {
    let _guard = ENV_LOCK.lock().unwrap();
    clear_env_vars();
    unsafe { env::set_var("VOICEINPUT_DASHSCOPE_API_KEY", "test-key"); }
    unsafe { env::set_var("VOICEINPUT_MAX_RECORDING_SECONDS", "120"); }
    unsafe { env::set_var("VOICEINPUT_AUDIO_SAMPLE_RATE", "44100"); }
    unsafe { env::set_var("VOICEINPUT_AUDIO_CHANNELS", "2"); }
    unsafe { env::set_var("VOICEINPUT_ASR_STRATEGY", "omni_plus"); }

    let config = Config::from_env().unwrap();
    assert_eq!(config.dashscope_api_key, "test-key");
    assert_eq!(config.max_recording_seconds, 120);
    assert_eq!(config.audio_sample_rate, 44100);
    assert_eq!(config.audio_channels, 2);
    assert_eq!(config.asr_strategy, AsrStrategy::OmniPlus);
}

#[test]
fn test_load_minimal_config() {
    let _guard = ENV_LOCK.lock().unwrap();
    clear_env_vars();
    unsafe { env::set_var("VOICEINPUT_DASHSCOPE_API_KEY", "minimal-key"); }

    let config = Config::from_env().unwrap();
    assert_eq!(config.dashscope_api_key, "minimal-key");
    assert_eq!(config.max_recording_seconds, 60);
    assert_eq!(config.audio_sample_rate, 16000);
    assert_eq!(config.audio_channels, 1);
    assert_eq!(config.asr_strategy, AsrStrategy::DashscopeRealtime);
}

#[test]
fn test_missing_api_key_returns_error() {
    let _guard = ENV_LOCK.lock().unwrap();
    clear_env_vars();

    let result = Config::from_env();
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("VOICEINPUT_DASHSCOPE_API_KEY"));
}

#[test]
fn test_invalid_asr_strategy_returns_error() {
    let _guard = ENV_LOCK.lock().unwrap();
    clear_env_vars();
    unsafe { env::set_var("VOICEINPUT_DASHSCOPE_API_KEY", "test-key"); }
    unsafe { env::set_var("VOICEINPUT_ASR_STRATEGY", "invalid_strategy"); }

    let result = Config::from_env();
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("VOICEINPUT_ASR_STRATEGY"));
}

#[test]
fn test_partial_config_with_defaults() {
    let _guard = ENV_LOCK.lock().unwrap();
    clear_env_vars();
    unsafe { env::set_var("VOICEINPUT_DASHSCOPE_API_KEY", "partial-key"); }
    unsafe { env::set_var("VOICEINPUT_MAX_RECORDING_SECONDS", "90"); }

    let config = Config::from_env().unwrap();
    assert_eq!(config.dashscope_api_key, "partial-key");
    assert_eq!(config.max_recording_seconds, 90);
    assert_eq!(config.audio_sample_rate, 16000);
    assert_eq!(config.audio_channels, 1);
    assert_eq!(config.asr_strategy, AsrStrategy::DashscopeRealtime);
}

#[test]
fn test_dashscope_realtime_strategy_explicit() {
    let _guard = ENV_LOCK.lock().unwrap();
    clear_env_vars();
    unsafe { env::set_var("VOICEINPUT_DASHSCOPE_API_KEY", "test-key"); }
    unsafe { env::set_var("VOICEINPUT_ASR_STRATEGY", "dashscope_realtime"); }

    let config = Config::from_env().unwrap();
    assert_eq!(config.asr_strategy, AsrStrategy::DashscopeRealtime);
}