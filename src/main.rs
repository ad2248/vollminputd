use vollminputd::app::{SideEffect, VollminputdApp};
use vollminputd::asr::create_asr_engine;
use vollminputd::audio::{list_input_devices, CpalAudioCapture};
use vollminputd::clipboard::WlCopyClipboard;
use vollminputd::config::Config;
use vollminputd::notifier::{Notifier, NotifyRustNotifier};
use vollminputd::state::{AppEvent, AppState};
use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{self, Command};
use std::thread;
use tokio::sync::mpsc;

#[derive(Debug)]
enum ImeCommand {
    Toggle,
}

enum StartupAction {
    Run(String),
    ListInputDevices,
}

fn print_help(program: &str) {
    eprintln!(
        "Usage:\n  {program} --instance <NAME>\n  {program} --list-input-devices\n\nOptions:\n  --instance <NAME>       启动指定实例\n  --list-input-devices    列出可选的音频输入设备\n  -h, --help              显示此帮助信息"
    );
}

fn parse_startup_action() -> StartupAction {
    let program = env::args()
        .next()
        .unwrap_or_else(|| "vollminputd".to_string());
    let mut args = env::args().skip(1);
    let Some(first_arg) = args.next() else {
        print_help(&program);
        process::exit(1);
    };

    if first_arg == "--list-input-devices" {
        return StartupAction::ListInputDevices;
    }
    if first_arg == "-h" || first_arg == "--help" {
        print_help(&program);
        process::exit(0);
    }

    let mut args = std::iter::once(first_arg).chain(args);
    while let Some(arg) = args.next() {
        if arg == "--instance" {
            if let Some(instance) = args.next() {
                if instance.contains('/') {
                    eprintln!("[ERROR] 实例名不能包含 '/'");
                    process::exit(1);
                }
                return StartupAction::Run(instance);
            } else {
                eprintln!("[ERROR] --instance 缺少值");
                print_help(&program);
                process::exit(1);
            }
        }
    }
    print_help(&program);
    process::exit(1);
}

