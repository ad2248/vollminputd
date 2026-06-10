use vollminputd::audio::{AudioCapture, MockAudioCapture};

#[tokio::test]
async fn test_mock_start_stop_lifecycle() {
    let mut mock = MockAudioCapture::new();
    mock.expect_start_capture()
        .times(1)
        .returning(|| Ok(()));
    mock.expect_stop_capture()
        .times(1)
        .returning(|| Ok(vec![1u8, 2u8, 3u8]));
    
    mock.start_capture().await.unwrap();
    let data = mock.stop_capture().await.unwrap();
    assert_eq!(data, vec![1u8, 2u8, 3u8]);
}

#[tokio::test]
async fn test_mock_is_capturing_state() {
    let mut mock = MockAudioCapture::new();
    mock.expect_is_capturing()
        .times(1)
        .returning(|| false);
    
    assert!(!mock.is_capturing());
}

#[tokio::test]
async fn test_mock_elapsed_seconds() {
    let mut mock = MockAudioCapture::new();
    mock.expect_elapsed_seconds()
        .times(1)
        .returning(|| 42);
    
    assert_eq!(mock.elapsed_seconds(), 42);
}

#[tokio::test]
async fn test_mock_stop_returns_empty_data() {
    let mut mock = MockAudioCapture::new();
    mock.expect_stop_capture()
        .times(1)
        .returning(|| Ok(Vec::new()));
    
    let data = mock.stop_capture().await.unwrap();
    assert!(data.is_empty());
}

#[tokio::test]
async fn test_mock_start_capture_error() {
    let mut mock = MockAudioCapture::new();
    mock.expect_start_capture()
        .times(1)
        .returning(|| Err(anyhow::anyhow!("device unavailable")));
    
    let result = mock.start_capture().await;
    assert!(result.is_err());
}
