use VoiceInput::clipboard::{Clipboard, MockClipboard};

#[test]
fn test_mock_copy_text_success() {
    let mut mock = MockClipboard::new();
    mock.expect_copy_text()
        .with(mockall::predicate::eq("Hello 中文"))
        .times(1)
        .returning(|_| Ok(()));

    mock.copy_text("Hello 中文").unwrap();
}

#[test]
fn test_mock_copy_text_error() {
    let mut mock = MockClipboard::new();
    mock.expect_copy_text()
        .times(1)
        .returning(|_| Err(anyhow::anyhow!("wl-copy not found")));

    let result = mock.copy_text("test");
    assert!(result.is_err());
}

#[test]
fn test_mock_copy_empty_text() {
    let mut mock = MockClipboard::new();
    mock.expect_copy_text()
        .with(mockall::predicate::eq(""))
        .times(1)
        .returning(|_| Ok(()));

    mock.copy_text("").unwrap();
}

#[test]
fn test_mock_copy_special_chars() {
    let mut mock = MockClipboard::new();
    mock.expect_copy_text()
        .with(mockall::predicate::eq("Hello\nWorld\t!"))
        .times(1)
        .returning(|_| Ok(()));

    mock.copy_text("Hello\nWorld\t!").unwrap();
}
