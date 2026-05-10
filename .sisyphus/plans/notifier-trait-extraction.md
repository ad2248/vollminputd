# Notifier Trait 提取 — 六边形架构改造

## TL;DR

> 将现有的 `notification.rs` 中的直接函数调用重构为 `Notifier` Trait，实现生产环境 `NotifyRustNotifier`（包装 notify-rust）和测试环境 `MockNotifier`（mockall 自动生成）+ `TestNotifier`（channel 收集）。保持 `VoiceInputApp` 纯业务逻辑不变，`SideEffect::Notify` 继续作为命令返回，执行层通过 Trait 解耦。
>
> **Deliverables**:
> - `src/notifier/mod.rs` — Notifier trait + NotifyRustNotifier 实现
> - `src/lib.rs` 更新 — 导出 notifier 模块
> - `src/notification.rs` 删除 — 逻辑迁移至 NotifyRustNotifier
> - `src/main.rs` 更新 — execute_effect() 接收 `&dyn Notifier`
> - `tests/notifier_test.rs` — MockNotifier + TestNotifier 测试
>
> **Estimated Effort**: Short (~2-3 小时)
> **Parallel Execution**: YES — 2 Waves
> **Critical Path**: T1-T3 (Wave 1) → T4-T5 (Wave 2) → F1-F3 (Final)

---

## Context

### Original Request
为 Rust 麦克风转录程序设计并实现基于六边形架构的测试体系，提取 `Notifier` Trait，生产环境走 notify-rust，测试环境走 Vec/channel 断言。

### Interview Summary
**Key Decisions**:
- IPC 方式：保留现有 FIFO 命名管道，不新增 Socket
- Notifier：提取 Trait，生产用 notify-rust，测试用 channel 断言
- 范围：先不考虑 E2E、proptest、Socket 等其他内容

### Research Findings
- 项目已具备成熟的六边形架构基础：AudioCapture、AsrEngine、Clipboard 均已 Trait 化 + automock
- 通知当前是纯函数 `notify()` 在 `notification.rs`，直接调用 notify-rust
- `VoiceInputApp` 生成 `SideEffect::Notify`，`main.rs` 的 `execute_effect()` 执行实际通知
- 9 个测试文件、52+ 测试用例全部通过

### Metis Review
**Identified Gaps** (addressed in this plan):
- `notify()` 当前返回 `()` 且内部吞掉错误 → **决策**: Trait 返回 `Result<(), Box<dyn Error + Send + Sync>>`，execute_effect 保持现有行为（忽略错误）
- 用户要求"Vec/channel 断言"与现有 mockall 模式冲突 → **决策**: 主要使用 `MockNotifier`（mockall，与项目一致），额外提供 `TestNotifier`（channel-based）用于集成测试场景
- `notification.rs` 的命运 → **决策**: 删除，逻辑完整迁移至 `NotifyRustNotifier`

---

## Work Objectives

### Core Objective
将通知系统从直接函数调用重构为 Trait 抽象，使执行层可通过依赖注入替换实现，支持 mockall 自动 mock 和 channel-based 测试收集。

### Concrete Deliverables
- `src/notifier/mod.rs` — 新模块，包含 Notifier trait、NotifyRustNotifier、TestNotifier
- `src/lib.rs` — 更新模块导出
- `src/notification.rs` — 删除（逻辑迁移完成）
- `src/main.rs` — execute_effect / execute_effect_with_asr 适配
- `tests/notifier_test.rs` — 新测试文件

### Definition of Done
- [x] `cargo build` 零错误
- [x] `cargo test` 全部通过（52+ 现有测试 + 新增测试）
- [x] `src/notification.rs` 不存在
- [x] `MockNotifier` 可在测试中使用
- [x] `TestNotifier` 可收集通知并通过 channel 断言

### Must Have
- Notifier trait 使用 `#[mockall::automock]` + `Send + Sync`
- NotifyRustNotifier 行为与现有 `notify()` 完全一致（stdout/stderr 输出、错误处理）
- 不修改 `app.rs`、`state.rs`、`SideEffect` 枚举
- 不修改 FIFO/IPC 代码
- 不添加新依赖

