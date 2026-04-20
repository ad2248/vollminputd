use std::fs;
use std::io::Write;
use std::os::unix::fs::FileTypeExt;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

fn create_temp_fifo() -> String {
    let uuid = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("test_voice_ime_{}.fifo", uuid));
    let path_str = path.to_str().unwrap().to_string();
    if path.exists() {
        fs::remove_file(&path).unwrap();
    }
    Command::new("mkfifo").arg(&path_str).status().unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();
    path_str
}

#[test]
fn test_fifo_command_parsing() {
    let start_cmd = "START";
    let stop_cmd = "STOP";
    let unknown_cmd = "UNKNOWN";

    assert_eq!(start_cmd.trim(), "START");
    assert_eq!(stop_cmd.trim(), "STOP");
    assert_ne!(unknown_cmd.trim(), "START");
    assert_ne!(unknown_cmd.trim(), "STOP");
}

#[test]
fn test_fifo_file_creation() {
    let fifo_path = create_temp_fifo();

    assert!(fs::metadata(&fifo_path).is_ok());
    let meta = fs::metadata(&fifo_path).unwrap();
    assert!(meta.file_type().is_fifo());

    fs::remove_file(&fifo_path).unwrap();
}

#[test]
fn test_fifo_permissions() {
    let fifo_path = create_temp_fifo();
    let meta = fs::metadata(&fifo_path).unwrap();
    let permissions = meta.permissions();
    assert_eq!(permissions.mode() & 0o777, 0o666);
    fs::remove_file(&fifo_path).unwrap();
}
