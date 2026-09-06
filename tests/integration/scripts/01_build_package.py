"""Build, test and package the source snapshot using the production recipe."""
import os
from pathlib import Path
import shutil
import subprocess
import tarfile


def main():
    build = Path('/tmp/build')
    build.mkdir()
    source = Path('/src.tar.gz')
    shutil.copyfile(source, build / 'vollminputd.tar.gz')
    with tarfile.open(source) as archive:
        for member, destination in (
            ('vollminputd/PKGBUILD', 'production.PKGBUILD'),
            ('vollminputd/.SRCINFO', 'expected.SRCINFO'),
            ('vollminputd/tests/integration/PKGBUILD', 'PKGBUILD'),
        ):
            (build / destination).write_bytes(archive.extractfile(member).read())
    subprocess.run(['chown', '-R', 'builder:builder', str(build)], check=True)
    srcinfo = subprocess.check_output(
        ['sudo', '-H', '-u', 'builder', 'makepkg', '-p', 'production.PKGBUILD', '--printsrcinfo'],
        cwd=build, timeout=30,
    )
    if srcinfo != (build / 'expected.SRCINFO').read_bytes():
        raise RuntimeError('PKGBUILD and .SRCINFO differ; regenerate .SRCINFO before publishing to AUR')
    # Pass only networking settings, never a live API key, to the build user.
    env_args = [f'{key}={value}' for key, value in os.environ.items()
                if key.lower() in ('http_proxy', 'https_proxy', 'all_proxy', 'no_proxy')]
    # Reproduce a user toolchain shadowing the distro tools, without installing old Rust.
    shadow = Path('/tmp/old-rust-bin')
    shadow.mkdir()
    for name in ('cargo', 'rustc', 'rustdoc'):
        tool = shadow / name
        tool.write_text('#!/bin/sh\n' + f'printf "ERROR: shadowed {name} was used\\n" >&2\nexit 99\n')
        tool.chmod(0o755)
    env_args += [f'PATH={shadow}:/usr/bin', f'RUSTC={shadow}/rustc',
                 f'RUSTDOC={shadow}/rustdoc', 'RUSTUP_TOOLCHAIN=stable']
    subprocess.run(
        ['sudo', '-H', '-u', 'builder', 'env', *env_args, 'makepkg', '--force'],
        cwd=build, check=True, timeout=1800,
    )
    packages = list(build.glob('vollminputd-git-0.0.0.test-*.pkg.tar.zst'))
    if len(packages) != 1:
        raise RuntimeError(f'Expected one package, found {len(packages)}')
    shutil.copyfile(packages[0], Path('/build-out') / packages[0].name)


if __name__ == '__main__':
    main()