### Must NOT Have (Guardrails)
- 不修改 VoiceInputApp 泛型参数（保持 `<A, C>` 不加入 Notifier）
- 不修改 SideEffect::Notify 结构或任何生成它的代码
- 不新增 Socket、E2E 测试、proptest、Docker 相关内容
- 不改动 ASR、AudioCapture、Clipboard 模块
- 不改timeout_secs为Duration类型

---

## Verification Strategy

### Test Decision
- **Infrastructure exists**: YES — cargo test + mockall
- **Automated tests**: Tests-after（现有模式，非 TDD）
- **Framework**: cargo test + mockall 0.13

### QA Policy
Every task includes agent-executed QA scenarios. Evidence saved to `.sisyphus/evidence/`.

- **Library/Module**: Bash (cargo test / cargo build) — compile, test, grep assertions
- **Each scenario**: exact command + exact expected output + evidence path

---

## Execution Strategy

### Parallel Execution Waves

```
Wave 1 (Foundation — 3 parallel tasks):
├── Task 1: 创建 src/notifier/mod.rs (Notifier trait + NotifyRustNotifier + TestNotifier)
├── Task 2: 更新 src/lib.rs (导出 notifier，移除 notification)
└── Task 3: 删除 src/notification.rs

Wave 2 (Adaptation + Tests — 2 parallel tasks, depends on Wave 1):
├── Task 4: 更新 src/main.rs (execute_effect 接收 &dyn Notifier)
└── Task 5: 创建 tests/notifier_test.rs (MockNotifier + TestNotifier 测试)

Wave FINAL (Verification — 3 parallel reviews):
├── Task F1: 编译验证 — cargo build 零错误
├── Task F2: 测试验证 — cargo test 全部通过
└── Task F3: 代码审查 — 文件变更合规检查

Critical Path: T1/T2/T3 → T4/T5 → F1/F2/F3
Parallel Speedup: Wave 1 (3x), Wave 2 (2x), Final (3x)
```

---

## TODOs

- [x] 1. **创建 `src/notifier/mod.rs` — Notifier Trait + 实现**

  **What to do**:
  创建新文件 `src/notifier/mod.rs`，包含以下内容：

  1. **Notifier trait**（同步 trait，参考 `src/clipboard.rs` 模式）：
     - `#[mockall::automock]` + `pub trait Notifier: Send + Sync`
     - 方法签名：`fn notify(&self, title: &str, body: &str, timeout_secs: u32) -> Result<(), Box<dyn std::error::Error + Send + Sync>>`
     - 与现有 `notify()` 函数的参数完全一致

  2. **NotifyRustNotifier struct**：
     - 空结构体 `pub struct NotifyRustNotifier;`
     - `impl Notifier for NotifyRustNotifier`：将现有 `notification.rs` 中 `notify()` 的完整逻辑迁移至此
     - 保持完全一致的 stdout/stderr 输出格式：`[INFO] 通知: ...` 和 `[ERROR] 发送通知失败: ...`
     - 内部调用 `notify_rust::Notification::new().summary(title).body(body).timeout(...).show()`
     - timeout 逻辑：`timeout_secs == 0` → `Timeout::Never`，否则 `Timeout::Milliseconds(timeout_secs * 1000)`

  3. **TestNotifier struct**（channel-based，用于集成测试场景）：
     - `pub struct TestNotifier { tx: mpsc::Sender<NotificationRecord> }`
     - `pub struct NotificationRecord { pub title: String, pub body: String, pub timeout_secs: u32 }`
     - `impl Notifier for TestNotifier`：将每个通知通过 channel 发送，返回 `Ok(())`
     - 提供构造函数 `new() -> (Self, mpsc::Receiver<NotificationRecord>)`

  **Must NOT do**:
  - 不要添加 async_trait — Notifier 保持同步（notify-rust 是同步 API）
  - 不要改变现有 `notify()` 的可观察行为（日志输出、错误处理策略）
  - 不要在此文件中添加测试代码

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: 单一文件创建，模式明确，参考现有 trait 定义
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with T2, T3)
  - **Blocks**: T4, T5
  - **Blocked By**: None

  **References**:
  - `src/clipboard.rs:6-10` — Clipboard trait 模式（automock + Send + Sync + sync trait）
  - `src/notification.rs:1-21` — 现有 notify() 函数逻辑，需完整迁移
  - `src/audio.rs` — AudioCapture trait 模式（automock + async_trait，仅供参考不要复制 async）

  **WHY Each Reference Matters**:
  - `clipboard.rs`: 这是同步 trait + automock 的标准模板，Notifier 应完全遵循此模式
  - `notification.rs`: 现有业务逻辑，NotifyRustNotifier::notify() 的行为必须与此文件逐行等价

  **Acceptance Criteria**:
  - [ ] `src/notifier/mod.rs` 存在且可编译
  - [ ] `grep "pub trait Notifier" src/notifier/mod.rs` → found
  - [ ] `grep "automock" src/notifier/mod.rs` → found
  - [ ] `grep "NotifyRustNotifier" src/notifier/mod.rs` → found
  - [ ] `grep "TestNotifier" src/notifier/mod.rs` → found

  **QA Scenarios**:

  ```
  Scenario: Notifier trait compiles with automock
    Tool: Bash
    Preconditions: 文件已创建
    Steps:
      1. cargo check --lib 2>&1 | grep -c "error"
    Expected Result: 0 errors (may have warnings from unused code)
    Failure Indicators: 编译错误、automock 生成失败
    Evidence: .sisyphus/evidence/task-1-compile-check.txt
  ```

  **Commit**: NO (groups with Wave 1)

