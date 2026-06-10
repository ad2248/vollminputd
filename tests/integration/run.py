#!/usr/bin/env python3
"""
host 编排脚本：逐个拉起容器、每个容器跑一个 Python 测试脚本。

用法：
    python3 run.py                    # 跑 scripts/ 下所有测试
    python3 run.py scripts/01_build_package.py  # 只跑某个测试
"""
import os
import shutil
import subprocess
import sys
import tarfile
import time
from pathlib import Path

INTEGRATION_DIR = Path(__file__).resolve().parent
# run.py 在 tests/integration/ 下；仓根是 tests/integration 的父目录的父目录
REPO_ROOT = INTEGRATION_DIR.parent.parent
BUILD_OUT = INTEGRATION_DIR / "build-out"
PROXY_HTTP = "http://100.64.0.5:7912"
KEY_FILE = INTEGRATION_DIR / ".env.test"
SCRIPTS_DIR = INTEGRATION_DIR / "scripts"
IMAGE_NAME = "vollminputd-itest"
PROXY_ENV_KEYS = ("http_proxy", "https_proxy", "all_proxy",
                  "HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY")


def log(msg):
    print(f"[run.py] {msg}", flush=True)


def build_image() -> str:
    """（幂等）构建测试镜像。build 时代理走 box:7912。"""
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


def pack_source_tarball() -> Path:
    """把 host 仓库存成 tarball，挂在容器内 /src.tar.gz。
    顶层是 vollminputd/（makepkg 期望）。
    必须含 .git（pkgver() 调 git describe）。"""
    BUILD_OUT.mkdir(parents=True, exist_ok=True)
    tarball = BUILD_OUT / "src.tar.gz"
    if tarball.exists() and (time.time() - tarball.stat().st_mtime < 60):
        log(f"源码 tarball 已存在: {tarball.name}")
        return tarball
    log(f"打包源码: {tarball.name} ...")
    # 先跳过大目录再 walk，避免 rglob 遍历 target/ 等成千上万文件
    # .git 必须保留完整（pkgver() 需要 git describe）
    skip_dirs = {"target", "tests/integration/build-out"}
    with tarfile.open(tarball, "w:gz") as tf:
        for root, dirs, files in os.walk(REPO_ROOT):
            root_path = Path(root)
            rel_root = root_path.relative_to(REPO_ROOT)
            # 原地修改 dirs 跳过子目录（os.walk 特性）
            dirs[:] = [d for d in dirs if str(rel_root / d) not in skip_dirs and d not in skip_dirs]
            for f in files:
                p = root_path / f
                rel = p.relative_to(REPO_ROOT)
                arcname = str(Path("vollminputd") / rel)
                tf.add(p, arcname=arcname)
    log(f"tarball 大小: {tarball.stat().st_size} bytes")
    return tarball


def run_one_script(script_path: Path, image: str, src_tar: Path) -> int:
    """起一次性容器跑一个 Python 测试脚本。返回容器退出码。"""
    if not script_path.is_file():
        log(f"跳过: {script_path} 不是文件")
        return 0
    if script_path.suffix != ".py":
        return 0

    # 把 scripts/ 挂进容器 /tests/scripts/
    # PKGBUILD 挂到 /build-out/PKGBUILD
    # src tarball 挂到 /src.tar.gz
    # key 挂到 /run/secrets/key
    # 仓内 tests/ 挂到 /tests/repo-tests/（脚本 02 需要 test_audio.wav）
    log(f"━━━ 容器内执行: {script_path.relative_to(INTEGRATION_DIR) if script_path.is_absolute() else script_path} ━━━")
    # 姐姐 2026-06-09 拍板：build + 容器内运行时都走 box:7912 代理
    # （治 nested podman slirp4netns 拉 crates.io 卡 12 分钟 futex_wait）
    proxy_env_args = []
    for k in PROXY_ENV_KEYS:
        proxy_env_args += ["-e", f"{k}={PROXY_HTTP}"]
    # 容器内程序需要直连的本机（如果有）走 no_proxy
    proxy_env_args += [
        "-e", "no_proxy=127.0.0.1,localhost,::1",
        "-e", "NO_PROXY=127.0.0.1,localhost,::1",
    ]
    # 姐姐 2026-06-09 铁律：不用 host cargo registry bind mount
    # 原因：rootless podman subuid 映射导致容器内 builder (uid 1000)
    #       对 host 目录没有写权限，cargo fetch 报 Permission denied
    # 治法：让容器内自己下载，编译一次后容器内也有缓存
    # 实测 2026-06-10：cpal 0.18 + pipewire 1.6.6 + clang 22 编译通过，无需 patch
    cargo_mount_args = []
    proc = subprocess.run(
        [
            "podman", "run", "--rm",
            *proxy_env_args,
            *cargo_mount_args,
            "-v", f"{SCRIPTS_DIR}:/tests/scripts:ro",
            "-v", f"{INTEGRATION_DIR / 'PKGBUILD'}:/build-out/PKGBUILD:ro",
            "-v", f"{src_tar}:/src.tar.gz:ro",
            "-v", f"{KEY_FILE}:/run/secrets/key:ro",
            "-v", f"{REPO_ROOT / 'tests'}:/tests/repo-tests:ro",
            image,
            "python3", f"/tests/scripts/{script_path.name}",
        ],
        capture_output=True, text=True, timeout=1800,
    )
    # 把容器输出展示出来（包含脚本内的 print）
    if proc.stdout:
        print(proc.stdout, end="")
    if proc.stderr:
        print(proc.stderr, end="", file=sys.stderr)
    log(f"━━━ 退出码: {proc.returncode} ━━━")
    return proc.returncode


def main() -> int:
    if not KEY_FILE.exists():
        log(f"!! 缺 {KEY_FILE}，从 .env.test.example 复制并填入真 key")
        return 1

    image = build_image()
    src_tar = pack_source_tarball()

    # 决定跑哪些脚本
    if len(sys.argv) > 1:
        scripts = [Path(a) for a in sys.argv[1:]]
    else:
        scripts = sorted(SCRIPTS_DIR.glob("*.py"))

    log(f"待跑脚本: {[s.name for s in scripts]}")

    # 逐个跑（前一个失败就停）
    for s in scripts:
        rc = run_one_script(s, image, src_tar)
        if rc != 0:
            log(f"✗ {s.name} 失败 (rc={rc})")
            return rc
        log(f"✓ {s.name} 通过")

    log("全部测试通过 ✓")
    return 0


if __name__ == "__main__":
    sys.exit(main())
