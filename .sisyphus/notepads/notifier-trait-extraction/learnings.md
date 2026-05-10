# Notifier Trait Extraction - Learnings

## Test Patterns Discovered

### MockNotifier (mockall)
- Created via `VoiceInput::notifier::MockNotifier::new()`
- `expect_notify()` with `.with(eq(...), eq(...), eq(...))` for arg matching
- `.times(n)` to assert call count
- `.returning(|_, _, _| Ok(()))` for closure return

### TestNotifier (channel-based)
- Created via `TestNotifier::new()` returning `(Self, Receiver<NotificationRecord>)`
- `receiver.try_iter().collect()` collects all notifications
- `receiver.try_recv().is_err()` checks if empty

### NotificationRecord fields
- `title: String`
- `body: String`
- `timeout_secs: u32`

## File created
- `tests/notifier_test.rs` - 5 tests passing (2 mock, 2 channel, 1 existence)