use vollminputd::asr::engine::{AsrEngine, MockAsrEngine};

#[tokio::test]
async fn test_mock_recognize_returns_text() {
    let mut mock = MockAsrEngine::new();
    mock.expect_recognize()
        .times(1)
        .returning(|_| Ok("你好世界".to_string()));
    
    let result = mock.recognize(&[0u8; 100]).await.unwrap();
    assert_eq!(result, "你好世界");
}

#[tokio::test]
async fn test_mock_recognize_timeout_error() {
    let mut mock = MockAsrEngine::new();
    mock.expect_recognize()
        .times(1)
        .returning(|_| Err(anyhow::anyhow!("ASR timeout")));
    
    let result = mock.recognize(&[0u8; 100]).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_mock_recognize_empty_result() {
    let mut mock = MockAsrEngine::new();
    mock.expect_recognize()
        .times(1)
        .returning(|_| Ok("".to_string()));
    
    let result = mock.recognize(&[0u8; 100]).await.unwrap();
    assert!(result.is_empty());
}

#[tokio::test]
async fn test_asr_config_defaults() {
    let config = vollminputd::asr::realtime::AsrConfig::default();
    assert_eq!(config.model, "qwen3-asr-flash-realtime");
    assert_eq!(config.base_url, "wss://dashscope.aliyuncs.com/api-ws/v1/realtime");
    assert!(config.api_key.is_empty());
}
