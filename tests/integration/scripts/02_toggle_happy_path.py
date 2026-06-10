"""
第二个测试用例：真打 DashScope 的 TOGGLE happy path。
- pacman -U /build-out/*.pkg.tar.zst 第一件事
- 起 PipeWire + wireplumber
- 起 weston headless
- 起 vollminputd
- pw-play 喂测试音频进 fake-mic
- 写 TOGGLE 到 FIFO
- 等 ASR 回调
- 读 wl-paste 断言非空

注意：镜像没装 pactl（pipewire-pulse 跟 pulseaudio 包冲突），改用 pipewire 配置注入 null-sink。
"""
import os
import random
import signal
import subprocess
import sys
import time
import uuid
from pathlib import Path

OUTPUT_DIR = Path("/build-out")
KEY_FILE = Path("/run/secrets/key")
AUDIO = Path("/tests/repo-tests/test_audio.wav")
PKG_NAME = "vollminputd-git"

INSTANCE = os.environ.get("TEST_INSTANCE", f"pytest-{uuid.uuid4().hex[:8]}")
FIFO_PATH = Path(f"/tmp/vollminputd_{INSTANCE}.fifo")
DAEMON_LOG = Path("/tmp/daemon.log")
WESTON_SOCKET = f"wayland-test-{INSTANCE}"

XDG_RUNTIME_DIR = Path("/run/user/0")
XDG_RUNTIME_DIR.mkdir(parents=True, exist_ok=True)
XDG_RUNTIME_DIR.chmod(0o700)


def log(msg):
    print(f"[02_toggle] {msg}", flush=True)


def run_bg(cmd, **kw):
    log(f"$ {' '.join(str(c) for c in cmd)} (background)")
    return subprocess.Popen(cmd, start_new_session=True, **kw)


def cleanup(procs):
    for p in procs:
        try:
            p.send_signal(signal.SIGTERM)
        except ProcessLookupError:
            pass
    time.sleep(1)
    for p in procs:
        try:
            p.kill()
        except ProcessLookupError:
            pass


def setup_fake_mic():
    """
    注入 null-sink 配置到 ~/.config/pipewire/pipewire.conf.d/
    pipewire 启动时自动 load module-null-sink 作为 fake-mic + fake-mic.monitor source
    """
    cfg_dir = Path("/root/.config/pipewire/pipewire.conf.d")
    cfg_dir.mkdir(parents=True, exist_ok=True)
    fake_mic_conf = cfg_dir / "99-fake-mic.conf"
    fake_mic_conf.write_text("""\
# 集成测试专用：注入 fake-mic null-sink
context.modules = [
    {   name = libpipewire-module-null-sink
        args = {
            sink.name = "fake-mic"
            sink.props = {
                "node.name" = "fake-mic"
                "device.description" = "FakeMicITest"
            }
        }
    }
]
""")
    log(f"写入假 mic 配置: {fake_mic_conf}")


def wait_for_fake_mic(timeout_sec=15) -> bool:
    """轮询 pw-cli ls Node 直到 fake-mic 出现。"""
    for i in range(timeout_sec):
        r = subprocess.run(["pw-cli", "ls", "Node"], capture_output=True, text=True)
        if r.returncode == 0 and "fake-mic" in r.stdout:
            log(f"pw-cli ls Node 找到 fake-mic ({i+1}s)")
            return True
        time.sleep(1)
    return False


def set_default_source() -> bool:
    """尝试将 fake-mic.monitor 设为默认 source。"""
    # pw-cli 的 set-default 需要对象类型和名称
    # 先尝试 Node 类型
    r = subprocess.run(
        ["pw-cli", "set-default", "Node", "fake-mic.monitor"],
        capture_output=True, text=True,
    )
    if r.returncode == 0:
        log("pw-cli set-default Node fake-mic.monitor 成功")
        return True
    # 老版本可能不支持类型参数，直接试名称
    r = subprocess.run(
        ["pw-cli", "set-default", "fake-mic.monitor"],
        capture_output=True, text=True,
    )
    if r.returncode == 0:
        log("pw-cli set-default fake-mic.monitor 成功")
        return True
    log(f"pw-cli set-default 失败: {r.stderr}")
    return False


