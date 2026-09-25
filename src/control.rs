//! Local, request/response control protocol. One newline-terminated JSON request per connection.
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::env;
use std::fs;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, oneshot};
use tokio::time::{Duration, timeout};

const MAX_REQUEST_BYTES: u64 = 4096;

pub enum ControlCommand {
    Toggle,
    GetStatus,
}

pub struct ControlRequest {
    pub command: ControlCommand,
    pub reply: oneshot::Sender<std::result::Result<&'static str, String>>,
}

pub fn socket_path(instance: &str) -> Result<PathBuf> {
    let runtime = env::var_os("XDG_RUNTIME_DIR").context("XDG_RUNTIME_DIR 未设置，无法创建控制 socket")?;
    let runtime = PathBuf::from(runtime);
    anyhow::ensure!(runtime.is_dir(), "XDG_RUNTIME_DIR 不是目录: {}", runtime.display());
    Ok(runtime.join(format!("vollminputd_{instance}.sock")))
}

/// Owns the socket pathname, removing it on normal shutdown only if it is still ours.
pub struct ControlSocket {
    listener: UnixListener,
    path: PathBuf,
    device: u64,
    inode: u64,
}

impl ControlSocket {
    pub fn bind(path: PathBuf) -> Result<Self> {
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            anyhow::ensure!(metadata.file_type().is_socket(), "控制 socket 路径已被占用: {}", path.display());
            // Never take over an active daemon's socket.
            if std::os::unix::net::UnixStream::connect(&path).is_ok() {
                anyhow::bail!("控制 socket 已在使用: {}", path.display());
            }
            fs::remove_file(&path).context("无法清理失效的控制 socket")?;
        }
        let listener = UnixListener::bind(&path).context("无法创建控制 socket")?;
        let metadata = fs::metadata(&path)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        Ok(Self { listener, path, device: metadata.dev(), inode: metadata.ino() })
    }

    pub async fn serve(self, tx: mpsc::Sender<ControlRequest>) -> Result<()> {
        loop {
            let (stream, _) = self.listener.accept().await?;
            let tx = tx.clone();
            tokio::spawn(async move {
                if let Err(error) = handle_connection(stream, tx).await {
                    eprintln!("[WARN] 控制连接失败: {error}");
                }
            });
        }
    }
}

impl Drop for ControlSocket {
    fn drop(&mut self) {
        if let Ok(meta) = fs::symlink_metadata(&self.path) {
            if meta.file_type().is_socket() && meta.dev() == self.device && meta.ino() == self.inode {
                let _ = fs::remove_file(&self.path);
            }
        }
    }
}

fn parse_request(line: &str) -> std::result::Result<(u64, ControlCommand), String> {
    let request: Value = serde_json::from_str(line).map_err(|_| "无效的 JSON 请求".to_string())?;
    let id = request.get("id").and_then(Value::as_u64).ok_or("id 必须是非负整数")?;
    let command = match request.get("command").and_then(Value::as_str) {
        Some("toggle") => ControlCommand::Toggle,
        Some("get_status") => ControlCommand::GetStatus,
        _ => return Err("未知命令".to_string()),
    };
    Ok((id, command))
}

async fn handle_connection(stream: UnixStream, tx: mpsc::Sender<ControlRequest>) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut line = String::new();
    let read = timeout(Duration::from_secs(5), BufReader::new(reader).take(MAX_REQUEST_BYTES + 1).read_line(&mut line)).await;
    let response = match read {
        Ok(Ok(n)) if n > 0 && n as u64 <= MAX_REQUEST_BYTES && line.ends_with('\n') => {
            match parse_request(&line) {
                Ok((id, command)) => {
                    let (reply, rx) = oneshot::channel();
                    let result = if tx.send(ControlRequest { command, reply }).await.is_ok() {
                        rx.await.unwrap_or_else(|_| Err("守护进程无法处理请求".to_string()))
                    } else {
                        Err("守护进程已关闭".to_string())
                    };
                    match result {
                        Ok(state) => json!({"id": id, "ok": true, "state": state}),
                        Err(error) => json!({"id": id, "ok": false, "error": error}),
                    }
                }
                Err(error) => json!({"id": null, "ok": false, "error": error}),
            }
        }
        _ => json!({"id": null, "ok": false, "error": "请求超时、不完整或超过 4096 字节"}),
    };
    writer.write_all(format!("{response}\n").as_bytes()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_known_commands_with_numeric_ids() {
        assert!(matches!(parse_request("{\"id\":1,\"command\":\"toggle\"}"), Ok((1, ControlCommand::Toggle))));
        assert!(matches!(parse_request("{\"id\":2,\"command\":\"get_status\"}"), Ok((2, ControlCommand::GetStatus))));
        assert!(parse_request("{\"id\":-1,\"command\":\"toggle\"}").is_err());
        assert!(parse_request("{\"id\":1,\"command\":\"unknown\"}").is_err());
    }

    #[tokio::test]
    async fn exchanges_request_and_response() {
        let (server, client) = UnixStream::pair().unwrap();
        let (tx, mut rx) = mpsc::channel(1);
        let task = tokio::spawn(handle_connection(server, tx));
        let (reader, mut writer) = client.into_split();
        writer.write_all(b"{\"id\":7,\"command\":\"toggle\"}\n").await.unwrap();
        let request = rx.recv().await.unwrap();
        assert!(matches!(request.command, ControlCommand::Toggle));
        request.reply.send(Ok("recording")).unwrap();
        let mut response = String::new();
        BufReader::new(reader).read_line(&mut response).await.unwrap();
        assert_eq!(serde_json::from_str::<Value>(&response).unwrap(), json!({"id":7,"ok":true,"state":"recording"}));
        task.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn rejects_unknown_commands_without_dispatching() {
        let (server, client) = UnixStream::pair().unwrap();
        let (tx, mut rx) = mpsc::channel(1);
        let task = tokio::spawn(handle_connection(server, tx));
        let (reader, mut writer) = client.into_split();
        writer.write_all(b"{\"id\":7,\"command\":\"delete_everything\"}\n").await.unwrap();
        let mut response = String::new();
        BufReader::new(reader).read_line(&mut response).await.unwrap();
        assert_eq!(serde_json::from_str::<Value>(&response).unwrap(),
                   json!({"id":null,"ok":false,"error":"未知命令"}));
        task.await.unwrap().unwrap();
        assert!(rx.recv().await.is_none());
    }

    #[tokio::test]
    async fn socket_is_private_and_refuses_second_daemon() {
        let path = std::env::temp_dir().join(format!(
            "vollminputd-control-test-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let socket = ControlSocket::bind(path.clone()).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        assert!(ControlSocket::bind(path.clone()).is_err());
        drop(socket);
        assert!(!path.exists());
    }
}
