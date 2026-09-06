"""Fresh source snapshots, one build per session, isolated scenario containers."""
import hashlib
import json
import os
from pathlib import Path
import shlex
import subprocess
import tarfile
import uuid

import pytest

collect_ignore = ['build-out']

INTEGRATION_DIR = Path(__file__).resolve().parent
REPO_ROOT = INTEGRATION_DIR.parent.parent
BUILD_OUT = INTEGRATION_DIR / 'build-out'
KEY_NAME = 'VOLLMINPUTD_DASHSCOPE_API_KEY'
PROXY_KEYS = ('http_proxy', 'https_proxy', 'all_proxy',
              'HTTP_PROXY', 'HTTPS_PROXY', 'ALL_PROXY')


def load_key(path):
    key = os.environ.get(KEY_NAME, '').strip()
    if not key and path.is_file():
        lines = [line.strip() for line in path.read_text().splitlines()
                 if line.strip() and not line.lstrip().startswith('#')]
        for line in lines:
            if line.startswith('export '):
                line = line[7:]
            if line.startswith(KEY_NAME + '='):
                values = shlex.split(line.split('=', 1)[1], comments=True)
                key = values[0] if len(values) == 1 else ''
                break
        else:
            # Previously supported local key files contain only a bare key.
            if len(lines) == 1 and '=' not in lines[0]:
                key = lines[0]
    if not key or key == 'YOUR_DASHSCOPE_KEY_HERE' or any(c.isspace() for c in key):
        raise pytest.UsageError('Live tests require a valid API key via environment or --key-file.')
    return key


def pytest_addoption(parser):
    parser.addoption('--live', action='store_true', help='Also run the real native HTTP ASR E2E scenario.')
    parser.addoption('--key-file', type=Path, default=INTEGRATION_DIR / '.env.test')


def pytest_configure(config):
    config.addinivalue_line('markers', 'live: calls the real DashScope service')
    config._voiceinput_key_file = config.getoption('--key-file').resolve()
    if not config.option.collectonly:
        config._voiceinput_output = BUILD_OUT / uuid.uuid4().hex
        config._voiceinput_output.mkdir(parents=True)
        if not config.option.xmlpath:
            config.option.xmlpath = str(config._voiceinput_output / 'junit.xml')
        if os.environ.get('GITHUB_OUTPUT'):
            with open(os.environ['GITHUB_OUTPUT'], 'a') as output:
                output.write(f'artifacts={config._voiceinput_output}\n')
    if config.getoption('--live'):
        config._voiceinput_key = load_key(config.getoption('--key-file'))


def pytest_collection_modifyitems(config, items):
    if not config.getoption('--live'):
        live = [item for item in items if item.get_closest_marker('live')]
        items[:] = [item for item in items if not item.get_closest_marker('live')]
        config.hook.pytest_deselected(items=live)


def run_container(image, mounts, command, output, *, network=False, env=None, timeout=1800):
    name = 'vollminputd-test-' + uuid.uuid4().hex
    output.mkdir(parents=True, exist_ok=True)
    child_env = os.environ.copy()
    # Offline/build containers must never inherit the cloud key.
    child_env.pop(KEY_NAME, None)
    child_env.update(env or {})
    cmd = ['podman', 'run', '--rm', '--name', name]
    if not network:
        cmd += ['--network=none', '--http-proxy=false']
    else:
        for key in PROXY_KEYS:
            if key in child_env:
                cmd += ['--env', key]
        bypass = child_env.get('no_proxy', child_env.get('NO_PROXY', ''))
        child_env['no_proxy'] = child_env['NO_PROXY'] = bypass + ',localhost,127.0.0.1,::1'
        cmd += ['--env', 'no_proxy', '--env', 'NO_PROXY']
    for key in env or {}:
        cmd += ['--env', key]
    for host, target, mode in mounts:
        cmd += ['--volume', f'{host}:{target}:{mode}']
    cmd += ['--volume', f'{output}:/artifacts:rw', image, *command]
    try:
        with (output / 'container.log').open('w') as log:
            try:
                proc = subprocess.run(cmd, env=child_env, stdout=log,
                                      stderr=subprocess.STDOUT, text=True, timeout=timeout)
            finally:
                # Killing the podman client alone leaves its container running.
                try:
                    subprocess.run(['podman', 'rm', '--force', '--ignore', name],
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                   timeout=30, check=True)
                except (subprocess.TimeoutExpired, subprocess.CalledProcessError):
                    pytest.fail(f'Could not clean up container {name}; inspect it manually. Logs: {output}')
        text = (output / 'container.log').read_text()
        if KEY_NAME in child_env:
            text = text.replace(child_env[KEY_NAME], '[REDACTED]')
        print(text)
        print(f'Artifacts: {output}')
        return subprocess.CompletedProcess(cmd, proc.returncode, text, '')
    except subprocess.TimeoutExpired:
        pytest.fail(f'Container exceeded {timeout}s; logs: {output}')
    finally:
        if KEY_NAME in child_env:
            for path in output.rglob('*'):
                if path.is_file():
                    data = path.read_bytes()
                    redacted = data.replace(child_env[KEY_NAME].encode(), b'[REDACTED]')
                    if redacted != data:
                        path.write_bytes(redacted)


