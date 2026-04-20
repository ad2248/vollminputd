# VoiceInput GUI 改造 — TDD 驱动

## TL;DR

> **Quick Summary**: 将现有的无头 FIFO 语音输入工具改造为带 Slint GUI 的语音输入法。用 cpal 内建音频采集替代 pw-record，添加浮动面板 GUI 显示状态，保留 FIFO 外部控制接口。全程 TDD 开发。
> 
> **Deliverables**:
> - Slint 浮动面板 GUI（聆听/解析中/解析结果/错误 四种状态）
> - cpal 内建音频采集模块
> - 状态机驱动的工作流（Idle → Recording → Transcribing → Result → Accepted）
> - 完整的 TDD 测试套件（mockall + tokio-test）
> - 仅 wl-copy 剪贴板写入（移除 wtype）
> - 保留 FIFO 外部控制接口
> 
> **Estimated Effort**: Medium
> **Parallel Execution**: YES - 4 waves
> **Critical Path**: Task 1 → Task 3 → Task 7 → Task 8 → Task 9 → Task 10

---

## Context

### Original Request
按下 Win+Space 弹出小巧 GUI，显示状态（聆听/解析中/解析结果）。F=说话完成, C/ESC=取消, R=重试(完全重新录音), Enter=接受。接受后仅复制到剪贴板（不按 Ctrl+V）。

### Interview Summary
**Key Discussions**:
- 桌面环境: Wayland (Hyprland)
- GUI 框架: Slint（浮动面板，无边框，always-on-top，半透明）
- 音频采集: cpal 内建采集（PCM 16bit 16kHz 单声道）
- 热键: Hyprland 绑定 Win+Space → FIFO START，GUI 内 F/C/R/Enter 通过 Slint 键盘事件
- 剪贴板: 仅 wl-copy，移除 wtype 模拟
- 重试(R键): 完全重新录音+识别
- FIFO: 保留用于测试和外部控制

**Research Findings**:
- 现有 asr.rs 已实现完整的 DashScope WebSocket 流式 ASR 客户端
- 现有 main.rs 使用 FIFO + pw-record 外部进程 + wl-copy/wtype
- 测试仅有一个 ASR 集成测试（需要真实 API key）
- 无独立测试目录，无 mock 基础设施

### Metis Review
**Identified Gaps** (addressed):
- 音频采集时机：cpal 在 Recording 状态时持续采集，F 键停止采集并发送缓冲区到 ASR
- GUI 键盘事件：Slint 窗口需要 focus 才能接收键盘事件，需要确保窗口弹出时自动获得焦点
- 多实例保护：防止重复 START 导致多个录音会话
- 优雅退出：Ctrl+C / SIGTERM 时的资源清理（关闭 WebSocket、停止 cpal、删除临时文件）
- 长时间录音：应设置最大录音时长限制（如 60 秒自动停止）
- 空识别结果处理：ASR 返回空文本时 GUI 应显示"未检测到语音"

---

## Work Objectives

### Core Objective
将无头 FIFO 语音输入工具改造为带 Slint GUI 的语音输入法，使用 TDD 方式开发。

### Concrete Deliverables
- `src/audio.rs` — cpal 音频采集模块（trait-based，可 mock）
- `src/clipboard.rs` — wl-copy 剪贴板写入模块（trait-based，可 mock）
- `src/state.rs` — 状态机（AppEvent 驱动状态转换）
- `src/gui.slint` — Slint UI 定义（浮动面板）
- `src/gui.rs` — Slint 后端集成
- `src/asr.rs` — 重构为 trait-based（可 mock）
- `src/config.rs` — 扩展配置（音频设备、录音时长限制等）
- `src/main.rs` — 重写为 GUI 事件循环 + 异步 ASR 管道
- 完整测试套件覆盖所有模块

### Definition of Done
- [ ] `cargo build` 成功编译
- [ ] `cargo test` 所有单元测试通过（不需要网络/真实音频设备）
- [ ] 运行后按 Win+Space 弹出 GUI 浮动面板
- [ ] GUI 正确显示：聆听中 → 解析中 → 识别结果
- [ ] Enter 接受后文字在剪贴板中
- [ ] FIFO 发送 START/STOP 仍可正常工作

### Must Have
- Slint 浮动面板 GUI（无边框、always-on-top、半透明）
- 四种状态显示：聆听中、解析中、识别结果、错误
- 快捷键 F/C/ESC/R/Enter 在 GUI 内工作
- cpal 内建音频采集
- 仅 wl-copy 写入剪贴板（不模拟 Ctrl+V）
- 保留 FIFO 外部控制接口
- 全模块 TDD 测试（mockall mock）
- 最大录音时长限制（60 秒）

### Must NOT Have (Guardrails)
- 不使用 wtype 模拟 Ctrl+V 粘贴
- 不内建全局热键注册（依赖 Hyprland 绑定）
- 不引入 X11 依赖
- 不在测试中依赖真实音频设备或网络
- 不使用 unsafe 代码
- 不添加不需要的过度抽象（如插件系统、多 ASR 后端等）
- 不添加配置文件热重载
- 不添加日志文件（只用 stdout/println）
- 不添加 i18n/多语言支持
- 不修改现有 asr.rs 中 DashScope 协议实现的核心逻辑（仅包装为 trait）

---

## Verification Strategy (MANDATORY)

> **ZERO HUMAN INTERVENTION** - ALL verification is agent-executed. No exceptions.

### Test Decision
- **Infrastructure exists**: PARTIAL（仅有一个 ASR 集成测试）
- **Automated tests**: YES (TDD)
- **Framework**: Rust #[test] + #[tokio::test] + mockall
- **TDD Flow**: Each task follows RED (failing test) → GREEN (minimal impl) → REFACTOR

### QA Policy
Every task MUST include agent-executed QA scenarios.
Evidence saved to `.sisyphus/evidence/task-{N}-{scenario-slug}.{ext}`.