- [x] 2. **更新 `src/lib.rs` — 模块导出**

  **What to do**:
  1. 添加 `pub mod notifier;` 到模块列表
  2. 移除 `pub mod notification;`（该模块将被删除）

  **Must NOT do**:
  - 不要改变其他模块的导出顺序或方式

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with T1, T3)
  - **Blocks**: None (T1 完成后 T4/T5 才需要完整编译)
  - **Blocked By**: None

  **References**:
  - `src/lib.rs:1-7` — 当前模块列表

  **Acceptance Criteria**:
  - [ ] `grep "pub mod notifier" src/lib.rs` → found
  - [ ] `grep "pub mod notification" src/lib.rs` → not found

  **QA Scenarios**:
  ```
  Scenario: lib.rs exports notifier module
    Tool: Bash
    Steps:
      1. grep "pub mod notifier" src/lib.rs
      2. grep "pub mod notification" src/lib.rs
    Expected Result: (1) found, (2) not found
    Evidence: .sisyphus/evidence/task-2-lib-exports.txt
  ```

  **Commit**: NO (groups with Wave 1)

- [x] 3. **删除 `src/notification.rs` — 逻辑已迁移**

  **What to do**:
  删除 `src/notification.rs` 文件。所有逻辑已迁移至 `NotifyRustNotifier`。

  **Must NOT do**:
  - 不要留下空文件或占位文件

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with T1, T2)
  - **Blocks**: T4 (main.rs 不再能 import notification::notify)
  - **Blocked By**: T1 (NotifyRustNotifier 必须先存在)

  **Acceptance Criteria**:
  - [ ] `ls src/notification.rs` → "No such file or directory"

  **QA Scenarios**:
  ```
  Scenario: notification.rs is removed
    Tool: Bash
    Steps:
      1. ls src/notification.rs 2>&1
    Expected Result: "No such file or directory"
    Evidence: .sisyphus/evidence/task-3-file-deleted.txt
  ```

  **Commit**: NO (groups with Wave 1)