fn print_input_device_list() -> anyhow::Result<()> {
    let devices = list_input_devices()?;
    if devices.is_empty() {
        println!("没有发现音频输入设备");
        return Ok(());
    }

    println!("可用的音频输入设备：");
    for device in devices {
        let default_marker = if device.is_default { " [默认]" } else { "" };
        println!("- {}{}", device.name, default_marker);
        println!("  ID: {}", device.id.as_deref().unwrap_or("不可用"));
    }
    println!("\n通过 VOLLMINPUTD_AUDIO_DEVICE=<设备 ID 或完整名称> 选择麦克风。");
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let StartupAction::Run(instance) = parse_startup_action() else {
        return print_input_device_list();
    };

    println!("[INFO] 语音输入法守护进程启动");

    let fifo_path = format!("/tmp/vollminputd_{}.fifo", instance);

    let config = Config::from_env()?;
    let notifier = NotifyRustNotifier;

    setup_fifo(&fifo_path)?;
    println!("[INFO] FIFO 已创建: {}", fifo_path);

    let audio = CpalAudioCapture::new(config.audio_device.clone());
    let clipboard = WlCopyClipboard::new();
    let mut app = VollminputdApp::new(audio, clipboard);

    let (fifo_tx, mut fifo_rx) = mpsc::channel::<ImeCommand>(10);

    thread::spawn(move || {
        println!("[INFO] FIFO 监听线程已启动: {}", fifo_path);
        loop {
            if let Ok(file) = File::open(&fifo_path) {
                let reader = BufReader::new(file);
                for line in reader.lines().flatten() {
                    let cmd = line.trim();
                    if cmd == "TOGGLE" {
                        let _ = fifo_tx.blocking_send(ImeCommand::Toggle);
                    }
                }
            }
        }
    });

    let (event_tx, mut event_rx) = mpsc::channel::<AppEvent>(10);

    let max_recording_seconds = config.max_recording_seconds;
    
    // 配置已保存，ASR 引擎会在需要时通过工厂函数创建

    println!("[INFO] 程序就绪，等待快捷键触发...");

    loop {
        let mut event: Option<AppEvent> = None;

        tokio::select! {
            Some(cmd) = fifo_rx.recv() => {
                println!("[INFO] 收到 FIFO 命令: {:?}", cmd);
                match cmd {
                    ImeCommand::Toggle => {
                        event = Some(AppEvent::ToggleRecording);
                    }
                }
            }
            Some(asr_event) = event_rx.recv() => {
                println!("[INFO] 收到 ASR 事件: {:?}", asr_event);
                event = Some(asr_event);
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {}
        }

        // 检查录音超时
        if matches!(app.state, AppState::Recording) {
            let (poll_effects, timeout) = app.poll_recording(max_recording_seconds);
            for effect in &poll_effects {
                execute_effect(effect, &notifier);
            }
            if timeout {
                println!("[INFO] 录音超时，自动停止");
                event = Some(AppEvent::ToggleRecording);
            }
        }

        if let Some(incoming) = event {
            println!("[INFO] 处理事件: {:?}", incoming);
            let effects = app.handle_event(incoming).await;
            println!("[INFO] 事件处理完成，新状态: {:?}", app.state);
            
            for effect in &effects {
                execute_effect_with_asr(
                    effect,
                    &config,
                    &event_tx,
                    &notifier,
                );
            }
        }
    }
}

fn execute_effect(effect: &SideEffect, notifier: &dyn Notifier) {
    match effect {
        SideEffect::StartAudio => {
            println!("[INFO] 副作用: 启动音频采集");
        }
        SideEffect::StopAudio => {
            println!("[INFO] 副作用: 停止音频采集");
        }
        SideEffect::Notify { title, body, timeout_secs } => {
            println!("[INFO] 副作用: 发送通知 - {} ({})", title, body);
            let _ = notifier.notify(title, body, *timeout_secs);
        }
        SideEffect::CopyToClipboard(text) => {
            println!("[INFO] 副作用: 复制到剪贴板 - '{}'", text);
        }
        SideEffect::RequestAsr { .. } => {}
    }
}

fn execute_effect_with_asr(
    effect: &SideEffect,
    config: &Config,
    tx: &mpsc::Sender<AppEvent>,
    notifier: &dyn Notifier,
) {
    match effect {
        SideEffect::RequestAsr { pcm_data } => {
            println!("[INFO] 副作用: 请求 ASR 识别 ({} 字节)", pcm_data.len());
            let tx = tx.clone();
            let pcm = pcm_data.clone();
            // 每次请求都创建新的引擎实例
            let engine = create_asr_engine(config);
            tokio::spawn(async move {
                match engine.recognize(&pcm).await {
                    Ok(text) if !text.is_empty() => {
                        println!("[INFO] ASR 识别成功: '{}'", text);
                        let _ = tx.send(AppEvent::TranscriptionComplete(text)).await;
                    }
                    Ok(_) => {
                        println!("[INFO] ASR 返回空结果");
                        let _ = tx
                            .send(AppEvent::TranscriptionFailed("未检测到语音".to_string()))
                            .await;
                    }
                    Err(e) => {
                        println!("[ERROR] ASR 识别失败: {}", e);
                        let _ = tx
                            .send(AppEvent::TranscriptionFailed(format!("识别失败: {}", e)))
                            .await;
                    }
                }
            });
        }
        _ => execute_effect(effect, notifier),
    }
}

fn setup_fifo(path: &str) -> anyhow::Result<()> {
    let path_obj = Path::new(path);
    if path_obj.exists() {
        fs::remove_file(path_obj)?;
    }
    Command::new("mkfifo").arg(path).status()?;
    Command::new("chmod").arg("0666").arg(path).status()?;
    Ok(())
}
