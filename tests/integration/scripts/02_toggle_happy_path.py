#!/usr/bin/env python3
"""
TOGGLE happy path E2E（容器内、root、由 host 在 dbus-run-session 下启动）。
挂载：/build-out（pkg.tar.zst）、/tests/repo-tests/test_audio.wav、/tests/scripts:ro、
/artifacts（可写；收尾保留全部组件日志，key 脱敏，保存失败按失败处理）。
环境：TEST_INSTANCE、TEST_LIVE_ASR（1=live）、VOLLMINPUTD_ASR_MODEL（默认
qwen-audio-3.0-asr-flash）；TEST_LIVE_ASR=1 时必须有 VOLLMINPUTD_DASHSCOPE_API_KEY
（host 注入，脚本不读 key 文件）。离线：容器内起 mock_asr.py（HTTP :18765，原生
DashScope 协议），daemon 经 VOLLMINPUTD_ASR_ENDPOINT 指向 mock；mock 校验路径/鉴权/
SSE 头/model/参数与音频，任何失败请求 → 本轮失败；live 不碰 endpoint（用 daemon 默认
端点或宿主透传的 VOLLMINPUTD_ASR_ENDPOINT）。每轮：清剪贴板 → TOGGLE → 等录音就绪 →
pw-play 播 WAV → TOGGLE → 等识别结果 + 「新状态: Idle」→ 剪贴板 == daemon 识别文本
（离线/live 同样精确断言）。离线固定 2 轮，live 1 轮。全程轮询就绪，无固定 sleep。
"""
import errno
import json
import os
import re
import signal
import socket
import subprocess
import sys
import time
from pathlib import Path

INSTANCE = os.environ.get("TEST_INSTANCE", "itest")
FIFO = Path(f"/tmp/vollminputd_{INSTANCE}.fifo")
AUDIO = Path("/tests/repo-tests/test_audio.wav")
SCRIPTS = Path("/tests/scripts")
LOGS = Path("/tmp/e2e-logs")
ARTIFACTS = Path("/artifacts")
XDG = Path("/run/user/0")
DAEMON_LOG = LOGS / "daemon.log"
MOCK_REQ = LOGS / "mock_requests.jsonl"

LIVE = os.environ.get("TEST_LIVE_ASR") == "1"
TEXT = "容器语音测试通过"
MODEL = os.environ.get("VOLLMINPUTD_ASR_MODEL", "qwen-audio-3.0-asr-flash")
OFFLINE_KEY = "itest-offline-key"
ROUNDS = 1 if LIVE else 2
MOCK_PORT = 18765
MOCK_PATH = "/api/v1/services/aigc/multimodal-generation/generation"
OFFLINE_ENDPOINT = f"http://127.0.0.1:{MOCK_PORT}{MOCK_PATH}"
FAIL_MARKS = ("ASR 识别失败", "ASR 返回空结果")

T = dict(pacman=120, mock=15, pw=25, mic=20, src=15, wayland=25, daemon=20,
         fifo=15, record=20, play=60, idle=10, paste=15, tool=10,
         asr=150 if LIVE else 90)

PROCS = []
DAEMON = None
_pos = 0        # daemon.log 消费游标（字符）
mock_seen = 0   # mock_requests.jsonl 已消费行数


def log(m):
    print(f"[02_toggle] {m}", flush=True)


class StepFailure(Exception):
    pass


def run(cmd, timeout, desc):
    try:
        return subprocess.run(cmd, capture_output=True, text=True,
                              timeout=timeout, env=os.environ.copy())
    except subprocess.TimeoutExpired:
        raise StepFailure(f"超时({timeout}s): {desc}")


def poll(fn, timeout, desc):
    """fn() -> (ok, info)，轮询到 ok 或超时。"""
    deadline, info = time.monotonic() + timeout, ""
    while time.monotonic() < deadline:
        ok, info = fn()
        if ok:
            return info
        time.sleep(0.25)
    raise StepFailure(f"等待超时({timeout}s): {desc}（最后: {info}）")


def spawn(name, cmd):
    log(f"$ {' '.join(map(str, cmd))} (bg → {name}.log)")
    with open(LOGS / f"{name}.log", "ab") as f:
        PROCS.append(subprocess.Popen(cmd, stdout=f, stderr=subprocess.STDOUT,
                                      stdin=subprocess.DEVNULL, start_new_session=True))
    return PROCS[-1]


