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
            ('vollminputd/tests/integration/PKGBUILD', 'PKGBUILD'),
        ):
            (build / destination).write_bytes(archive.extractfile(member).read())
    subprocess.run(['chown', '-R', 'builder:builder', str(build)], check=True)
    # Pass only networking settings, never a live API key, to the build user.
    env_args = [f'{key}={value}' for key, value in os.environ.items()
                if key.lower() in ('http_proxy', 'https_proxy', 'all_proxy', 'no_proxy')]
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