- [x] 4. **更新 `src/main.rs` — execute_effect 接收 `&dyn Notifier`**

  **What to do**:
  1. 移除 `use VoiceInput::notification::notify;` import
  2. 添加 `use VoiceInput::notifier::{Notifier, NotifyRustNotifier};`
  3. 在 `main()` 中创建 `let notifier = NotifyRustNotifier;`
  4. 修改 `execute_effect(effect: &SideEffect)` → `execute_effect(effect: &SideEffect, notifier: &dyn Notifier)`
  5. 在 `SideEffect::Notify` 分支中，将 `notify(title, body, *timeout_secs)` 替换为 `let _ = notifier.notify(title, body, *timeout_secs);`
     - 保持 `let _ = ...` 以忽略错误，维持现有行为
  6. 修改 `execute_effect_with_asr(effect, config, tx)` → `execute_effect_with_asr(effect, config, tx, notifier: &dyn Notifier)`
  7. 在 `main()` 的 effect 处理循环中，将 `execute_effect_with_asr(effect, &config, &event_tx)` 替换为 `execute_effect_with_asr(effect, &config, &event_tx, &notifier)`
  8. 在 `poll_effects` 的处理中同样传递 `&notifier`（`execute_effect(effect, &notifier)`）

  **Must NOT do**:
  - 不要修改 FIFO 监听循环
  - 不要修改 ASR 引擎创建和调用逻辑
  - 不要修改事件状态机逻辑
  - 不要修改错误处理策略（继续保持忽略 notify 错误）

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES (with T5)
  - **Parallel Group**: Wave 2
  - **Blocked By**: T1, T2, T3 (notifier 模块必须存在)

  **References**:
  - `src/main.rs:6` — 现有 `use VoiceInput::notification::notify;` import
  - `src/main.rs:129-146` — `execute_effect()` 函数
  - `src/main.rs:148-183` — `execute_effect_with_asr()` 函数
  - `src/main.rs:118-124` — effect 处理循环
  - `src/main.rs:103-106` — poll_effects 处理

  **WHY Each Reference Matters**:
  - main.rs:6: 这是需要移除的 import 行
  - main.rs:129-146: execute_effect 是核心修改点，需要添加 notifier 参数并替换 notify() 调用
  - main.rs:148-183: 需要透传 notifier 参数到 execute_effect
  - main.rs:118-124 / 103-106: 所有调用点都需要传递 &notifier

  **Acceptance Criteria**:
  - [ ] `grep "notification::notify" src/main.rs` → not found
  - [ ] `grep "execute_effect(effect: &SideEffect, notifier: &dyn Notifier)" src/main.rs` → found (or equivalent signature)
  - [ ] `grep "notifier.notify" src/main.rs` → found
  - [ ] `cargo build --bin VoiceInput` → 0 errors

  **QA Scenarios**:
  ```
  Scenario: main.rs compiles with Notifier trait
    Tool: Bash
    Preconditions: Wave 1 完成
    Steps:
      1. cargo build --bin VoiceInput 2>&1 | tee build.log
      2. grep -c "error" build.log
    Expected Result: 0 errors
    Failure Indicators: 编译错误、trait bound 不匹配、找不到模块
    Evidence: .sisyphus/evidence/task-4-main-build.txt
  ```

  **Commit**: NO (groups with Wave 2)

