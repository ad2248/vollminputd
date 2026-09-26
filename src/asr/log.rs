//! 按请求保存实际上传的 WAV 与服务端返回的转录文字。
use anyhow::{Context, Result};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use time::OffsetDateTime;

#[derive(Clone)]
pub struct RequestLogger {
    root: PathBuf,
    max_entries: usize,
}

pub struct RequestEntry {
    path: PathBuf,
    logger: RequestLogger,
    completed: bool,
}

impl RequestLogger {
    pub fn new(root: impl Into<PathBuf>, max_entries: usize) -> Self {
        Self { root: root.into(), max_entries }
    }

    pub fn start(&self, wav: &[u8]) -> Result<RequestEntry> {
        if self.max_entries == 0 {
            anyhow::bail!("最大日志条目为 0，日志已禁用");
        }
        fs::DirBuilder::new().recursive(true).mode(0o700).create(&self.root)
            .with_context(|| format!("创建日志目录 {} 失败", self.root.display()))?;
        let metadata = fs::symlink_metadata(&self.root)?;
        if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 {
            anyhow::bail!("日志目录 {} 必须是仅当前用户可访问的目录 (0700)", self.root.display());
        }

        // 纳秒级 UTC ISO 8601 名称；极少数同一时刻发生的请求重新取时钟避免覆盖。
        for _ in 0..100 {
            let now = OffsetDateTime::now_utc();
            let name = format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:09}Z",
                now.year(), now.month() as u8, now.day(),
                now.hour(), now.minute(), now.second(), now.nanosecond()
            );
            let path = self.root.join(name);
            match fs::DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => {
                    let entry = RequestEntry { path, logger: self.clone(), completed: false };
                    write_private(&entry.path.join("request.wav"), wav)?;
                    return Ok(entry);
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        anyhow::bail!("无法生成唯一的 ASR 日志目录")
    }

    fn prune(&self) -> Result<()> {
        // 仅清理包含完整文件对的条目，不碰其他目录或正在识别的请求。
        let mut entries = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if is_timestamp_name(&name)
                && entry.file_type()?.is_dir()
                && path.join("request.wav").is_file()
                && path.join("response.txt").is_file()
            {
                entries.push(path);
            }
        }
        entries.sort();
        let excess = entries.len().saturating_sub(self.max_entries);
        for path in entries.into_iter().take(excess) {
            fs::remove_dir_all(&path).with_context(|| format!("清理旧日志 {} 失败", path.display()))?;
        }
        Ok(())
    }
}

fn is_timestamp_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() == 30
        && [4, 7].iter().all(|&i| bytes[i] == b'-')
        && bytes[10] == b'T'
        && [13, 16].iter().all(|&i| bytes[i] == b':')
        && bytes[19] == b'.'
        && bytes[29] == b'Z'
        && bytes.iter().enumerate().all(|(i, byte)|
            matches!(i, 4 | 7 | 10 | 13 | 16 | 19 | 29) || byte.is_ascii_digit())
}

impl RequestEntry {
    pub fn finish(mut self, text: &str) -> Result<()> {
        write_private(&self.path.join("response.txt"), text.as_bytes())?;
        self.completed = true;
        self.logger.prune()
    }
}

impl Drop for RequestEntry {
    fn drop(&mut self) {
        if !self.completed {
            // API 失败或文字无法保存时，不留只有音频的半成品。
            if let Err(error) = fs::remove_dir_all(&self.path) {
                eprintln!("[WARN] 清理不完整 ASR 日志 {} 失败: {error}", self.path.display());
            }
        }
    }
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)?;
    file.write_all(bytes)?;
    file.flush()?;
    Ok(())
}
