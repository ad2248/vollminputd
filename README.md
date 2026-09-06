# vollminputd

一个 Linux 语音输入法守护进程。通过快捷键触发录音，自动将语音转换为文字并写入剪贴板，支持在任意应用中粘贴使用。

## 功能特性

- **原生 HTTP 语音识别**：单一后端/模型 `qwen-audio-3.0-asr-flash`，走原生 HTTP 服务
- **系统级集成**：通过 FIFO 命名管道接收快捷键触发，可作为守护进程常驻后台
- **实时反馈**：录音过程中显示桌面通知，包括录音时长、设备信息
- **自动超时保护**：可配置最大录音时长，防止忘记停止录音
- **Wayland 原生支持**：使用 `wl-copy` 写入剪贴板，适配现代 Linux 桌面环境
- **桌面通知**：全程通过系统通知提示用户当前状态（开始录音、识别中、识别完成/失败）

## 系统要求

- **操作系统**：Linux（Wayland 桌面环境）
- **编译工具链**：Rust/Cargo 1.87 或更新版本；2024 edition 本身要求 1.85，当前锁定的 `zbus` 依赖进一步要求 1.87。不需要 nightly。
- **系统依赖**：
  - `wl-copy`（[wl-clipboard](https://github.com/bugaevc/wl-clipboard) 包提供）
  - 音频输入设备（麦克风）
  - 支持桌面通知的环境（D-Bus）

## 快速开始

### 1. 克隆仓库

```bash
git clone <repository-url>
cd vollminputd
```

### 2. 配置 API 密钥

vollminputd 使用环境变量进行配置。启动前必须设置 `VOLLMINPUTD_DASHSCOPE_API_KEY`：

```bash
export VOLLMINPUTD_DASHSCOPE_API_KEY="your-api-key"
```

> 获取 DashScope API 密钥：[阿里云 DashScope](https://dashscope.aliyun.com/)

### 3. 编译运行

先确认 `cargo --version` 和 `rustc --version` 均不低于 1.87。
若使用 rustup，执行 `rustup update stable`；`stable` 只是本地工具链名称，不保证它已经更新。

```bash
cargo build --locked --release
```

### Arch Linux / AUR 安装说明

`yay -S vollminputd-git` 使用 AUR 仓库单独维护的 `PKGBUILD` 和 `.SRCINFO`。
应用仓库合并代码不会自动更新 AUR 配方，即使 `-git` 包已经拉取了最新源码。

本仓库配方要求官方 `rust>=1:1.87`（`1:` 为 Arch 包 epoch）、`clang`，并声明 `alsa-lib`、`libpipewire` 等依赖；
编译与测试显式调用 `/usr/bin/cargo` 和 `/usr/bin/rustc`，避免 `~/.cargo/bin` 中的旧工具链抢先被使用。
如果 pacman 安装的 `rustup` 与官方 `rust` 包冲突，应选择一种安装方式：使用官方工具链构建发行包，或用更新后的 rustup 手动编译源码。

使用当前仓库配方本地安装（普通用户执行 `makepkg`）：

```bash
sudo pacman -Syu --needed base-devel rust clang git
# 在当前仓库根目录；makepkg -s 会安装配方声明的其他依赖
makepkg -Csi
```

维护者发布 AUR 时需同步本仓库 `PKGBUILD` 和 `.SRCINFO`；修改配方后运行
`makepkg --printsrcinfo > .SRCINFO` 重新生成元数据。AUR 更新之前，不能将仓库内的安装验证视为 `yay -S` 已修复。

### 4. 启动守护进程

确保已设置所需环境变量，然后启动守护进程：

```bash
./target/release/vollminputd --instance default
```

### 5. 触发录音

通过向 FIFO 管道发送 `TOGGLE` 命令来开始/停止录音：

```bash
# 开始录音（第一次 TOGGLE）
echo "TOGGLE" > /tmp/vollminputd_default.fifo

# 停止录音（第二次 TOGGLE）
echo "TOGGLE" > /tmp/vollminputd_default.fifo
```

识别完成后，文字会自动写入剪贴板，你可以直接粘贴使用。

## 配置说明

vollminputd 通过环境变量进行配置：

| 环境变量 | 类型 | 默认值 | 说明 |
|----------|------|--------|------|
| `VOLLMINPUTD_DASHSCOPE_API_KEY` | string | - | **必填**，DashScope API 密钥 |
| `VOLLMINPUTD_ASR_ENDPOINT` | string | `https://llm-y3exskfcgxgxzn23.cn-beijing.maas.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation` | 可选，ASR 原生 HTTP 端点；API key 会发往该端点，仅对可信端点使用 |
| `VOLLMINPUTD_ASR_MODEL` | string | `qwen-audio-3.0-asr-flash` | 可选，ASR 模型名 |
| `VOLLMINPUTD_MAX_RECORDING_SECONDS` | integer | `60` | 最大录音时长（秒），超时自动停止 |
| `VOLLMINPUTD_AUDIO_SAMPLE_RATE` | integer | `16000` | 音频采样率（Hz） |
| `VOLLMINPUTD_AUDIO_CHANNELS` | integer | `1` | 音频通道数 |

> 准确性说明：当前实际录音固定为 16 kHz、16 bit、单声道（实现内硬编码），`VOLLMINPUTD_AUDIO_SAMPLE_RATE` / `VOLLMINPUTD_AUDIO_CHANNELS` 不会改变采集参数。

### 配置示例

```bash
export VOLLMINPUTD_DASHSCOPE_API_KEY="sk-xxx...xxxx"
export VOLLMINPUTD_MAX_RECORDING_SECONDS="60"
export VOLLMINPUTD_AUDIO_SAMPLE_RATE="16000"
export VOLLMINPUTD_AUDIO_CHANNELS="1"
```

## ASR 接口

调用 `VOLLMINPUTD_ASR_ENDPOINT` 的原生 HTTP 服务，请求体为 `input.messages` + `parameters`（`formatwav`，采样率 16000），音频以 `audio/wav;base64` 内嵌；从应答的 `text` 或 `output.text` 取识别文本。

## 架构设计

```
┌─────────────────────────────────────────────────────────────┐
│                       vollminputd 守护进程                   │
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
│                            ▼                                 │
│              ┌───────────────────────────┐                    │
│              │  原生 HTTP ASR 服务        │                   │
│              │ (qwen-audio-3.0-asr-flash)│                    │
│              └───────────────────────────┘                    │
└─────────────────────────────────────────────────────────────┘
```

### 核心模块

| 模块 | 职责 |
|------|------|
| `main.rs` | 守护进程入口，FIFO 监听，事件循环，副作用执行 |
| `app.rs` | 应用核心，状态机驱动的事件处理，录音超时轮询 |
| `state.rs` | 纯函数状态转换：`Idle ⟷ Recording ⟷ Transcribing` |
| `audio.rs` | 基于 `cpal` 的音频采集，PCM 转 WAV |
| `asr/` | ASR 引擎抽象及原生 HTTP 实现（`native.rs`） |
| `clipboard.rs` | Wayland 剪贴板操作（`wl-copy`） |
| `notifier/` | 桌面通知抽象及 `notify-rust` 实现 |
| `config.rs` | 环境变量配置解析 |

## 快捷键绑定示例

将以下命令绑定到你喜欢的快捷键（如 Hyprland、Sway、i3 等）：

### Hyprland

```ini
bind = , F12, exec, echo "TOGGLE" > /tmp/vollminputd_default.fifo
```

### Sway / i3

```
bindsym F12 exec echo "TOGGLE" > /tmp/vollminputd_default.fifo
```

### 命令行脚本

```bash
#!/bin/bash
# voice-toggle.sh
INSTANCE="${VOICE_INSTANCE:-default}"
echo "TOGGLE" > "/tmp/vollminputd_${INSTANCE}.fifo"
```

## 开发

### 运行测试

单元测试（host 直接跑）：

```bash
# 运行所有测试
cargo test

# 运行特定模块测试
cargo test audio
cargo test asr
cargo test config
```

集成测试（podman 容器端到端：构建 + `cargo test --locked` + makepkg 打包在同一容器完成，每个场景独立全新容器跑 headless PipeWire/Sway 语音链路）：

```bash
# 离线套件（默认，不需要 API key；live 用例默认排除）
python3 tests/integration/run.py

# 真打云端原生 HTTP 服务；缺 key 会失败而不是跳过
python3 tests/integration/run.py --live
```

host 前置条件、API key 配置、CI 部署步骤与覆盖边界见 [tests/integration/README.md](tests/integration/README.md)。

CI（`.gitea/workflows/tests.yml`）在 push main、同仓库 PR、手动与每日定时触发：配置了 `VOLLMINPUTD_DASHSCOPE_API_KEY` secret 时自动加跑 live 用例，否则只跑离线套件并在日志中注明 live 已禁用；本次运行的产物（日志、清单、JUnit）无论成败都会上传。

### 项目结构

```
vollminputd/
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
│   │   └── native.rs       # 原生 HTTP 识别
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
