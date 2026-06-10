"""
Pytest 配置 + session-scope fixture。

- 构建测试镜像（幂等）
- 打包源码 tarball（session 级缓存）
- 在容器内 makepkg 构建 .pkg.tar.zst（session 级缓存）
- 每个测试函数起独立 --rm 容器
"""
import os
import shutil
import subprocess
import sys
import tarfile
import time
import uuid
from pathlib import Path

import pytest

INTEGRATION_DIR = Path(__file__).resolve().parent
# conftest.py 在 tests/integration/ 下；仓根是两级的父目录
REPO_ROOT = INTEGRATION_DIR.parent.parent
BUILD_OUT = INTEGRATION_DIR / "build-out"
KEY_FILE = INTEGRATION_DIR / ".env.test"
SCRIPTS_DIR = INTEGRATION_DIR / "scripts"
IMAGE_NAME = "vollminputd-itest"
PROXY_HTTP = "http://100.64.0.5:7912"
PROXY_ENV_KEYS = (
    "http_proxy", "https_proxy", "all_proxy",
    "HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY",
)


def log(msg):
    print(f"[conftest] {msg}", flush=True)


# ---------------------------------------------------------------------------
# Session-scope helpers
# ---------------------------------------------------------------------------

@pytest.fixture(scope="session")
def test_image():
    """（幂等）构建测试镜像。"""
    if _image_exists(IMAGE_NAME):
        log(f"镜像已存在: {IMAGE_NAME}")
        return IMAGE_NAME
    log(f"构建镜像: {IMAGE_NAME} ...")
    env = {**os.environ, **{k: PROXY_HTTP for k in PROXY_ENV_KEYS}}
    proc = subprocess.run(
        ["podman", "build", "-t", IMAGE_NAME,
         "-f", str(INTEGRATION_DIR / "Containerfile"),
         str(INTEGRATION_DIR)],
        env=env, capture_output=True, text=True, timeout=1800,
    )
    if proc.returncode != 0:
        sys.stderr.write(proc.stdout)
        sys.stderr.write(proc.stderr)
        raise RuntimeError(f"镜像构建失败 (rc={proc.returncode})")
    log(f"镜像就绪: {IMAGE_NAME}")
    return IMAGE_NAME


def _image_exists(name: str) -> bool:
    proc = subprocess.run(
        ["podman", "image", "exists", name],
        capture_output=True, text=True,
    )
    return proc.returncode == 0


@pytest.fixture(scope="session")
def src_tarball() -> Path:
    """把 host 仓库存成 tarball，挂在容器内 /src.tar.gz。"""
    BUILD_OUT.mkdir(parents=True, exist_ok=True)
    tarball = BUILD_OUT / "src.tar.gz"
    if tarball.exists() and (time.time() - tarball.stat().st_mtime < 60):
        log(f"源码 tarball 已存在: {tarball.name}")
        return tarball
    log(f"打包源码: {tarball.name} ...")
    skip_dirs = {"target", "tests/integration/build-out"}
    with tarfile.open(tarball, "w:gz") as tf:
        for root, dirs, files in os.walk(REPO_ROOT):
            root_path = Path(root)
            rel_root = root_path.relative_to(REPO_ROOT)
            dirs[:] = [
                d for d in dirs
                if str(rel_root / d) not in skip_dirs and d not in skip_dirs
            ]
            for f in files:
                p = root_path / f
                rel = p.relative_to(REPO_ROOT)
                arcname = str(Path("vollminputd") / rel)
                tf.add(p, arcname=arcname)
    log(f"tarball 大小: {tarball.stat().st_size} bytes")
    return tarball