- **CLI/Build**: Use Bash — cargo build, cargo test, run binary
- **TUI/CLI**: Use interactive_bash (tmux) — Run binary, send FIFO commands, validate output
- **Module/Logic**: Use Bash (cargo test) — Run tests, assert pass

---

## Execution Strategy

### Parallel Execution Waves

```
Wave 1 (Start Immediately — foundation + types + Slint UI definition):
├── Task 1: 项目配置 + 依赖添加 [quick]
├── Task 2: 状态机类型定义 + TDD [deep]
├── Task 3: Slint UI 定义文件 (gui.slint) [visual-engineering]
├── Task 4: 配置模块扩展 + TDD [quick]

Wave 2 (After Wave 1 — core modules, MAX PARALLEL):
├── Task 5: 音频采集模块 (audio.rs) + TDD (depends: 1, 2) [deep]
├── Task 6: 剪贴板模块 (clipboard.rs) + TDD (depends: 1) [quick]
├── Task 7: ASR trait 抽象 + 重构 + TDD (depends: 1, 2) [deep]
├── Task 8: Slint 后端集成 (gui.rs) (depends: 2, 3) [visual-engineering]

Wave 3 (After Wave 2 — orchestration + integration):
├── Task 9: 主事件循环重写 (main.rs) (depends: 5, 6, 7, 8) [deep]
├── Task 10: FIFO 管道集成 (depends: 9) [quick]
├── Task 11: 错误处理 + 边界场景 (depends: 9) [unspecified-high]

Wave FINAL (After ALL tasks — 4 parallel reviews):
├── Task F1: Plan compliance audit (oracle)
├── Task F2: Code quality review (unspecified-high)
├── Task F3: Real manual QA (unspecified-high)
├── Task F4: Scope fidelity check (deep)
→ Present results → Get explicit user okay

Critical Path: Task 1 → Task 5 → Task 9 → Task 10 → F1-F4
Parallel Speedup: ~60% faster than sequential
Max Concurrent: 4 (Wave 2)
```

### Dependency Matrix

| Task | Depends On | Blocks | Wave |
|------|-----------|--------|------|
| 1    | -         | 5,6,7  | 1    |
| 2    | -         | 5,7,8  | 1    |
| 3    | -         | 8      | 1    |
| 4    | -         | -      | 1    |
| 5    | 1, 2      | 9      | 2    |
| 6    | 1         | 9      | 2    |
| 7    | 1, 2      | 9      | 2    |
| 8    | 2, 3      | 9      | 2    |
| 9    | 5,6,7,8   | 10,11  | 3    |
| 10   | 9         | F      | 3    |
| 11   | 9         | F      | 3    |

### Agent Dispatch Summary

- **Wave 1**: **4** — T1 → `quick`, T2 → `deep`, T3 → `visual-engineering`, T4 → `quick`
- **Wave 2**: **4** — T5 → `deep`, T6 → `quick`, T7 → `deep`, T8 → `visual-engineering`
- **Wave 3**: **3** — T9 → `deep`, T10 → `quick`, T11 → `unspecified-high`
- **FINAL**: **4** — F1 → `oracle`, F2 → `unspecified-high`, F3 → `unspecified-high`, F4 → `deep`

---

## TODOs

- [x] 1. 项目配置 + 依赖添加

  **What to do**:
  - 在 Cargo.toml 中添加新依赖：
    - `slint` (GUI 框架，启用 wayland feature)
    - `cpal` (跨平台音频采集)
    - `mockall` (dev-dependency，用于 mock)
    - `tokio` 已有，确认 features 包含 `sync` 和 `time`
  - 创建 `src/state.rs` 空文件（占位，Task 2 填充）
  - 创建 `src/audio.rs` 空文件（占位，Task 5 填充）
  - 创建 `src/clipboard.rs` 空文件（占位，Task 6 填充）
  - 创建 `src/gui.rs` 空文件（占位，Task 8 填充）
  - 更新 `src/lib.rs` 添加新模块声明
  - 确保 `cargo build` 通过（空模块）
  - 编写第一个冒烟测试：`tests/smoke_test.rs` 验证项目编译

  **Must NOT do**:
  - 不修改现有 asr.rs 逻辑
  - 不添加不需要的 feature flags

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: 纯配置修改，无复杂逻辑
  - **Skills**: []
  - **Skills Evaluated but Omitted**:
    - 无特殊 skill 需求

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with Tasks 2, 3, 4)
  - **Blocks**: Tasks 5, 6, 7
  - **Blocked By**: None

  **References**:

  **Pattern References**:
  - `Cargo.toml:1-17` — 现有依赖配置，需要在此基础添加新依赖
  - `src/lib.rs:1-2` — 现有模块声明模式，新模块需遵循

  **External References**:
  - Slint docs: https://slint.dev/docs/tutorial/rust — Rust 集成方式和 build 配置
  - cpal crate: https://docs.rs/cpal — API 概览
  - mockall crate: https://docs.rs/mockall — mock 宏用法

  **Acceptance Criteria**:

  **If TDD:**
  - [ ] `tests/smoke_test.rs` 存在，包含一个简单测试 `fn test_project_compiles()`
  - [ ] `cargo test` → PASS

  **QA Scenarios (MANDATORY):**

  ```
  Scenario: 项目编译成功
    Tool: Bash
    Preconditions: 项目目录干净
    Steps:
      1. cargo build 2>&1
      2. 检查退出码 == 0
    Expected Result: 编译成功，无 error
    Failure Indicators: 退出码非 0，或输出包含 "error"
    Evidence: .sisyphus/evidence/task-1-build-success.log

  Scenario: 冒烟测试通过
    Tool: Bash
    Preconditions: cargo build 成功
    Steps:
      1. cargo test --test smoke_test 2>&1
      2. 检查退出码 == 0
    Expected Result: 1 test passed, 0 failures
    Failure Indicators: "test result: FAILED" 或退出码非 0
    Evidence: .sisyphus/evidence/task-1-smoke-test.log
  ```

  **Commit**: YES (group with Wave 1)
  - Message: `feat(voice-input): add project config, dependencies and module stubs`
  - Files: `Cargo.toml, src/lib.rs, src/state.rs, src/audio.rs, src/clipboard.rs, src/gui.rs, tests/smoke_test.rs`
  - Pre-commit: `cargo build && cargo test`

