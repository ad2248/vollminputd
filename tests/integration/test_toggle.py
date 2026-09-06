"""
测试用例 02：TOGGLE happy path E2E。

每条用例在独立容器内跑 scripts/02_toggle_happy_path.py：
    pacman -U 安装 /build-out/*.pkg.tar.zst → PipeWire + wireplumber + pw-loopback
    虚拟麦克风 → sway headless → vollminputd → FIFO TOGGLE ×2 夹一段 pw-play 播放
    → 断言剪贴板（host 的 conftest 负责镜像/tarball/产物 fixture 与 --live 收集过滤）。

两条用例：
    - offline（默认总是跑，2 轮）：容器内 mock_asr.py 按原生 DashScope HTTP 协议应答，
      并校验路径/鉴权/SSE 头/model/参数与 WAV 音频非空非静音；剪贴板精确等于
      「容器语音测试通过」。
    - live（@pytest.mark.live，由 conftest 的 --live 过滤控制，1 轮）：真打云端默认
      端点（key 由 host conftest 经容器环境变量注入）；断言非空且剪贴板等于本轮识别文本。
"""
from pathlib import Path

import pytest

from conftest import run_test_container

SCRIPT = Path(__file__).parent / "scripts" / "02_toggle_happy_path.py"


def test_toggle_offline(test_image, src_tarball, built_package):
    """离线 E2E：mock ASR 校验协议+音频，剪贴板 == 容器语音测试通过。"""
    proc = run_test_container(
        test_image, SCRIPT, src_tarball, built_package,
        extra_env={"TEST_LIVE_ASR": "0"},
    )
    assert proc.returncode == 0, f"02_toggle_happy_path.py 失败 (rc={proc.returncode})"


@pytest.mark.live
def test_toggle_live(test_image, src_tarball, built_package):
    """live E2E：真打云端默认端点；跳过与否由 conftest 的 --live 收集过滤管理，
    脚本内对缺失 key 直接 FAIL，绝不静默跳过。"""
    proc = run_test_container(
        test_image, SCRIPT, src_tarball, built_package,
        extra_env={"TEST_LIVE_ASR": "1"},
    )
    assert proc.returncode == 0, f"02_toggle_happy_path.py 失败 (live, rc={proc.returncode})"
