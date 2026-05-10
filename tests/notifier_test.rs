use VoiceInput::notifier::{Notifier, NotifyRustNotifier, TestNotifier};
use mockall::predicate::eq;

#[test]
fn test_mock_notifier_called_with_expected_args() {
    let mut mock = VoiceInput::notifier::MockNotifier::new();
    mock.expect_notify()
        .with(eq("标题"), eq("内容"), eq(5))
        .times(1)
        .returning(|_, _, _| Ok(()));

    mock.notify("标题", "内容", 5).unwrap();
}

#[test]
fn test_mock_notifier_called_exactly_n_times() {
    let mut mock = VoiceInput::notifier::MockNotifier::new();
    mock.expect_notify()
        .times(2)
        .returning(|_, _, _| Ok(()));

    mock.notify("title1", "body1", 1).unwrap();
    mock.notify("title2", "body2", 2).unwrap();
}

#[test]
fn test_test_notifier_collects_notifications() {
    let (notifier, receiver) = TestNotifier::new();

    notifier.notify("Title 1", "Body 1", 1).unwrap();
    notifier.notify("Title 2", "Body 2", 2).unwrap();
    notifier.notify("Title 3", "Body 3", 3).unwrap();

    let notifications: Vec<_> = receiver.try_iter().collect();
    assert_eq!(notifications.len(), 3);
    assert_eq!(notifications[0].title, "Title 1");
    assert_eq!(notifications[1].body, "Body 2");
    assert_eq!(notifications[2].timeout_secs, 3);
}

#[test]
fn test_test_notifier_empty_when_no_notifications() {
    let (_notifier, receiver) = TestNotifier::new();

    assert!(receiver.try_recv().is_err());
}

#[test]
fn test_notify_rust_notifier_exists() {
    let _notifier: NotifyRustNotifier = NotifyRustNotifier::new();
}