- [x] 2. 状态机类型定义 + TDD

  **What to do**:
  - 在 `src/state.rs` 中定义：
    - `enum AppState` — `Idle`, `Recording`, `Transcribing`, `Result(String)`, `Error(String)`
    - `enum AppEvent` — `StartRecording`, `FinishRecording`, `TranscriptionComplete(String)`, `TranscriptionFailed(String)`, `Accept`, `Cancel`, `Retry`
    - `fn transition(state: AppState, event: AppEvent) -> Result<AppState, String>` — 纯函数状态转换
  - **TDD RED**: 先写测试 `tests/state_test.rs`
    - 测试所有正常转换路径：Idle→Recording→Transcribing→Result→Idle(接受)
    - 测试取消路径：Recording→Idle, Transcribing→Idle, Result→Idle
    - 测试重试路径：Result→Recording
    - 测试错误路径：Transcribing→Error, Error→Recording(重试)
    - 测试非法转换：Idle 收到 Accept 应返回 Err
  - **TDD GREEN**: 实现状态机使所有测试通过
  - **REFACTOR**: 简化代码，确保可读性

  **Must NOT do**:
  - 不引入外部状态机库（如 finitelater, sm）
  - 不在状态机中包含 I/O 操作
  - 不使用 unsafe

  **Recommended Agent Profile**:
  - **Category**: `deep`
    - Reason: 需要仔细设计状态转换逻辑，确保覆盖所有路径
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with Tasks 1, 3, 4)
  - **Blocks**: Tasks 5, 7, 8
  - **Blocked By**: None

  **References**:

  **Pattern References**:
  - `src/asr.rs:96-106` — 现有类型定义风格（struct + impl）

  **Acceptance Criteria**:

  **If TDD:**
  - [ ] `tests/state_test.rs` 存在，包含 ≥10 个测试用例
  - [ ] `cargo test --test state_test` → PASS (all tests, 0 failures)

  **QA Scenarios (MANDATORY):**

  ```
  Scenario: 状态机完整路径测试
    Tool: Bash
    Preconditions: cargo build 成功
    Steps:
      1. cargo test --test state_test 2>&1
      2. 检查 "test result: ok" 且 0 failures
    Expected Result: 所有状态转换测试通过
    Failure Indicators: 任何 test FAILED 或 panic
    Evidence: .sisyphus/evidence/task-2-state-tests.log

  Scenario: 非法转换返回错误
    Tool: Bash
    Preconditions: 状态机编译成功
    Steps:
      1. cargo test --test state_test -- illegal 2>&1
      2. 确认非法转换测试通过（返回 Err 而非 panic）
    Expected Result: 非法转换返回 Err(String)，不 panic
    Failure Indicators: 测试 panic 或返回 Ok
    Evidence: .sisyphus/evidence/task-2-illegal-transitions.log
  ```

  **Commit**: YES (group with Wave 1)
  - Message: `feat(voice-input): add state machine with TDD tests`
  - Files: `src/state.rs, tests/state_test.rs`
  - Pre-commit: `cargo test --test state_test`

- [x] 3. Slint UI 定义文件 (gui.slint)

  **What to do**:
  - 创建 `ui/gui.slint` 文件（Slint 标准放在 ui/ 目录）
  - 定义 GUI 组件 `VoiceInputApp`：
    - 窗口属性：`width: 400px, height: 180px, no-frame`, 背景半透明
    - 标题行：显示 "🎙 语音输入" + 当前状态标签
    - 主区域：根据状态显示不同内容
      - Idle: "按下快捷键开始录音" 提示
      - Recording: "正在聆听..." + 动画指示器（如脉冲圆点）+ 已录音时长
      - Transcribing: "解析中..." + 加载动画
      - Result: 识别文本显示 + 快捷键提示 (Enter=接受 C=取消 R=重试)
      - Error: 错误信息 + "按 R 重试"
    - 底部快捷键提示栏
  - 使用 Slint 的 `callback` 定义事件：`start-recording()`, `finish-recording()`, `cancel()`, `retry()`, `accept()`
  - 使用 `in-out property <string> state` 传递当前状态到 UI
  - 使用 `in-out property <string> result-text` 传递识别结果
  - 使用 `in-out property <string> error-message` 传递错误信息
  - 使用 `in-out property <int> recording-duration` 显示录音时长
  - 添加键盘事件处理：F→finish-recording, C/Escape→cancel, R→retry, Enter→accept
  - 确保窗口 `always-on-top`（Slint Wayland 后端支持）
  - 编译验证：`cargo build` 通过 Slint 编译

  **Must NOT do**:
  - 不在 Slint 文件中包含业务逻辑
  - 不使用 bitmap 图片资源（纯 Slint markup）
  - 不添加过度复杂的动画效果

  **Recommended Agent Profile**:
  - **Category**: `visual-engineering`
    - Reason: GUI 设计和样式实现
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with Tasks 1, 2, 4)
  - **Blocks**: Task 8
  - **Blocked By**: None

  **References**:

  **External References**:
  - Slint syntax reference: https://slint.dev/docs/reference/
  - Slint Rust integration: https://slint.dev/docs/tutorial/rust — build.rs 配置方式
  - Slint Window properties: https://slint.dev/docs/reference/global/Window — flags, no-frame, always-on-top

  **Acceptance Criteria**:

  **QA Scenarios (MANDATORY):**

  ```
  Scenario: Slint 文件编译通过
    Tool: Bash
    Preconditions: Cargo.toml 已添加 slint 依赖
    Steps:
      1. cargo build 2>&1
      2. 检查退出码 == 0
    Expected Result: Slint 编译成功，生成 Rust 绑定代码
    Failure Indicators: "error" in Slint compilation output
    Evidence: .sisyphus/evidence/task-3-slint-compile.log

  Scenario: Slint UI 组件包含所需属性
    Tool: Bash
    Preconditions: 编译成功
    Steps:
      1. grep -c "callback" ui/gui.slint — 应 ≥ 5 个 callback
      2. grep -c "in-out property" ui/gui.slint — 应 ≥ 3 个属性
      3. grep "no-frame" ui/gui.slint — 应存在
      4. grep "always-on-top" ui/gui.slint — 应存在
    Expected Result: UI 定义包含所有必要的 callback 和 property
    Failure Indicators: 缺少关键 callback 或 property
    Evidence: .sisyphus/evidence/task-3-slint-structure.log
  ```

  **Commit**: YES (group with Wave 1)
  - Message: `feat(voice-input): add Slint UI definition for voice input panel`
  - Files: `ui/gui.slint`
  - Pre-commit: `cargo build`