def stop_all():
    for p in PROCS:
        if p.poll() is not None:
            continue
        try:
            os.killpg(os.getpgid(p.pid), signal.SIGTERM)
            p.wait(timeout=2)
        except (ProcessLookupError, subprocess.TimeoutExpired):
            try:
                os.killpg(os.getpgid(p.pid), signal.SIGKILL)
            except ProcessLookupError:
                pass
            try:
                p.wait(timeout=1)
            except subprocess.TimeoutExpired:
                pass


def fifo_write(line):
    deadline = time.monotonic() + T["fifo"]
    while True:
        try:
            fd = os.open(FIFO, os.O_WRONLY | os.O_NONBLOCK)
            break
        except FileNotFoundError:
            raise StepFailure(f"daemon 没创建 FIFO: {FIFO}")
        except OSError as e:
            if e.errno == errno.ENXIO and time.monotonic() < deadline:
                time.sleep(0.2)
                continue
            raise StepFailure(f"FIFO 打不开 (errno={e.errno}): {FIFO}")
    try:
        os.write(fd, line.encode())
        log(f"FIFO ← {line.strip()!r}")
    except BrokenPipeError as e:
        raise StepFailure(f"FIFO 写入失败（读者消失？daemon 挂了？）: {e}")
    finally:
        os.close(fd)


def dtext():
    return DAEMON_LOG.read_text(errors="replace") if DAEMON_LOG.exists() else ""


def wait_log(pattern, timeout, flags=0, fail_marks=()):
    """轮询 daemon 日志新出现的内容；fail_marks 命中即判负；daemon 退出即判负。"""
    global _pos
    rx = re.compile(pattern, flags)
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if DAEMON is not None and DAEMON.poll() is not None:
            raise StepFailure(f"daemon 提前退出 rc={DAEMON.returncode}\n{dtext()[-2500:]}")
        t = dtext()[_pos:]
        if any(m in t for m in fail_marks):
            raise StepFailure(f"识别失败（命中 {fail_marks}）\n{dtext()[-2500:]}")
        m = rx.search(t)
        if m:
            _pos += m.end()
            return m
        time.sleep(0.2)
    raise StepFailure(f"等待 daemon 日志超时({timeout}s): {pattern}\n{dtext()[-2500:]}")


def install_package():
    log("STEP 1: pacman -U")
    pkgs = sorted(Path("/build-out").glob("vollminputd-git-*.pkg.tar.zst"))
    if not pkgs:
        raise StepFailure("/build-out 没有 vollminputd-git-*.pkg.tar.zst")
    r = run(["pacman", "-U", "--noconfirm", str(pkgs[0])], T["pacman"], "pacman -U")
    if r.returncode != 0:
        raise StepFailure(f"pacman -U 失败\n{r.stdout}{r.stderr}")


def start_mock(key):
    log("STEP 2: mock ASR（原生 DashScope HTTP 协议）")
    os.environ["MOCK_EXPECTED_KEY"] = key
    p = spawn("mock_asr",
              [sys.executable, str(SCRIPTS / "mock_asr.py"),
               "--port", str(MOCK_PORT), "--path", MOCK_PATH,
               "--model", MODEL, "--text", TEXT, "--requests-log", str(MOCK_REQ)])

    def port_ok():
        s = socket.socket()
        try:
            return s.connect_ex(("127.0.0.1", MOCK_PORT)) == 0, MOCK_PORT
        finally:
            s.close()
    poll(port_ok, T["mock"], f"mock 端口 {MOCK_PORT}")
    if p.poll() is not None:
        raise StepFailure(f"mock_asr 提前退出 rc={p.returncode}")
    return OFFLINE_ENDPOINT


