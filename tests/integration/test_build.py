"""
测试用例 01：验证 makepkg 能成功构建 .pkg.tar.zst。

实际构建在 conftest.py 的 session-scope fixture `built_package` 里完成，
这里只是断言产物存在且非空。
"""
from pathlib import Path


def test_package_exists(built_package: Path):
    """构建产物必须存在且大于 1MB。"""
    assert built_package.exists(), f"产物不存在: {built_package}"
    size = built_package.stat().st_size
    assert size > 1_000_000, f"产物太小 ({size} bytes)，可能构建不完整"