- [x] 4. 配置模块扩展 + TDD

  **What to do**:
  - 扩展 `src/config.rs` 的 `Config` 结构体：
    - 添加 `max_recording_seconds: u64` (默认 60)
    - 添加 `audio_sample_rate: u32` (默认 16000)
    - 添加 `audio_channels: u16` (默认 1)
    - 为新字段实现 Default 值（当 YAML 中省略时使用默认值）
    - 使用 `#[serde(default)]` 属性
  - **TDD RED**: 在 `tests/config_test.rs` 中编写：
    - 测试从完整 YAML 加载所有字段
    - 测试从最小 YAML 加载（只有 API key，其他用默认值）
    - 测试无效 YAML 返回错误
    - 测试缺少必填字段返回错误
  - **TDD GREEN**: 实现配置加载
  - **REFACTOR**: 清理代码

  **Must NOT do**:
  - 不添加配置验证库（如 validator）
  - 不添加配置热重载
  - 不添加环境变量覆盖（保持简单）

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: 简单的结构体扩展和测试
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with Tasks 1, 2, 3)
  - **Blocks**: None (独立任务)
  - **Blocked By**: None

  **References**:

  **Pattern References**:
  - `src/config.rs:1-16` — 现有 Config 结构体和 from_yaml 实现，在此基础上扩展
  - `conf.yaml.tmpl:1` — 配置模板格式

  **Acceptance Criteria**:

  **If TDD:**
  - [ ] `tests/config_test.rs` 存在，包含 ≥ 4 个测试
  - [ ] `cargo test --test config_test` → PASS

  **QA Scenarios (MANDATORY):**

  ```
  Scenario: 配置测试通过
    Tool: Bash
    Steps:
      1. cargo test --test config_test 2>&1
      2. 检查 "test result: ok"
    Expected Result: 所有配置测试通过
    Evidence: .sisyphus/evidence/task-4-config-tests.log

  Scenario: 默认值正确
    Tool: Bash
    Steps:
      1. cargo test --test config_test -- default_values 2>&1
    Expected Result: max_recording_seconds=60, audio_sample_rate=16000, audio_channels=1
    Evidence: .sisyphus/evidence/task-4-defaults.log
  ```

  **Commit**: YES (group with Wave 1)
  - Message: `feat(voice-input): extend config with audio and recording settings`
  - Files: `src/config.rs, tests/config_test.rs`
  - Pre-commit: `cargo test --test config_test`

- [x] 5. 音频采集模块 (audio.rs) + TDD

  **What to do**:
  - 在 `src/audio.rs` 中实现：
    - `trait AudioCapture` — 异步音频采集接口
      - `async fn start_capture(&mut self) -> Result<()>`
      - `async fn stop_capture(&mut self) -> Result<Vec<u8>>` — 返回采集的 PCM 数据
      - `fn is_capturing(&self) -> bool`
      - `fn elapsed_seconds(&self) -> u64`
    - `struct CpalAudioCapture` — cpal 实现
      - 内部使用 `cpal::default_input_device()` 获取设备
      - 配置为 16kHz, 16bit, 单声道 PCM
      - 使用 `Arc<Mutex<Vec<u8>>>` 缓存采集数据
      - 在 start_capture 中开始采集线程
      - 在 stop_capture 中停止采集并返回缓冲区
      - 支持最大录音时长（超过自动停止）
    - `#[cfg(test)] mod mock` — 使用 mockall 生成 mock：`mockall::mock!` 生成 `MockAudioCapture`
  - **TDD RED**: 在 `tests/audio_test.rs` 中编写：
    - 测试 MockAudioCapture 的 start/stop 生命周期
    - 测试 is_capturing 状态变化
    - 测试 elapsed_seconds 递增
    - 测试空音频数据返回
    - 测试最大录音时长限制
  - **TDD GREEN**: 实现 trait 和 cpal 后端
  - **REFACTOR**: 清理

  注意：cpal 实际设备测试放在集成测试中标记 `#[ignore]`，单元测试全部使用 mock。

  **Must NOT do**:
  - 不在单元测试中使用真实音频设备
  - 不使用 unsafe 代码
  - 不引入额外的音频处理库（如 dasp, hound）

  **Recommended Agent Profile**:
  - **Category**: `deep`
    - Reason: cpal API 较复杂，需要正确处理线程安全和异步
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with Tasks 6, 7, 8)
  - **Blocks**: Task 9
  - **Blocked By**: Tasks 1, 2

  **References**:

  **Pattern References**:
  - `src/asr.rs:96-168` — 现有 trait + impl 分离模式，audio.rs 应遵循类似模式
  - `src/main.rs:103-108` — 现有 pw-record 调用方式，参数 (--rate=16000 --channels=1 --format=s16) 是 cpal 需要匹配的配置

  **External References**:
  - cpal docs: https://docs.rs/cpal — Device, Stream, StreamConfig, SampleFormat
  - cpal examples: https://github.com/RustAudio/cpal/blob/master/examples/feedback.rs — 录音模式

  **Acceptance Criteria**:

  **If TDD:**
  - [ ] `tests/audio_test.rs` 存在，包含 ≥ 5 个测试
  - [ ] `cargo test --test audio_test` → PASS (使用 mock，不需要真实设备)

  **QA Scenarios (MANDATORY):**

  ```
  Scenario: 音频模块测试通过
    Tool: Bash
    Steps:
      1. cargo test --test audio_test 2>&1
      2. 检查 "test result: ok" 且 0 failures
    Expected Result: 所有 mock-based 测试通过
    Failure Indicators: 任何 test FAILED
    Evidence: .sisyphus/evidence/task-5-audio-tests.log

  Scenario: AudioCapture trait 定义正确
    Tool: Bash
    Steps:
      1. grep "trait AudioCapture" src/audio.rs — 应存在
      2. grep "fn start_capture" src/audio.rs — 应存在
      3. grep "fn stop_capture" src/audio.rs — 应存在
      4. grep "MockAudioCapture" src/audio.rs — 应存在（mockall 生成）
    Expected Result: trait 和 mock 都已定义
    Evidence: .sisyphus/evidence/task-5-trait-check.log
  ```

  **Commit**: YES
  - Message: `feat(voice-input): add audio capture module with cpal and TDD`
  - Files: `src/audio.rs, tests/audio_test.rs`
  - Pre-commit: `cargo test --test audio_test`

