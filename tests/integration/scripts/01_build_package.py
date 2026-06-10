"""
第一个测试用例：在容器内用 makepkg 构建 .pkg.tar.zst。
输出产物在 /build-out/，host 编排脚本会把 build-out/ 整个 bind 给后续测试容器。
"""
import shutil
import subprocess
import sys
import time
from pathlib import Path

PKG_NAME = "vollminputd-git"
BUILD_DIR = Path("/tmp/build")
OUTPUT_DIR = Path("/build-out")
SRC_TARBALL = Path("/src.tar.gz")
PKGBUILD_PATH = OUTPUT_DIR / "PKGBUILD"

# 姐姐需要的 Cargo 镜像配置内容
config_content = """[source.crates-io]
replace-with = 'rsproxy-sparse'

[source.rsproxy-sparse]
registry = "sparse+https://rsproxy.cn/index/"

[registries.rsproxy]
index = "https://rsproxy.cn/crates.io-index"

[net]
git-fetch-with-cli = true
"""

def write_cargo_config():
    # 获取 ~/.cargo 目录和 config.toml 文件的绝对路径
    cargo_dir = Path.home() / ".cargo"
    config_file = cargo_dir / "config.toml"

    # 确保 ~/.cargo 目录存在 (相当于 mkdir -p)
    cargo_dir.mkdir(parents=True, exist_ok=True)

    # 将配置内容写入文件 (会覆盖已有内容喵)
    config_file.write_text(config_content, encoding="utf-8")
    
    print(f"喵！配置已经成功写入到：{config_file}")


def log(msg):
    print(f"[01_build_package] {msg}", flush=True)


def run(cmd, **kw):
    log(f"$ {' '.join(str(c) for c in cmd)}")
    return subprocess.run(cmd, **kw)


def main() -> int:
    write_cargo_config()
    
    if not SRC_TARBALL.exists():
        log(f"!! 找不到 {SRC_TARBALL}")
        return 1
    if not PKGBUILD_PATH.exists():
        log(f"!! 找不到 {PKGBUILD_PATH}")
        return 1

    # 清空旧产物
    if OUTPUT_DIR.exists():
        for p in OUTPUT_DIR.glob("*.pkg.tar.zst"):
            p.unlink()
        for p in OUTPUT_DIR.glob("cargo-target"):
            if p.is_dir():
                shutil.rmtree(p)

    # 准备 makepkg 工作目录
    if BUILD_DIR.exists():
        shutil.rmtree(BUILD_DIR)
    BUILD_DIR.mkdir(parents=True)
    (BUILD_DIR / "vollminputd.tar.gz").write_bytes(SRC_TARBALL.read_bytes())
    shutil.copy(PKGBUILD_PATH, BUILD_DIR / "PKGBUILD")
    subprocess.run(["chown", "-R", "builder:builder", str(BUILD_DIR)], check=True)

    # ★ makepkg 必须以 builder 跑（拒绝 root）
    # sudo 配了 NOPASSWD，可行；su 在 podman 容器里 shadow 不可写
    log("开始 makepkg（容器内首次 cargo build，可能 5-10 分钟）...")
    t0 = time.time()
    proc = run(
        ["sudo", "-u", "builder", "makepkg"],
        cwd=BUILD_DIR,
        capture_output=True,
        text=True,
        timeout=1800,
    )
    elapsed = time.time() - t0
    log(f"makepkg 用时 {elapsed:.0f}s，rc={proc.returncode}")
    if proc.returncode != 0:
        sys.stderr.write("--- makepkg stdout ---\n")
        sys.stderr.write(proc.stdout)
        sys.stderr.write("--- makepkg stderr ---\n")
        sys.stderr.write(proc.stderr)
        return proc.returncode

    # 找产物
    pkgs = list(BUILD_DIR.glob(f"{PKG_NAME}-*.pkg.tar.zst"))
    if not pkgs:
        log(f"!! 没找到 {PKG_NAME}-*.pkg.tar.zst")
        return 1
    log(f"找到产物: {pkgs[0].name}")

    # 把产物和 PKGBUILD 一起放到 /build-out
    shutil.copy(pkgs[0], OUTPUT_DIR / pkgs[0].name)
    log(f"产物复制到 {OUTPUT_DIR / pkgs[0].name}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
