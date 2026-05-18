# VoiceInput

一个 Linux 语音输入法守护进程。通过快捷键触发录音，自动将语音转换为文字并写入剪贴板，支持在任意应用中粘贴使用。

## 功能特性

- **双 ASR 策略**：支持 DashScope 实时语音识别（速度快）和 OmniPlus 模型（识别质量好，支持润色）
- **系统级集成**：通过 FIFO 命名管道接收快捷键触发，可作为守护进程常驻后台
- **实时反馈**：录音过程中显示桌面通知，包括录音时长、设备信息
- **自动超时保护**：可配置最大录音时长，防止忘记停止录音
- **Wayland 原生支持**：使用 `wl-copy` 写入剪贴板，适配现代 Linux 桌面环境
- **桌面通知**：全程通过系统通知提示用户当前状态（开始录音、识别中、识别完成/失败）

## 系统要求

- **操作系统**：Linux（Wayland 桌面环境）
- **系统依赖**：
  - `wl-copy`（[wl-clipboard](https://github.com/bugaevc/wl-clipboard) 包提供）
  - 音频输入设备（麦克风）
  - 支持桌面通知的环境（D-Bus）

## 快速开始

### 1. 克隆仓库

```bash
git clone <repository-url>
cd VoiceInput
```

### 2. 配置 API 密钥

VoiceInput 使用环境变量进行配置。启动前必须设置 `VOICEINPUT_DASHSCOPE_API_KEY`：

```bash
export VOICEINPUT_DASHSCOPE_API_KEY="your-api-key"
```

> 获取 DashScope API 密钥：[阿里云 DashScope](https://dashscope.aliyun.com/)

### 3. 编译运行

```bash
cargo build --release
```

### 4. 启动守护进程

确保已设置所需环境变量，然后启动守护进程：

```bash
./target/release/VoiceInput --instance default
```

### 5. 触发录音

通过向 FIFO 管道发送 `TOGGLE` 命令来开始/停止录音：

```bash
# 开始录音（第一次 TOGGLE）
echo "TOGGLE" > /tmp/amao_voice_ime_default.fifo

# 停止录音（第二次 TOGGLE）
echo "TOGGLE" > /tmp/amao_voice_ime_default.fifo
```

识别完成后，文字会自动写入剪贴板，你可以直接粘贴使用。

## 配置说明

VoiceInput 通过环境变量进行配置：

| 环境变量 | 类型 | 默认值 | 说明 |
|----------|------|--------|------|
| `VOICEINPUT_DASHSCOPE_API_KEY` | string | - | **必填**，DashScope API 密钥 |
| `VOICEINPUT_ASR_STRATEGY` | string | `"dashscope_realtime"` | ASR 策略：`dashscope_realtime`（实时）或 `omni_plus`（高质量） |
| `VOICEINPUT_MAX_RECORDING_SECONDS` | integer | `60` | 最大录音时长（秒），超时自动停止 |
| `VOICEINPUT_AUDIO_SAMPLE_RATE` | integer | `16000` | 音频采样率（Hz） |
| `VOICEINPUT_AUDIO_CHANNELS` | integer | `1` | 音频通道数 |

### 配置示例

```bash
export VOICEINPUT_DASHSCOPE_API_KEY="sk-xxxxxxxxxxxxxxxxxxxxxxxx"
export VOICEINPUT_ASR_STRATEGY="dashscope_realtime"
export VOICEINPUT_MAX_RECORDING_SECONDS="60"
export VOICEINPUT_AUDIO_SAMPLE_RATE="16000"
export VOICEINPUT_AUDIO_CHANNELS="1"
```

## ASR 策略对比

| 特性 | `dashscope_realtime` | `omni_plus` |
|------|----------------------|-------------|
| 速度 | 快（流式 WebSocket） | 较慢（HTTP 请求） |
| 识别质量 | 良好 | 优秀 |
| 文本润色 | 不支持 | 支持 |
| 适用场景 | 快速输入、实时性要求高 | 对识别准确度要求高 |

## 架构设计

```
┌─────────────────────────────────────────────────────────────┐
│                        VoiceInput 守护进程                    │
│  ┌─────────────┐    ┌──────────────┐    ┌────────────────┐  │
│  │  FIFO 监听器 │───▶│   状态机      │───▶│  副作用执行器   │  │
│  └─────────────┘    │ (Idle/       │    │ (音频/通知/    │  │
│                     │  Recording/  │    │  ASR/剪贴板)   │  │
│                     │  Transcribing│    └────────────────┘  │
│                     └──────────────┘                         │
│                            │                                 │
│              ┌─────────────┼─────────────┐                   │
│              ▼             ▼             ▼                   │
│  ┌─────────────────┐ ┌─────────┐ ┌──────────────────┐       │
│  │   cpal 音频采集  │ │ notify- │ │   wl-copy 剪贴板  │       │
│  │  (16kHz PCM)    │ │  rust   │ │   (Wayland)      │       │
│  └─────────────────┘ └─────────┘ └──────────────────┘       │
│                            │                                 │
│                     ┌──────┴──────┐                          │
│                     ▼             ▼                          │
│        ┌─────────────────┐ ┌─────────────────┐               │
│        │ DashScope 实时  │ │   OmniPlus      │               │
│        │   (WebSocket)   │ │   (HTTP API)    │               │
│        └─────────────────┘ └─────────────────┘               │
└─────────────────────────────────────────────────────────────┘
```

### 核心模块

| 模块 | 职责 |
|------|------|
| `main.rs` | 守护进程入口，FIFO 监听，事件循环，副作用执行 |
| `app.rs` | 应用核心，状态机驱动的事件处理，录音超时轮询 |
| `state.rs` | 纯函数状态转换：`Idle ⟷ Recording ⟷ Transcribing` |
| `audio.rs` | 基于 `cpal` 的音频采集，PCM 转 WAV |
| `asr/` | ASR 引擎抽象及两种实现（实时 / OmniPlus） |
| `clipboard.rs` | Wayland 剪贴板操作（`wl-copy`） |
| `notifier/` | 桌面通知抽象及 `notify-rust` 实现 |
| `config.rs` | 环境变量配置解析 |

## 快捷键绑定示例

将以下命令绑定到你喜欢的快捷键（如 Hyprland、Sway、i3 等）：

### Hyprland

```ini
bind = , F12, exec, echo "TOGGLE" > /tmp/amao_voice_ime_default.fifo
```

### Sway / i3

```
bindsym F12 exec echo "TOGGLE" > /tmp/amao_voice_ime_default.fifo
```

### 命令行脚本

```bash
#!/bin/bash
# voice-toggle.sh
INSTANCE="${VOICE_INSTANCE:-default}"
echo "TOGGLE" > "/tmp/amao_voice_ime_${INSTANCE}.fifo"
```

## 开发

### 运行测试

```bash
# 运行所有测试
cargo test

# 运行特定模块测试
cargo test audio
cargo test asr
cargo test config
```

### 项目结构

```
VoiceInput/
├── Cargo.toml              # 项目配置
├── src/
│   ├── main.rs             # 守护进程入口
│   ├── lib.rs              # 库入口
│   ├── app.rs              # 应用核心逻辑
│   ├── state.rs            # 状态机
│   ├── audio.rs            # 音频采集
│   ├── clipboard.rs        # 剪贴板操作
│   ├── config.rs           # 配置解析
│   ├── asr/                # ASR 引擎
│   │   ├── engine.rs       # Trait 定义
│   │   ├── factory.rs      # 工厂函数
│   │   ├── realtime.rs     # DashScope 实时识别
│   │   └── omni_plus.rs    # OmniPlus 模型
│   └── notifier/           # 通知系统
│       └── mod.rs
└── tests/                  # 集成测试
    ├── audio_test.rs
    ├── clipboard_test.rs
    ├── config_test.rs
    ├── state_test.rs
    └── ...
```

### 关键技术选型

| 依赖 | 用途 |
|------|------|
| `tokio` | 异步运行时 |
| `cpal` | 跨平台音频采集 |
| `reqwest` | HTTP/HTTPS 客户端 |
| `tokio-tungstenite` | WebSocket 客户端（实时 ASR） |
| `notify-rust` | Linux 桌面通知（D-Bus） |
| `serde` | 数据序列化/反序列化 |
| `anyhow` | 错误处理 |
| `mockall` | 测试 Mock |

## 注意事项

- 确保系统已安装 `wl-clipboard` 包（提供 `wl-copy` 命令）
- 首次运行前必须配置有效的 DashScope API 密钥
- 守护进程通过 `--instance` 参数支持多实例运行，每个实例使用独立的 FIFO 管道
- 录音文件格式为 16kHz、16bit、单声道 PCM，自动转换为 WAV 后上传

## 许可证

本项目采用 [Apache License 2.0](LICENSE.txt) 许可证。

```
Copyright 2026 i@kals.dev

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0
```