def assert_default_source():
    def once():
        r = run(["wpctl", "status", "--name"], T["tool"], "wpctl status --name")
        mid = next((int(m.group(1)) for line in r.stdout.splitlines()
                    if "test-mic" in line
                    for m in [re.search(r"(\d+)\s*\.", line)] if m), None)
        if mid is None:
            return False, "status --name 找不到 test-mic"
        if run(["wpctl", "set-default", str(mid)], T["tool"], "set-default").returncode != 0:
            return False, f"set-default {mid} 失败"
        ins = run(["wpctl", "inspect", "@DEFAULT_AUDIO_SOURCE@"], T["tool"], "wpctl inspect")
        ok = re.search(r'node\.name\s*[=:]\s*"?(test-mic)"', ins.stdout) is not None
        return ok, f"inspect @DEFAULT_AUDIO_SOURCE@ 未断言到 test-mic:\n{ins.stdout[-400:]}"
    poll(once, T["src"], "默认录音源 = test-mic（wpctl inspect 断言）")
    log("默认录音源已断言为 test-mic")


def start_audio():
    log("STEP 3: PipeWire + wireplumber + 虚拟麦")
    spawn("pipewire", ["pipewire"])
    spawn("wireplumber", ["wireplumber"])
    poll(lambda: (run(["wpctl", "status", "--name"], T["tool"], "wpctl").returncode == 0, ""),
         T["pw"], "PipeWire/wireplumber 就绪")
    spawn("pw-loopback",
          ["pw-loopback",
           "--capture-props=media.class=Audio/Sink node.name=test-sink",
           "--playback-props=media.class=Audio/Source node.name=test-mic"])

    def nodes_ready():
        r = run(["pw-cli", "ls", "Node"], T["tool"], "pw-cli ls Node")
        ok = r.returncode == 0 and "test-mic" in r.stdout and "test-sink" in r.stdout
        return ok, r.stdout[-200:]
    poll(nodes_ready, T["mic"], "test-mic/test-sink 节点出现")
    assert_default_source()


def start_compositor():
    """Sway provides a headless seat for the real wl-clipboard clients."""
    log("STEP 4: sway headless")
    os.environ.update(WLR_BACKENDS="headless", WLR_LIBINPUT_NO_DEVICES="1",
                      WLR_HEADLESS_OUTPUTS="1")
    os.environ.pop("WAYLAND_DISPLAY", None)
    (LOGS / "sway-config").write_text("seat seat0 fallback true\n")
    spawn("sway", ["sway", "-c", str(LOGS / "sway-config")])

    def ready():
        found = sorted(s for s in XDG.glob("wayland-*") if s.is_socket())
        if found:
            os.environ["WAYLAND_DISPLAY"] = found[0].name
            return True, found[0].name
        return False, "XDG_RUNTIME_DIR 下无 wayland-* socket"
    poll(ready, T["wayland"], "wayland socket 可连接")
    log(f"wayland 就绪: WAYLAND_DISPLAY={os.environ['WAYLAND_DISPLAY']}")


def start_daemon(key, endpoint):
    global DAEMON
    log(f"STEP 5: vollminputd live={LIVE} model={MODEL}")
    env = os.environ.copy()
    env.update(VOLLMINPUTD_DASHSCOPE_API_KEY=key, VOLLMINPUTD_ASR_MODEL=MODEL)
    if not LIVE:
        env["VOLLMINPUTD_ASR_ENDPOINT"] = endpoint
    DAEMON_LOG.write_text("")
    with open(DAEMON_LOG, "ab") as f:
        DAEMON = subprocess.Popen(["vollminputd", "--instance", INSTANCE],
                                  stdout=f, stderr=subprocess.STDOUT,
                                  stdin=subprocess.DEVNULL,
                                  start_new_session=True, env=env)
    PROCS.append(DAEMON)
    wait_log(r"程序就绪", T["daemon"])
    log(f"daemon 就绪，FIFO: {FIFO}")


def paste():
    r = run(["wl-paste"], T["tool"], "wl-paste")
    return r.stdout if r.returncode == 0 else ""


def wait_clipboard(expect):
    def once():
        got = paste()
        return got.strip() == expect, f"剪贴板={got.strip()!r}"
    poll(once, T["paste"], f"剪贴板 == {expect!r}")


def mock_records():
    if not MOCK_REQ.exists():
        return []
    return [json.loads(l) for l in MOCK_REQ.read_text().splitlines() if l.strip()]