- [x] 6. 剪贴板模块 (clipboard.rs) + TDD

  **What to do**:
  - 在 `src/clipboard.rs` 中实现：
    - `trait Clipboard` — 剪贴板接口
      - `fn copy_text(&self, text: &str) -> Result<()>`
    - `struct WlCopyClipboard` — wl-copy 实现
      - 调用 `wl-copy` 子进程写入文本
      - 处理进程启动失败和写入错误
    - `#[cfg(test)] mod mock` — mockall 生成 `MockClipboard`
  - **TDD RED**: 在 `tests/clipboard_test.rs` 中编写：
    - 测试 MockClipboard.copy_text 成功路径
    - 测试 MockClipboard.copy_text 错误路径（模拟 wl-copy 不存在）
    - 测试空文本不调用 wl-copy
    - 测试包含特殊字符的文本（中文、换行等）
  - **TDD GREEN**: 实现模块
  - **REFACTOR**: 清理

  **Must NOT do**:
  - 不使用 wtype 模拟按键
  - 不引入 clipboard 库（保持 subprocess 调用 wl-copy 的简单方式）
  - 不在测试中调用真实 wl-copy

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: 简单的 trait + subprocess 封装
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with Tasks 5, 7, 8)
  - **Blocks**: Task 9
  - **Blocked By**: Task 1

  **References**:

  **Pattern References**:
  - `src/main.rs:142-166` — 现有 wl-copy 调用方式，clipboard.rs 应将此逻辑提取为 trait

  **Acceptance Criteria**:

  **If TDD:**
  - [ ] `tests/clipboard_test.rs` 存在，包含 ≥ 4 个测试
  - [ ] `cargo test --test clipboard_test` → PASS

  **QA Scenarios (MANDATORY):**

  ```
  Scenario: 剪贴板模块测试通过
    Tool: Bash
    Steps:
      1. cargo test --test clipboard_test 2>&1
      2. 检查 "test result: ok"
    Expected Result: 所有 mock 测试通过
    Evidence: .sisyphus/evidence/task-6-clipboard-tests.log

  Scenario: 不包含 wtype 调用
    Tool: Bash
    Steps:
      1. grep -r "wtype" src/clipboard.rs
      2. 应该返回空（无匹配）
    Expected Result: clipboard.rs 中无 wtype 引用
    Failure Indicators: 找到 "wtype" 字符串
    Evidence: .sisyphus/evidence/task-6-no-wtype.log
  ```

  **Commit**: YES
  - Message: `feat(voice-input): add clipboard module with wl-copy and TDD`
  - Files: `src/clipboard.rs, tests/clipboard_test.rs`
  - Pre-commit: `cargo test --test clipboard_test`

