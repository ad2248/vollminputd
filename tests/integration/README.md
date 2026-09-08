# 集成测试

在 podman 容器里端到端验证 vollminputd：构建 + `cargo test --locked` + makepkg 打包在同一个容器内完成；每个测试场景再起一个全新的一次性容器跑语音链路（headless PipeWire + Sway 全部在容器内原生生起，不挂 host 设备）。

## 测试内容

- **构建/打包**：容器内产出 `.pkg.tar.zst`。复用仓库根 [PKGBUILD](../../PKGBUILD) 的生产打包逻辑，测试侧只做 source/version 覆盖（wrapper），不维护另一套打包逻辑。
- **工具链回归**：故意在 PATH 前放置不可用的 Cargo/Rust 包装器，并设置错误的 `RUSTC`/`RUSTDOC`，确认配方仍使用发行版工具链。另检查 `.SRCINFO` 与正式配方一致。最低依赖要求为 Rust 1.87；容器固定工具链的构建通过不代表所有最低版本组合或 AUR 发布渠道均已验证。
- **TOGGLE E2E**：容器内 `pacman -U` 安装产物 → PipeWire + wireplumber + pw-loopback 虚拟麦克风 → sway headless → 启动 vollminputd → FIFO `TOGGLE` ×2 夹一段 `pw-play` 播放 → 断言剪贴板。音频走 daemon 真实的 cpal 采集路径（ALSA → PipeWire 虚拟麦克风），不注入 PCM。
- **ASR**：单一原生 HTTP 后端/模型 `qwen-audio-3.0-asr-flash`；一个 offline 场景（2 轮 TOGGLE）+ 一个 live 场景（1 轮 TOGGLE）。
  - offline（默认）：容器内本地 HTTP mock 按原生协议应答（`input.messages` + `parameters`，其中 `format="wav"`、`sample_rate="16000"`，音频为 `data:audio/wav;base64,...`；应答取 `text` 或 `output.text`），校验鉴权与音频非空非静音，剪贴板必须精确等于预期文本。
  - live（`--live`）：真打云端原生 HTTP 端点，key 经容器环境变量注入，缺 key 直接失败（不是跳过）；断言剪贴板等于 daemon 实际识别输出。live 只是冒烟验证，不是识别质量基准。
- **通知**：容器内有 DBus session，通知链路会执行，但不断言通知内容。

## Host 前置条件

- Linux x86_64，专用原生 host（作业与容器直接跑在宿主机，不是嵌套/共享环境）
- podman（rootless 或 rootful），执行用户能直接 `podman build` / `podman run`
- Python 3.11+ 与 pytest
- Git（源码快照依赖工作区状态）
- 若用作 Gitea Actions runner：Node 20（`actions/checkout@v4` 依赖）

## 使用

```bash
cd <仓库根>

# 离线套件（默认；live 用例被排除，不需要 API key）
python3 tests/integration/run.py

# 过滤参数原样转发给 pytest
python3 tests/integration/run.py -q
python3 tests/integration/run.py -k toggle

# live：缺 key 会直接失败（不是 skip）
python3 tests/integration/run.py --live

# 显式指定 key 文件
python3 tests/integration/run.py --live --key-file /path/to/local.env
```

也可以直接 `pytest tests/integration/`，效果等价（run.py 只是编排包装）。

### API key

优先级：环境变量 `VOLLMINPUTD_DASHSCOPE_API_KEY` > `--key-file` 指定的文件（缺省即 `tests/integration/.env.test`）。key 文件支持 dotenv 或裸值格式（向后兼容）。默认 `.env.test` 已被 gitignore；自定义密钥文件应放在仓库外或自行加入忽略规则。源码快照排除 `.env*` 及本次指定的密钥文件。

安全提醒：key 一旦进入过 git 历史或其他共享渠道，即应视为已泄露并立即轮换；不要把 key 写进任何仓库文件。

## 容器行为约定

- 源码快照取自工作区，包含未被 ignore 的相关未跟踪文件；
- 每次运行唯一会话 ID，不跨会话复用陈旧的包/产物；
- 测试运行时容器默认 `--network=none`；镜像构建与 live 用例需要外网；
- 测试镜像钉版：Arch 基础镜像固定 digest + 2026-09-01 软件包归档快照，保留包签名校验；基础与依赖集已钉死，但编译存在非确定性，产物以内容哈希标识，不保证逐位一致的复现；
- E2E 用 sway headless（Weston 15 headless 不提供 wl_seat，wl-copy 无法工作）；容器内对 `/usr/bin/sway` 执行 `setcap -r`，默认非特权 capabilities 下避免 EPERM，无 `--privileged`、无额外 caps；
- 不做特权容器、不挂 podman socket、不挂 host 设备。

## CI（`.gitea/workflows/tests.yml`）

- 触发：push 到 `main`、`pull_request`、`workflow_dispatch`、每日定时；不使用 `pull_request_target`。
- Runner：`runs-on: voiceinput-linux`，注册 act_runner 时必须带 host 执行标签 `voiceinput-linux:host`。作业直接跑在宿主机上，podman 编排由 pytest fixtures 完成。
- live 门控：配置了 secret `VOLLMINPUTD_DASHSCOPE_API_KEY` 才执行 live pytest，否则该阶段记录原因后退出；其余阶段不受影响。密钥只经 live 阶段的环境变量注入，不落文件、不在 shell 里内插。
- 产物：`actions/upload-artifact@v3`（该 action 的 v4 与 Gitea 不兼容），`if: always()`，只上传 `VOLLMINPUTD_TEST_OUTPUT` 指向的本次运行目录，不包含旧会话。
- 阶段：runner 隔离检查、测试镜像、打包/Rust 测试、harness、offline E2E、live E2E 分别显示；阶段间通过 `VOLLMINPUTD_TEST_OUTPUT` 指定的运行目录复用镜像 ID、源码快照和软件包，不重复构建。
- 基础镜像：`Containerfile.base` 固化系统依赖和 `Cargo.lock` 对应的 vendored crates；发布及依赖升级流程见 [CI_IMAGE_RUNBOOK.md](CI_IMAGE_RUNBOOK.md)。切换到 Registry digest 前，现有 `Containerfile` 仍负责本地构建依赖镜像。
- fork 防护：同仓库 PR 检查在 checkout 之前跳过 fork 触发的整个 job，这是**尽力而为的过滤，不是安全屏障**——PR 作者可以修改自己的 workflow，因此不能视为隔离手段。在执行不受信贡献者的 workflow 之前，必须先有仅限可信成员的 runner 与 Gitea 服务端策略。

### Runner 部署要求

1. 准备专用 Linux x86_64 原生 host，装齐上文前置条件；
2. 将仓库与 runner 限制给可信贡献者，并配置 Gitea 服务端策略；
3. 注册 runner 并带 host 标签：`act_runner register --labels voiceinput-linux:host ...`；建议单 job 串行（默认 capacity=1）；
4. （可选）在仓库 Secrets 添加 `VOLLMINPUTD_DASHSCOPE_API_KEY`。

仓库 CI 已使用上述 host 标签运行；重新部署 runner 时仍需满足这些约束。

## 覆盖边界（不在测试范围内）

- 物理麦克风、真实桌面快捷键触发、真实剪贴板粘贴、真实桌面/GUI 均未测试，容器内以虚拟音频源与协议级断言替代；
- PipeWire 与 Wayland 是容器内原生生起的，不等价于真实桌面环境；
- mock 服务端只验证协议正确性与音频路由；`--live` 是冒烟验证（云端可达、daemon 工作），不是识别质量基准。