- [x] 5. **创建 `tests/notifier_test.rs` — MockNotifier + TestNotifier 测试**

  **What to do**:
  创建 `tests/notifier_test.rs`，包含以下测试：

  1. **MockNotifier 基础测试**（参考 `tests/clipboard_test.rs` 模式）：
     - `test_mock_notifier_called_with_expected_args`：创建 `MockNotifier`，设置 `expect_notify`  with `with(eq("标题"), eq("内容"), eq(5))`，调用 `.notify("标题", "内容", 5)`，验证 mock 自动断言通过
     - `test_mock_notifier_called_exactly_n_times`：创建 `MockNotifier`，设置 `expect_notify` with `times(2)`，调用两次，验证通过

  2. **TestNotifier channel 测试**（满足用户"Vec/channel 断言"需求）：
     - `test_test_notifier_collects_notifications`：创建 `TestNotifier`，发送 3 条通知，通过 receiver 收集，断言 Vec 长度和内容完全匹配
     - `test_test_notifier_empty_when_no_notifications`：创建 `TestNotifier` 但不发通知，断言 receiver 为空

  3. **NotifyRustNotifier 存在性测试**（可选，不测试实际 GUI）：
     - `test_notify_rust_notifier_exists`：验证 `NotifyRustNotifier` 可实例化且实现 `Notifier` trait

  **Must NOT do**:
  - 不要测试 notify-rust 是否真的弹出桌面通知（CI 环境无 GUI）
  - 不要引入新的测试依赖

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES (with T4)
  - **Parallel Group**: Wave 2
  - **Blocked By**: T1, T2, T3

  **References**:
  - `tests/clipboard_test.rs` — MockClipboard 测试模式（expect_xxx, with, times, return_once）
  - `src/notifier/mod.rs` — TestNotifier 结构定义和 channel API
  - `src/clipboard.rs:6-10` — Clipboard trait 签名模式（与 Notifier 相同：sync + automock + Result 返回）

  **WHY Each Reference Matters**:
  - clipboard_test.rs: 这是项目中最接近的测试模板。MockNotifier 的使用方式应与 MockClipboard 完全一致
  - notifier/mod.rs: TestNotifier 的 API 设计（new() 返回 (Self, Receiver)，notify() 发送消息）

  **Acceptance Criteria**:
  - [ ] `cargo test notifier` → all pass
  - [ ] 至少 4 个测试函数（2 mock + 2 channel）
  - [ ] `grep "MockNotifier" tests/notifier_test.rs` → found
  - [ ] `grep "TestNotifier" tests/notifier_test.rs` → found

  **QA Scenarios**:
  ```
  Scenario: Notifier tests pass
    Tool: Bash
    Preconditions: Wave 1 完成
    Steps:
      1. cargo test notifier --test notifier_test 2>&1 | tee test.log
      2. tail -5 test.log
    Expected Result: "test result: ok. 4 passed; 0 failed"
    Failure Indicators: 测试失败、mock 期望不匹配、channel 超时
    Evidence: .sisyphus/evidence/task-5-notifier-test.txt
  ```

  **Commit**: NO (groups with Wave 2)

---

## Final Verification Wave

- [x] F1. **编译验证**
  运行 `cargo build`，确认零编译错误、零警告（或现有警告未增加）。验证 `src/notification.rs` 已删除。

- [x] F2. **测试验证**
  运行 `cargo test`，确认所有 52+ 现有测试通过，新增 notifier 测试通过。统计测试数量并对比基线。

- [x] F3. **变更合规审查**
  检查 git diff，确认仅修改了计划内的文件（notifier/mod.rs、lib.rs、main.rs、tests/notifier_test.rs），未触碰 app.rs、state.rs、audio.rs、asr/、clipboard.rs、config.rs。

---

## Commit Strategy

- **Wave 1 + 2**: `refactor(notification): extract Notifier trait with NotifyRustNotifier and TestNotifier`
- **Wave Final**: `test(notifier): add MockNotifier and TestNotifier tests`

## Success Criteria

### Verification Commands
```bash
# 编译验证
cargo build 2>&1 | grep -c "error"   # Expected: 0

# 测试验证
cargo test 2>&1 | tail -10           # Expected: test result: ok. N passed; 0 failed

# 文件删除验证
ls src/notification.rs 2>&1          # Expected: No such file or directory

# Trait 导出验证
grep "pub mod notifier" src/lib.rs   # Expected: found

# main.rs 不再直接调用 notify
grep -n "notification::notify" src/main.rs  # Expected: nothing

# MockNotifier 存在验证
grep "MockNotifier" tests/notifier_test.rs   # Expected: found
```

### Final Checklist
- [x] 所有 "Must Have" 满足
- [x] 所有 "Must NOT Have" 未违反
- [x] 所有现有测试通过
- [x] 新增测试通过
- [x] `cargo build` 零错误