- [x] 7. ASR trait 抽象 + 重构 + TDD

  **What to do**:
  - 重构 `src/asr.rs`，将现有实现包装为 trait：
    - 提取 `trait AsrEngine` — ASR 识别接口
      - `async fn recognize(&self, audio_data: &[u8]) -> Result<String>` — 简化的接口，接收 PCM 音频数据，返回识别文本
    - 重命名现有 `AsrClient` → `DashScopeAsrEngine`，实现 `AsrEngine` trait
      - `recognize()` 方法内部：创建 session → 分块发送音频 → 等待结果
      - 复用现有的 `start_recognition`, `send_audio_chunk`, `finish_and_wait_result`
    - 添加 `mockall::mock!` 生成 `MockAsrEngine`
  - **TDD RED**: 在 `tests/asr_test.rs` 中编写：
    - 测试 MockAsrEngine.recognize 返回预期文本
    - 测试 MockAsrEngine.recognize 模拟超时错误
    - 测试 MockAsrEngine.recognize 模拟空结果
    - 测试 AsrConfig::default 的默认值
    - 将现有集成测试标记为 `#[ignore]`（保留但不默认运行）
  - **TDD GREEN**: 重构实现
  - **REFACTOR**: 确保 DashScope 实现不变，仅包装

  **Must NOT do**:
  - 不修改 DashScope WebSocket 协议实现的核心逻辑
  - 不删除现有的集成测试（标记 #[ignore] 即可）
  - 不引入新的 ASR 后端

  **Recommended Agent Profile**:
  - **Category**: `deep`
    - Reason: 需要理解现有 ASR 代码并安全重构
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with Tasks 5, 6, 8)
  - **Blocks**: Task 9
  - **Blocked By**: Tasks 1, 2

  **References**:

  **Pattern References**:
  - `src/asr.rs:96-276` — 完整的 DashScope ASR 客户端实现，需要在此基础上提取 trait
  - `src/asr.rs:278-346` — 现有集成测试，保留为 #[ignore]

  **Acceptance Criteria**:

  **If TDD:**
  - [ ] `tests/asr_test.rs` 存在，包含 ≥ 4 个 mock 测试
  - [ ] 现有集成测试保留且标记 `#[ignore]`
  - [ ] `cargo test --test asr_test` → PASS (仅 mock 测试)

  **QA Scenarios (MANDATORY):**

  ```
  Scenario: ASR trait 测试通过
    Tool: Bash
    Steps:
      1. cargo test --test asr_test 2>&1
      2. 检查 "test result: ok"
    Expected Result: 所有 mock 测试通过，integration test 被忽略
    Evidence: .sisyphus/evidence/task-7-asr-tests.log

  Scenario: DashScope 实现保留
    Tool: Bash
    Steps:
      1. grep "impl AsrEngine for DashScopeAsrEngine" src/asr.rs
      2. 应找到该实现
    Expected Result: trait 实现存在
    Evidence: .sisyphus/evidence/task-7-trait-impl.log
  ```

  **Commit**: YES
  - Message: `refactor(voice-input): extract ASR trait and add mock for TDD`
  - Files: `src/asr.rs, tests/asr_test.rs`
  - Pre-commit: `cargo test --test asr_test`

- [x] 8. Slint 后端集成 (gui.rs)

  **What to do**:
  - 在 `src/gui.rs` 中实现 Slint 后端绑定：
    - 使用 `slint::include_modules!` 引入 UI 定义
    - 创建 `struct VoiceInputGui` 封装 Slint 组件
      - `fn new() -> Result<Self>` — 创建窗口实例
      - `fn update_state(&self, state: &AppState)` — 根据 AppState 更新 UI
      - `fn set_result_text(&self, text: &str)` — 设置识别结果文本
      - `fn set_error_message(&self, msg: &str)` — 设置错误信息
      - `fn set_recording_duration(&self, seconds: u64)` — 更新录音时长
      - `fn run_event_loop(&self) -> Result<AppEvent>` — 非阻塞事件循环，返回用户操作
    - 处理键盘事件映射：F→FinishRecording, C/Escape→Cancel, R→Retry, Enter→Accept
    - 确保窗口在 Wayland 下正确显示为浮动面板
  - 编写测试验证状态到 UI 的映射逻辑（不启动真实窗口）

  **Must NOT do**:
  - 不在 GUI 代码中包含业务逻辑
  - 不在测试中打开真实窗口

  **Recommended Agent Profile**:
  - **Category**: `visual-engineering`
    - Reason: GUI 集成和事件绑定
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with Tasks 5, 6, 7)
  - **Blocks**: Task 9
  - **Blocked By**: Tasks 2, 3

  **References**:

  **Pattern References**:
  - `ui/gui.slint` — Task 3 创建的 UI 定义，gui.rs 需要与之对应

  **External References**:
  - Slint Rust API: https://slint.dev/docs/language/rust — ComponentHandle, set_property, invoke

  **Acceptance Criteria**:

  **QA Scenarios (MANDATORY):**

  ```
  Scenario: GUI 模块编译通过
    Tool: Bash
    Steps:
      1. cargo build 2>&1
      2. 检查退出码 == 0
    Expected Result: gui.rs 和 Slint 绑定编译成功
    Evidence: .sisyphus/evidence/task-8-gui-compile.log

  Scenario: 状态映射逻辑测试
    Tool: Bash
    Steps:
      1. cargo test gui 2>&1
      2. 检查状态映射测试通过
    Expected Result: AppState::Recording → UI 显示 "正在聆听..." 等
    Evidence: .sisyphus/evidence/task-8-gui-mapping.log
  ```

  **Commit**: YES
  - Message: `feat(voice-input): add Slint GUI backend integration`
  - Files: `src/gui.rs`
  - Pre-commit: `cargo build`

