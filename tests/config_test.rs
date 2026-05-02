use VoiceInput::config::Config;

#[test]
fn test_load_full_config() {
    let yaml = r#"
dashscope_api_key: "test-key"
max_recording_seconds: 120
audio_sample_rate: 44100
audio_channels: 2
"#;
    let temp_file = std::env::temp_dir().join("test_full_config.yaml");
    std::fs::write(&temp_file, yaml).unwrap();

    let config = Config::from_yaml(temp_file.to_str().unwrap()).unwrap();
    assert_eq!(config.dashscope_api_key, "test-key");
    assert_eq!(config.max_recording_seconds, 120);
    assert_eq!(config.audio_sample_rate, 44100);
    assert_eq!(config.audio_channels, 2);

    std::fs::remove_file(temp_file).unwrap();
}

#[test]
fn test_load_minimal_config() {
    let yaml = r#"dashscope_api_key: "minimal-key""#;
    let temp_file = std::env::temp_dir().join("test_minimal_config.yaml");
    std::fs::write(&temp_file, yaml).unwrap();

    let config = Config::from_yaml(temp_file.to_str().unwrap()).unwrap();
    assert_eq!(config.dashscope_api_key, "minimal-key");
    assert_eq!(config.max_recording_seconds, 60);
    assert_eq!(config.audio_sample_rate, 16000);
    assert_eq!(config.audio_channels, 1);

    std::fs::remove_file(temp_file).unwrap();
}

#[test]
fn test_invalid_yaml_returns_error() {
    let yaml = "not valid yaml: [unclosed";
    let temp_file = std::env::temp_dir().join("test_invalid_config.yaml");
    std::fs::write(&temp_file, yaml).unwrap();

    let result = Config::from_yaml(temp_file.to_str().unwrap());
    assert!(result.is_err());

    std::fs::remove_file(temp_file).unwrap();
}

#[test]
fn test_missing_file_returns_error() {
    let result = Config::from_yaml("/nonexistent/path/config.yaml");
    assert!(result.is_err());
}

#[test]
fn test_partial_config_with_defaults() {
    let yaml = r#"
dashscope_api_key: "partial-key"
max_recording_seconds: 90
"#;
    let temp_file = std::env::temp_dir().join("test_partial_config.yaml");
    std::fs::write(&temp_file, yaml).unwrap();

    let config = Config::from_yaml(temp_file.to_str().unwrap()).unwrap();
    assert_eq!(config.dashscope_api_key, "partial-key");
    assert_eq!(config.max_recording_seconds, 90);
    assert_eq!(config.audio_sample_rate, 16000);
    assert_eq!(config.audio_channels, 1);

    std::fs::remove_file(temp_file).unwrap();
}
