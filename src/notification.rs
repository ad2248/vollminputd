use notify_rust::Notification;
use std::time::Duration;

pub fn notify(title: &str, body: &str, timeout_secs: u32) {
    let timeout = if timeout_secs == 0 {
        notify_rust::Timeout::Never
    } else {
        notify_rust::Timeout::Milliseconds(timeout_secs * 1000)
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
}