- [x] 9. 主事件循环重写 (main.rs)

  **What to do**:
  - 重写 `src/main.rs` 为新的 GUI 驱动架构：
    - 初始化：加载 Config → 创建 GUI → 创建音频/ASR/剪贴板实例
    - 主循环结构：
      ```
      loop {
        // 非阻塞检查 GUI 事件
        if let Some(event) = gui.poll_event() {
          match event {
            AppEvent::StartRecording => {
              audio.start_capture()?;
              state = AppState::Recording;
              gui.update_state(&state);
            }
            AppEvent::FinishRecording => {
              let pcm_data = audio.stop_capture()?;
              state = AppState::Transcribing;
              gui.update_state(&state);
              // spawn ASR task
              let text = asr.recognize(&pcm_data).await?;
              state = AppState::Result(text);
              gui.update_state(&state);
            }
            AppEvent::Accept => {
              if let AppState::Result(text) = &state {
                clipboard.copy_text(text)?;
              }
              state = AppState::Idle;
              gui.update_state(&state);
              gui.hide(); // 或关闭窗口
            }
            AppEvent::Cancel => {
              if state == Recording { audio.stop_capture()?; }
              state = AppState::Idle;
              gui.update_state(&state);
              gui.hide();
            }
            AppEvent::Retry => {
              state = AppState::Recording;
              audio.start_capture()?;
              gui.update_state(&state);
            }
            _ => {}
          }
        }
        // 非阻塞检查 FIFO
        if let Some(cmd) = fifo.try_recv() {
          match cmd {
            "START" => // 触发 StartRecording 事件
            "STOP" => // 触发 FinishRecording 事件
          }
        }
        // 更新录音时长显示
        if state == Recording {
          gui.set_recording_duration(audio.elapsed_seconds());
        }
      }
      ```
    - 使用 tokio 异步运行时处理 ASR 调用
    - 录音超时自动停止（超过 config.max_recording_seconds）
  - **TDD**: 编写集成测试 `tests/integration_test.rs`
    - 使用 MockAudioCapture, MockAsrEngine, MockClipboard
    - 测试完整流程：Start→Finish→Transcribe→Accept
    - 测试取消流程：Start→Cancel
    - 测试重试流程：Start→Finish→Result→Retry→Finish→Result→Accept
    - 测试超时：Start→等待超时→自动Finish

  **Must NOT do**:
  - 不在主循环中执行阻塞 I/O
  - 不使用 unwrap()（用 ? 或 proper error handling）
  - 不忘记在退出时清理资源（关闭 WebSocket、停止音频采集）

  **Recommended Agent Profile**:
  - **Category**: `deep`
    - Reason: 核心编排逻辑，需要正确整合所有模块
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: NO
  - **Parallel Group**: Wave 3 (sequential within wave, but T10/T11 can parallel after)
  - **Blocks**: Tasks 10, 11
  - **Blocked By**: Tasks 5, 6, 7, 8

  **References**:

  **Pattern References**:
  - `src/main.rs:23-85` — 现有主循环结构，新版本需要保留 FIFO 管道部分但重构其余部分
  - `src/main.rs:87-99` — setup_fifo 函数，保留此逻辑
  - `src/state.rs` — Task 2 定义的状态机，主循环需要使用 transition() 函数
  - `src/audio.rs` — Task 5 定义的 AudioCapture trait
  - `src/asr.rs` — Task 7 定义的 AsrEngine trait
  - `src/clipboard.rs` — Task 6 定义的 Clipboard trait
  - `src/gui.rs` — Task 8 定义的 VoiceInputGui

  **Acceptance Criteria**:

  **If TDD:**
  - [ ] `tests/integration_test.rs` 存在，包含 ≥ 4 个集成测试
  - [ ] `cargo test --test integration_test` → PASS

  **QA Scenarios (MANDATORY):**

  ```
  Scenario: 主循环集成测试通过
    Tool: Bash
    Steps:
      1. cargo test --test integration_test 2>&1
      2. 检查 "test result: ok"
    Expected Result: 所有 mock-based 集成测试通过
    Failure Indicators: 任何 test FAILED
    Evidence: .sisyphus/evidence/task-9-integration-tests.log

  Scenario: cargo build 成功
    Tool: Bash
    Steps:
      1. cargo build 2>&1
      2. 检查退出码 == 0
    Expected Result: 编译成功
    Evidence: .sisyphus/evidence/task-9-build.log
  ```

  **Commit**: YES
  - Message: `feat(voice-input): rewrite main event loop with GUI integration`
  - Files: `src/main.rs, tests/integration_test.rs`
  - Pre-commit: `cargo test && cargo build`

- [x] 10. FIFO 管道集成

  **What to do**:
  - 在新的 main.rs 中保留并集成 FIFO 管道功能：
    - 保留 `setup_fifo()` 函数
    - 保留 FIFO 监听线程
    - 将 FIFO 命令映射到 AppEvent：
      - START → AppEvent::StartRecording
      - STOP → AppEvent::FinishRecording
    - 确保管道命令和 GUI 操作可以互斥执行
    - 当 GUI 正在录音时，忽略重复的 FIFO START
  - 编写测试 `tests/fifo_test.rs`：
    - 测试 FIFO 命令解析
    - 测试重复 START 的幂等性
    - 测试 STOP without START 的容错性

  **Must NOT do**:
  - 不修改 FIFO 路径或权限设置
  - 不在管道中添加新命令（保持 START/STOP 兼容）

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: 基于现有代码的适配
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 3 (with Task 11, parallel after Task 9)
  - **Blocks**: Final Wave
  - **Blocked By**: Task 9

  **References**:

  **Pattern References**:
  - `src/main.rs:12-52` — 现有 FIFO 管道监听逻辑，需要适配到新的 event loop

  **Acceptance Criteria**:

  **If TDD:**
  - [ ] `tests/fifo_test.rs` 存在，包含 ≥ 3 个测试
  - [ ] `cargo test --test fifo_test` → PASS

  **QA Scenarios (MANDATORY):**

  ```
  Scenario: FIFO 测试通过
    Tool: Bash
    Steps:
      1. cargo test --test fifo_test 2>&1
    Expected Result: 所有 FIFO 测试通过
    Evidence: .sisyphus/evidence/task-10-fifo-tests.log

  Scenario: 实际 FIFO 端到端验证
    Tool: interactive_bash (tmux)
    Preconditions: cargo build 成功
    Steps:
      1. 启动程序: cargo run
      2. 写入 FIFO: echo "START" > /tmp/amao_voice_ime.fifo
      3. 等待 2 秒
      4. 写入 FIFO: echo "STOP" > /tmp/amao_voice_ime.fifo
      5. 检查程序输出包含 "识别" 相关信息
    Expected Result: FIFO 触发录音和识别流程
    Failure Indicators: 程序崩溃或无输出
    Evidence: .sisyphus/evidence/task-10-fifo-e2e.log
  ```

  **Commit**: YES
  - Message: `feat(voice-input): integrate FIFO pipe with new event loop`
  - Files: `src/main.rs, tests/fifo_test.rs`
  - Pre-commit: `cargo test --test fifo_test`

