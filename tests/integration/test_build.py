"""
测试用例 01：验证测试镜像与 makepkg 产物。

实际构建在 conftest.py 的 session-scope fixtures 里完成，
这里只断言镜像 ID 与软件包产物有效。
"""
from pathlib import Path


def test_image_ready(test_image: str):
    """测试镜像必须由 podman 成功构建并返回内容 ID。"""
    assert test_image


def test_package_exists(built_package: Path):
    """构建产物必须存在且大于 1MB。"""
    assert built_package.exists(), f"产物不存在: {built_package}"
    size = built_package.stat().st_size
    assert size > 1_000_000, f"产物太小 ({size} bytes)，可能构建不完整"
