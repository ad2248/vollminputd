use notify_rust::{Notification, Timeout};
use std::error::Error;

#[derive(Debug, Clone)]
pub struct NotificationRecord {
    pub title: String,
    pub body: String,
    pub timeout_secs: u32,
}

#[mockall::automock]
pub trait Notifier: Send + Sync {
    fn notify(
        &self,
        title: &str,
        body: &str,
        timeout_secs: u32,
    ) -> Result<(), Box<dyn Error + Send + Sync>>;
}

pub struct NotifyRustNotifier;

impl NotifyRustNotifier {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NotifyRustNotifier {
    fn default() -> Self {
        Self::new()
    }
}

impl Notifier for NotifyRustNotifier {
    fn notify(
        &self,
        title: &str,
        body: &str,
        timeout_secs: u32,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let timeout = if timeout_secs == 0 {
            Timeout::Never
        } else {
            Timeout::Milliseconds(timeout_secs * 1000)
        };

        if let Err(e) = Notification::new()
            .summary(title)
            .body(body)
            .timeout(timeout)
            .show()
        {
            eprintln!("[ERROR] 发送通知失败: {}", e);
        } else {
            println!("[INFO] 通知: {} - {} (超时: {}s)", title, body, timeout_secs);
        }
        Ok(())
    }
}

pub struct TestNotifier {
    sender: std::sync::mpsc::Sender<NotificationRecord>,
}

impl TestNotifier {
    pub fn new() -> (Self, std::sync::mpsc::Receiver<NotificationRecord>) {
        let (sender, receiver) = std::sync::mpsc::channel();
        (Self { sender }, receiver)
    }
}

impl Notifier for TestNotifier {
    fn notify(
        &self,
        title: &str,
        body: &str,
        timeout_secs: u32,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.sender
            .send(NotificationRecord {
                title: title.to_string(),
                body: body.to_string(),
                timeout_secs,
            })
            .map_err(|e| e.into())
    }
}