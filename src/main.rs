mod config;
mod asr;

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command};
use std::thread;
use tokio::sync::mpsc;
use std::time::Duration;

/// 定义系统的核心控制指令
enum ImeCommand {
    Start,
    Stop,
}

/// 命名管道路径，与 Hyprland 配置保持一致
const FIFO_PATH: &str = "/tmp/amao_voice_ime.fifo";
/// 临时音频存储路径
const AUDIO_PATH: &str = "/tmp/amao_record.wav";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("喵！阿猫的 Rust 语音输入法守护进程启动啦！🐾");

    // 1. 初始化通信管道
    setup_fifo(FIFO_PATH)?;

    // 2. 创建 MPSC 异步通道，用于跨线程通信
    // 通道容量设为 10 即可，因为人类按键的手速是有物理极限的
    let (tx, mut rx) = mpsc::channel::<ImeCommand>(10);

    // 3. 启动同步的管道监听线程
    // 为什么这么设计：FIFO 是阻塞型 I/O，如果直接放在 tokio 中读取，会阻塞异步 worker 线程。
    // 因此剥离出一个原生的 OS 线程专门负责 read，读到数据就通过 channel 发送。
    thread::spawn(move || {
        loop {
            // 打开管道用于读取。如果没有程序写入，这里会阻塞挂起，不占 CPU。
            if let Ok(file) = File::open(FIFO_PATH) {
                let reader = BufReader::new(file);
                for line in reader.lines().flatten() {
                    let cmd = line.trim();
                    if cmd == "START" {
                        let _ = tx.blocking_send(ImeCommand::Start);
                    } else if cmd == "STOP" {
                        let _ = tx.blocking_send(ImeCommand::Stop);
                    }
                }
            }
        }
    });

    // 4. 异步事件循环与状态管理
    let mut record_process: Option<Child> = None;

    // 监听来自读取线程的指令
    while let Some(cmd) = rx.recv().await {
        match cmd {
            ImeCommand::Start => {
                if record_process.is_none() {
                    println!("竖起耳朵！(๑•̀ㅂ•́)و✧");
                    record_process = start_recording().ok();
                }
            }
            ImeCommand::Stop => {
                if let Some(mut child) = record_process.take() {
                    println!("听完啦，努力解析中...");
                    // 发送 SIGTERM 优雅停止录音进程
                    let _ = child.kill();
                    let _ = child.wait(); // 回收进程资源，防止变成僵尸进程

                    // 异步调用 ASR，不会阻塞主循环接收下一个指令
                    tokio::spawn(async move {
                        if let Some(text) = process_audio().await {
                            inject_text(&text);
                        }
                    });
                }
            }
        }
    }

    Ok(())
}

/// 准备命名管道（如果存在旧的则删除重建）
fn setup_fifo(path: &str) -> anyhow::Result<()> {
    let path_obj = Path::new(path);
    if path_obj.exists() {
        fs::remove_file(path_obj)?;
    }
    // 调用系统的 mkfifo 命令创建管道
    // 为了不引入重量级的 nix crate，这里直接使用 std::process::Command，足够稳健
    Command::new("mkfifo").arg(path).status()?;
    // 确保管道的权限正确，允许当前用户读写
    Command::new("chmod").arg("0666").arg(path).status()?;
    Ok(())
}

/// 启动 PipeWire 录音进程
/// 返回 Child 句柄以便后续终止
fn start_recording() -> anyhow::Result<Child> {
    let child = Command::new("pw-record")
        .args(["--rate=16000", "--channels=1", "--format=s16", AUDIO_PATH])
        .spawn()?;
    Ok(child)
}

/// 调用云端 ASR 接口处理音频文件
/// 采用异步请求，最大化利用系统 I/O 性能
async fn process_audio() -> Option<String> {
    // 读取录好的音频文件到内存
    let audio_data = fs::read(AUDIO_PATH).ok()?;
    
    // TODO: 替换为实际的 API 地址和 Key
    let api_url = "https://api.example.com/asr";
    
    let client = reqwest::Client::new();
    let part = reqwest::multipart::Part::bytes(audio_data)
        .file_name("record.wav")
        .mime_str("audio/wav").ok()?;
    let form = reqwest::multipart::Form::new().part("file", part);

    let res = client.post(api_url)
        .header("Authorization", "Bearer YOUR_API_KEY")
        .multipart(form)
        .send()
        .await.ok()?;

    if res.status().is_success() {
        // 假设 API 返回 JSON: {"text": "识别结果"}
        let json: serde_json::Value = res.json().await.ok()?;
        json.get("text").and_then(|v| v.as_str()).map(String::from)
    } else {
        println!("API 报错了呜呜呜...");
        None
    }
}

/// 将文本注入到剪贴板并通过 wtype 模拟按键上屏
fn inject_text(text: &str) {
    if text.is_empty() { return; }

    // 1. 写入 Wayland 剪贴板
    let mut wl_copy = Command::new("wl-copy")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .expect("无法启动 wl-copy");

    if let Some(mut stdin) = wl_copy.stdin.take() {
        use std::io::Write;
        let _ = stdin.write_all(text.as_bytes());
    }
    let _ = wl_copy.wait();

    // 2. 短暂休眠等待剪贴板同步（Wayland 的异步特性需要一点点时间）
    thread::sleep(Duration::from_millis(50));

    // 3. 模拟 Ctrl+V
    let _ = Command::new("wtype")
        .args(["-M", "ctrl", "-k", "v", "-m", "ctrl"])
        .status();
        
    println!("成功上屏：{} (=・ω・=)", text);
}