def check_mock_new_records():
    global mock_seen
    recs = mock_records()
    new, mock_seen = recs[mock_seen:], len(recs)
    if bad := [r for r in new if not r["ok"]]:
        raise StepFailure(f"mock 收到失败请求: {bad}")
    if not new:
        raise StepFailure("mock 未收到本轮请求（剪贴板文本来源可疑）")
    log(f"mock 校验通过 (bytes={new[0]['audio_bytes']}, peak={new[0]['peak']})")


def do_round(idx):
    log(f"━━━ 第 {idx}/{ROUNDS} 轮 ━━━")
    r = run(["wl-copy", "--clear"], T["tool"], "wl-copy --clear")
    if r.returncode != 0:
        raise StepFailure(f"wl-copy --clear 失败 rc={r.returncode}: {r.stdout}{r.stderr}")
    if paste().strip():
        raise StepFailure("剪贴板未清空")
    fifo_write("TOGGLE\n")
    wait_log(r"副作用: 启动音频采集", T["record"])
    log("录音就绪，播放测试音频")
    r = run(["pw-play", "--target=test-sink", str(AUDIO)], T["play"], "pw-play")
    if r.returncode != 0:
        raise StepFailure(f"pw-play rc={r.returncode}\n{r.stdout}{r.stderr}")
    log("播放完毕，第二次 TOGGLE")
    fifo_write("TOGGLE\n")
    text = wait_log(r"ASR 识别成功: '(.*?)'\n", T["asr"], re.S, FAIL_MARKS).group(1)
    if not text.strip() or (not LIVE and text.strip() != TEXT):
        raise StepFailure(f"识别文本不符合预期: {text!r}")
    log(f"ASR 识别成功: {text!r}")
    wait_log(r"事件处理完成，新状态: Idle", T["idle"])   # 确保 wl-copy 已执行
    wait_clipboard(text.strip())
    log("剪贴板 == 识别文本 ✓")
    if not LIVE:
        check_mock_new_records()


def save_logs(key):
    """组件日志保留到 /artifacts；key 脱敏。异常向上抛（不静默吞）。"""
    key_b = key.encode()
    for f in LOGS.iterdir():
        data = f.read_bytes()
        if len(key) >= 8 and key_b in data:
            data = data.replace(key_b, b"***REDACTED***")
        (ARTIFACTS / f.name).write_bytes(data)
    log(f"日志已保留到 {ARTIFACTS}")


def main():
    XDG.mkdir(parents=True, exist_ok=True)   # 必须先于音频/wayland
    os.chmod(XDG, 0o700)
    os.environ["XDG_RUNTIME_DIR"] = str(XDG)
    LOGS.mkdir(parents=True, exist_ok=True)
    log(f"instance={INSTANCE} live={LIVE} rounds={ROUNDS} model={MODEL}")
    key = os.environ.get("VOLLMINPUTD_DASHSCOPE_API_KEY", "")
    if LIVE and not key:
        log("!! FAIL: live 需要容器环境 VOLLMINPUTD_DASHSCOPE_API_KEY"
            "（conftest 注入），当前为空")
        return 1
    key = key or OFFLINE_KEY
    if not AUDIO.exists() or AUDIO.stat().st_size == 0:
        log(f"!! 缺测试音频 {AUDIO}")
        return 1

    err, rc = "", 1
    try:
        install_package()
        start_audio()
        start_compositor()
        start_daemon(key, "" if LIVE else start_mock(key))
        for i in range(1, ROUNDS + 1):
            do_round(i)
        if not LIVE and (bad := [r for r in mock_records() if not r["ok"]]):
            raise StepFailure(f"收尾检查: mock 存在失败请求 {bad}")
        rc = 0
        log(f"━━━ PASS live={LIVE} ×{ROUNDS} 轮 ━━━")
    except StepFailure as e:
        err = str(e)
    except Exception as e:
        import traceback
        traceback.print_exc()
        err = repr(e)
    finally:
        stop_all()
        if err:
            log(f"━━━ FAIL ━━━\n{err}\n--- daemon 日志尾部 ---\n{dtext()[-4000:]}")
        try:
            save_logs(key)
        except Exception as e:
            log(f"!! 保存日志到 {ARTIFACTS} 失败: {e}")
            rc = 1
    return rc


if __name__ == "__main__":
    sys.exit(main())