@pytest.fixture(scope="session")
def built_package(test_image, src_tarball) -> Path:
    """
    在容器内 makepkg 构建一次，产物缓存在 build-out/。
    返回 .pkg.tar.zst 的 Path。
    """
    BUILD_OUT.mkdir(parents=True, exist_ok=True)
    # 先看是否已有产物（60 秒内新鲜）
    existing = list(BUILD_OUT.glob("vollminputd-git-*.pkg.tar.zst"))
    if existing and (time.time() - existing[0].stat().st_mtime < 3600):
        log(f"使用已有产物: {existing[0].name}")
        return existing[0]

    log("━━━ session build: 容器内 makepkg ━━━")
    proxy_env_args = []
    for k in PROXY_ENV_KEYS:
        proxy_env_args += ["-e", f"{k}={PROXY_HTTP}"]
    proxy_env_args += [
        "-e", "no_proxy=127.0.0.1,localhost,::1",
        "-e", "NO_PROXY=127.0.0.1,localhost,::1",
    ]

    host_cargo_registry = Path.home() / ".cargo" / "registry"
    cargo_mount_args = []
    if host_cargo_registry.exists():
        # :ro = 只读，避免 overlay 复制占用空间
        cargo_mount_args = [
            "-v", f"{host_cargo_registry}:/home/builder/.cargo/registry:ro",
        ]

    proc = subprocess.run(
        [
            "podman", "run", "--rm", "--userns=keep-id",
            *proxy_env_args,
            *cargo_mount_args,
            "-v", f"{SCRIPTS_DIR}:/tests/scripts:ro",
            "-v", f"{INTEGRATION_DIR / 'PKGBUILD'}:/build-out/PKGBUILD:ro",
            "-v", f"{src_tarball}:/src.tar.gz:ro",
            "-v", f"{BUILD_OUT}:/build-out",
            test_image,
            "python3", "/tests/scripts/01_build_package.py",
        ],
        capture_output=True, text=True, timeout=1800,
    )
    if proc.stdout:
        print(proc.stdout, end="")
    if proc.stderr:
        print(proc.stderr, end="", file=sys.stderr)
    if proc.returncode != 0:
        raise RuntimeError(f"makepkg 构建失败 (rc={proc.returncode})")

    pkgs = list(BUILD_OUT.glob("vollminputd-git-*.pkg.tar.zst"))
    if not pkgs:
        raise RuntimeError("makepkg 后找不到 .pkg.tar.zst")
    log(f"产物就绪: {pkgs[0].name}")
    return pkgs[0]


# ---------------------------------------------------------------------------
# Per-function helper
# ---------------------------------------------------------------------------

def run_test_container(
    test_image: str,
    script_path: Path,
    src_tarball: Path,
    built_package: Path,
    extra_env: dict | None = None,
) -> subprocess.CompletedProcess:
    """
    起一次性容器跑一个 Python 测试脚本。
    返回 CompletedProcess（含 stdout/stderr/returncode）。
    """
    if not KEY_FILE.exists():
        raise RuntimeError(f"缺 {KEY_FILE}，请从 .env.test.example 复制并填入真 key")

    proxy_env_args = []
    for k in PROXY_ENV_KEYS:
        proxy_env_args += ["-e", f"{k}={PROXY_HTTP}"]
    proxy_env_args += [
        "-e", "no_proxy=127.0.0.1,localhost,::1",
        "-e", "NO_PROXY=127.0.0.1,localhost,::1",
    ]

    # 额外环境变量
    extra_env_args = []
    if extra_env:
        for k, v in extra_env.items():
            extra_env_args += ["-e", f"{k}={v}"]

    # 生成唯一 instance 名（并发隔离）
    instance = f"pytest-{uuid.uuid4().hex[:8]}"
    extra_env_args += ["-e", f"TEST_INSTANCE={instance}"]

    cmd = [
        "podman", "run", "--rm",
        *proxy_env_args,
        *extra_env_args,
        "-v", f"{SCRIPTS_DIR}:/tests/scripts:ro",
        "-v", f"{INTEGRATION_DIR / 'PKGBUILD'}:/build-out/PKGBUILD:ro",
        "-v", f"{src_tarball}:/src.tar.gz:ro",
        "-v", f"{KEY_FILE}:/run/secrets/key:ro",
        "-v", f"{REPO_ROOT / 'tests'}:/tests/repo-tests:ro",
        "-v", f"{built_package.parent}:/build-out:ro",
        test_image,
        "python3", f"/tests/scripts/{script_path.name}",
    ]

    log(f"━━━ 容器内执行: {script_path.name} (instance={instance}) ━━━")
    proc = subprocess.run(cmd, capture_output=True, text=True, timeout=1800)
    if proc.stdout:
        print(proc.stdout, end="")
    if proc.stderr:
        print(proc.stderr, end="", file=sys.stderr)
    log(f"━━━ {script_path.name} 退出码: {proc.returncode} ━━━")
    return proc