@pytest.fixture(scope='session')
def test_image(request):
    output = request.config._voiceinput_output
    name = 'localhost/vollminputd-itest'
    # Always ask the builder: its layer cache is keyed by the recipe, not image existence.
    try:
        with (output / 'image-build.log').open('w') as log:
            proc = subprocess.run(
                ['podman', 'build', '-t', name, '--iidfile', str(output / 'image-id'),
                 '-f', str(INTEGRATION_DIR / 'Containerfile'), str(INTEGRATION_DIR)],
                stdout=log, stderr=subprocess.STDOUT, timeout=1800,
            )
    except subprocess.TimeoutExpired:
        pytest.fail(f'Image build timed out; see {output / "image-build.log"}')
    if proc.returncode:
        pytest.fail(f'Image build failed: {output / "image-build.log"}')
    return (output / 'image-id').read_text().strip()


@pytest.fixture(scope='session')
def src_tarball(request):
    output = request.config._voiceinput_output
    snapshot = output / 'source'
    snapshot.mkdir()
    paths = subprocess.check_output(
        ['git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard', '--',
         'Cargo.toml', 'Cargo.lock', 'PKGBUILD', '.SRCINFO', 'LICENSE.txt', 'README.md', 'src', 'tests'],
        cwd=REPO_ROOT,
    ).decode().split('\0')
    tarball = output / 'source.tar.gz'
    key_file = getattr(request.config, '_voiceinput_key_file', INTEGRATION_DIR / '.env.test')
    with tarfile.open(tarball, 'w:gz') as archive:
        for name in sorted(set(paths)):
            path = REPO_ROOT / name
            if not name or not path.is_file() or path.resolve() == key_file or any(
                part.startswith('.env') or part in ('build-out', '__pycache__', '.pytest_cache')
                for part in Path(name).parts
            ):
                continue
            archive.add(path, arcname='vollminputd/' + name, recursive=False)
    with tarfile.open(tarball) as archive:
        archive.extractall(snapshot, filter='data')
    return tarball


@pytest.fixture(scope='session')
def built_package(request, test_image, src_tarball):
    output = src_tarball.parent
    source = output / 'source' / 'vollminputd'
    packages = output / 'packages'
    packages.mkdir()
    manifest = {
        'head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO_ROOT, text=True).strip(),
        'source_sha256': hashlib.sha256(src_tarball.read_bytes()).hexdigest(),
        'image_id': test_image,
    }
    (output / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    proc = run_container(test_image, [
        (src_tarball, '/src.tar.gz', 'ro'),
        (source / 'tests/integration/scripts', '/tests/scripts', 'ro'),
        (packages, '/build-out', 'rw'),
    ], ['python3', '/tests/scripts/01_build_package.py'], output / 'build', network=True)
    assert proc.returncode == 0, f'Build or Rust tests failed; see {output / "build"}'
    found = list(packages.glob('vollminputd-git-*.pkg.tar.zst'))
    assert len(found) == 1
    manifest['package_sha256'] = hashlib.sha256(found[0].read_bytes()).hexdigest()
    (output / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    return found[0]


@pytest.fixture(autouse=True)
def live_key(request, monkeypatch):
    if request.node.get_closest_marker('live'):
        monkeypatch.setenv(KEY_NAME, request.config._voiceinput_key)


def run_test_container(test_image, script_path, src_tarball, built_package, extra_env=None):
    output = src_tarball.parent
    source = output / 'source' / 'vollminputd'
    env = dict(extra_env or {})
    live = env.get('TEST_LIVE_ASR') == '1'
    if live:
        env[KEY_NAME] = os.environ[KEY_NAME]
        for key in ('VOLLMINPUTD_ASR_ENDPOINT', 'VOLLMINPUTD_ASR_MODEL'):
            if os.environ.get(key):
                env[key] = os.environ[key]
    env['TEST_INSTANCE'] = 'pytest-' + uuid.uuid4().hex[:12]
    scenario = 'live-native' if live else 'offline-native'
    return run_container(test_image, [
        (source / 'tests/integration/scripts', '/tests/scripts', 'ro'),
        (source / 'tests', '/tests/repo-tests', 'ro'),
        (built_package.parent, '/build-out', 'ro'),
    ], ['dbus-run-session', '--', 'python3', f'/tests/scripts/{script_path.name}'], output / scenario,
        network=live, env=env, timeout=600)