- [x] 11. 错误处理 + 边界场景

  **What to do**:
  - 在所有模块中添加健壮的错误处理：
    - **音频设备不可用**：显示 "无法访问音频设备" 错误状态
    - **ASR API 错误**：网络超时、认证失败、返回空结果
    - **剪贴板失败**：wl-copy 不存在或执行失败
    - **配置加载失败**：conf.yaml 不存在或格式错误
    - **长时间录音**：超过 max_recording_seconds 自动停止并发送已有音频
    - **空识别结果**：显示 "未检测到语音" 提示
    - **GUI 窗口焦点丢失**：Wayland 下窗口可能失去焦点，确保键盘事件仍能工作或提供鼠标点击支持
  - 编写测试 `tests/error_handling_test.rs`：
    - 测试 ASR 超时后的错误状态转换
    - 测试空识别结果的处理
    - 测试音频设备不可用的处理
    - 测试最大录音时长的自动停止
    - 测试连续多次 Start 的幂等性

  **Must NOT do**:
  - 不使用 panic! 处理可恢复错误
  - 不在错误信息中泄露 API key
  - 不添加自动重试机制（R 键由用户主动触发）

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: 需要仔细考虑各种错误路径
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 3 (with Task 10, parallel after Task 9)
  - **Blocks**: Final Wave
  - **Blocked By**: Task 9

  **References**:

  **Pattern References**:
  - `src/state.rs` — AppState::Error(String) 变体用于错误状态显示
  - `src/asr.rs:223-275` — 现有 ASR 超时处理逻辑（30 秒超时）

  **Acceptance Criteria**:

  **If TDD:**
  - [ ] `tests/error_handling_test.rs` 存在，包含 ≥ 5 个测试
  - [ ] `cargo test --test error_handling_test` → PASS

  **QA Scenarios (MANDATORY):**

  ```
  Scenario: 错误处理测试通过
    Tool: Bash
    Steps:
      1. cargo test --test error_handling_test 2>&1
    Expected Result: 所有错误处理测试通过
    Evidence: .sisyphus/evidence/task-11-error-tests.log

  Scenario: ASR 超时场景
    Tool: Bash
    Steps:
      1. cargo test --test error_handling_test -- timeout 2>&1
    Expected Result: 超时后状态转换为 Error("...timeout...")，可按 R 重试
    Evidence: .sisyphus/evidence/task-11-timeout.log
  ```

  **Commit**: YES
  - Message: `feat(voice-input): add comprehensive error handling and edge cases`
  - Files: `src/main.rs, src/state.rs, tests/error_handling_test.rs`
  - Pre-commit: `cargo test --test error_handling_test`

---

## Final Verification Wave (MANDATORY — after ALL implementation tasks)

> 4 review agents run in PARALLEL. ALL must APPROVE. Present consolidated results to user and get explicit "okay" before completing.

- [x] F1. **Plan Compliance Audit** — `oracle` ✅ PASS
  Read the plan end-to-end. For each "Must Have": verify implementation exists (read file, run command). For each "Must NOT Have": search codebase for forbidden patterns — reject with file:line if found. Check evidence files exist in .sisyphus/evidence/. Compare deliverables against plan.
  Output: `Must Have [N/N] | Must NOT Have [N/N] | Tasks [N/N] | VERDICT: APPROVE/REJECT`

- [x] F2. **Code Quality Review** — `unspecified-high` ✅ PASS
  Run `cargo build` + `cargo clippy` + `cargo test`. Review all changed files for: unsafe code, unwrap() in production paths, empty catches, commented-out code, unused imports. Check AI slop: excessive comments, over-abstraction, generic names.
  Output: `Build [PASS/FAIL] | Clippy [PASS/FAIL] | Tests [N pass/N fail] | Files [N clean/N issues] | VERDICT`

- [x] F3. **Real Manual QA** — `unspecified-high` ✅ PASS
  Start from clean state. Execute EVERY QA scenario from EVERY task — follow exact steps, capture evidence. Test cross-task integration. Test edge cases: empty recognition, timeout, double-start. Save to `.sisyphus/evidence/final-qa/`.
  Output: `Scenarios [N/N pass] | Integration [N/N] | Edge Cases [N tested] | VERDICT`

- [x] F4. **Scope Fidelity Check** — `deep` ✅ PASS
  For each task: read "What to do", read actual diff (git log/diff). Verify 1:1 — everything in spec was built (no missing), nothing beyond spec was built (no creep). Check "Must NOT do" compliance. Detect cross-task contamination.
  Output: `Tasks [N/N compliant] | Contamination [CLEAN/N issues] | Unaccounted [CLEAN/N files] | VERDICT`

---

## Commit Strategy

- **Wave 1**: `feat(voice-input): add project config and state machine types` - Cargo.toml, src/state.rs, src/config.rs
- **Wave 1**: `feat(voice-input): add Slint UI definition` - src/gui.slint
- **Wave 2**: `feat(voice-input): add audio capture module with TDD` - src/audio.rs
- **Wave 2**: `feat(voice-input): add clipboard module with TDD` - src/clipboard.rs
- **Wave 2**: `refactor(voice-input): extract ASR trait for testability` - src/asr.rs
- **Wave 2**: `feat(voice-input): add Slint GUI backend integration` - src/gui.rs
- **Wave 3**: `feat(voice-input): rewrite main event loop with GUI` - src/main.rs
- **Wave 3**: `feat(voice-input): integrate FIFO with new event loop` - src/main.rs
- **Wave 3**: `feat(voice-input): add error handling and edge cases` - src/main.rs, src/state.rs
- **Final**: `chore(voice-input): cleanup and final verification` - all files

---

## Success Criteria

### Verification Commands
```bash
cargo build                    # Expected: Successful compilation
cargo test                     # Expected: All tests pass, 0 failures
cargo clippy                   # Expected: No warnings or errors
```

### Final Checklist
- [ ] All "Must Have" present
- [ ] All "Must NOT Have" absent
- [ ] All tests pass
- [ ] GUI shows correct states (listening/parsing/result/error)
- [ ] F/C/ESC/R/Enter keys work correctly in GUI
- [ ] Win+Space via Hyprland + FIFO triggers recording
- [ ] Accepted text appears in clipboard (wl-copy)
- [ ] No wtype usage in codebase
