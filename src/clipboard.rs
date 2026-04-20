use anyhow::Result;
use std::io::Write;
use std::process::{Command, Stdio};

#[mockall::automock]
pub trait Clipboard: Send + Sync {
    fn copy_text(&self, text: &str) -> Result<()>;
}

pub struct WlCopyClipboard;

impl WlCopyClipboard {
    pub fn new() -> Self {
        Self
    }
}

impl Clipboard for WlCopyClipboard {
    fn copy_text(&self, text: &str) -> Result<()> {
        let mut child = Command::new("wl-copy")
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow::anyhow!("Failed to spawn wl-copy: {}", e))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(text.as_bytes())
                .map_err(|e| anyhow::anyhow!("Failed to write to wl-copy: {}", e))?;
        }

        let status = child
            .wait()
            .map_err(|e| anyhow::anyhow!("Failed to wait for wl-copy: {}", e))?;

        if !status.success() {
            return Err(anyhow::anyhow!("wl-copy exited with status: {:?}", status));
        }

        Ok(())
    }
}