def main() -> int:
    log("=== STEP 0: 注入假 mic 配置到 pipewire ===")
    setup_fake_mic()

    log("=== STEP 1: 装 vollminputd 包 ===")
    pkgs = list(OUTPUT_DIR.glob(f"{PKG_NAME}-*.pkg.tar.zst"))
    if not pkgs:
        log(f"!! {OUTPUT_DIR} 没有 {PKG_NAME}-*.pkg.tar.zst（第一个测试用例没跑？）")
        return 1
    proc = subprocess.run(
        ["pacman", "-U", "--noconfirm", str(pkgs[0])],
        capture_output=True, text=True,
    )
    log(f"pacman -U rc={proc.returncode}")
    if proc.returncode != 0:
        sys.stderr.write(proc.stdout)
        sys.stderr.write(proc.stderr)
        return proc.returncode
    r = subprocess.run(["which", "vollminputd"], capture_output=True, text=True)
    if r.returncode != 0:
        log("!! vollminputd 不在 PATH")
        return 1
    log(f"vollminputd: {r.stdout.strip()}")

    procs = []

    try:
        log("=== STEP 2: 起 PipeWire 用户实例 ===")
        # pipewire 会自动读取 ~/.config/pipewire/pipewire.conf.d/
        procs.append(run_bg(["pipewire"]))
        procs.append(run_bg(["wireplumber"]))
        time.sleep(3)

        if not wait_for_fake_mic(timeout_sec=15):
            log("!! 15s 内没找到 fake-mic")
            r = subprocess.run(["pw-cli", "ls", "Node"], capture_output=True, text=True)
            log(f"pw-cli ls Node 实际输出: {r.stdout[:500]}")
            return 1

        # 设 fake-mic.monitor 为默认 source
        set_default_source()
        time.sleep(1)

        log("=== STEP 3: 起 weston headless ===")
        os.environ["XDG_RUNTIME_DIR"] = str(XDG_RUNTIME_DIR)
        os.environ["WAYLAND_DISPLAY"] = WESTON_SOCKET
        procs.append(run_bg([
            "weston", "--backend=headless",
            f"--socket={WESTON_SOCKET}", "--use-pixman",
        ]))
        time.sleep(2)

        log("=== STEP 4: 起 vollminputd ===")
        api_key = KEY_FILE.read_text().strip()
        os.environ["VOLLMINPUTD_DASHSCOPE_API_KEY"] = api_key
        DAEMON_LOG.write_text("")
        with DAEMON_LOG.open("w") as f:
            procs.append(subprocess.Popen(
                ["vollminputd", "--instance", INSTANCE],
                stdout=f, stderr=subprocess.STDOUT,
                env=os.environ.copy(),
            ))
        time.sleep(3)
        log(f"daemon 启动日志:\n{DAEMON_LOG.read_text()}")

        log("=== STEP 5: 播放测试音频进 fake-mic ===")
        procs.append(subprocess.Popen(
            ["pw-play", "--target=fake-mic", str(AUDIO)],
            env=os.environ.copy(),
        ))
        time.sleep(1)

        log("=== STEP 6: 写 TOGGLE 到 FIFO ===")
        if not FIFO_PATH.exists():
            log(f"!! daemon 没建 FIFO {FIFO_PATH}")
            return 1
        FIFO_PATH.write_text("TOGGLE\n")
        time.sleep(5)

        log("=== STEP 7: 等 ASR 完成（最长 30s）===")
        recognized = None
        for i in range(30):
            content = DAEMON_LOG.read_text()
            if "ASR 识别成功" in content:
                for line in content.splitlines():
                    if "ASR 识别成功" in line:
                        recognized = line
                        log(f"ASR 成功（{i+1}s）: {recognized}")
                break
            if "识别失败" in content or "未检测到语音" in content:
                log(f"ASR 失败（{i+1}s）: {content.splitlines()[-3:]}")
                break
            time.sleep(1)
        else:
            log("ASR 超时 30s")

        log("=== STEP 8: 读剪贴板断言 ===")
        r = subprocess.run(
            ["wl-paste"], capture_output=True, text=True, env=os.environ,
        )
        clipboard = r.stdout.strip()
        log(f"剪贴板: {clipboard!r}")

        log("=== STEP 9: 检查 daemon 退出日志 ===")
        log(DAEMON_LOG.read_text()[-2000:])

        if not clipboard:
            log("FAIL: 剪贴板为空")
            return 1
        log(f"PASS: 剪贴板 = {clipboard!r}")
        return 0

    finally:
        cleanup(procs)


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as e:
        log(f"!! 未捕获异常: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)
