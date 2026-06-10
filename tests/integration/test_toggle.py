"""
测试用例 02：TOGGLE happy path。

在容器内：
1. pacman -U 安装 vollminputd
2. 起 PipeWire + wireplumber + fake-mic null-sink
3. 起 weston headless
4. 起 vollminputd（真打 DashScope）
5. pw-play 喂测试音频进 fake-mic
6. 写 TOGGLE 到 FIFO
7. 等 ASR 完成
8. wl-paste 断言非空
"""
import subprocess
from pathlib import Path

from conftest import run_test_container


def test_toggle_happy_path(test_image, src_tarball, built_package):
    """完整 E2E：音频 → ASR → 剪贴板非空。"""
    script = Path(__file__).parent / "scripts" / "02_toggle_happy_path.py"
    proc = run_test_container(test_image, script, src_tarball, built_package)
    assert proc.returncode == 0, f"02_toggle_happy_path.py 失败 (rc={proc.returncode})"
