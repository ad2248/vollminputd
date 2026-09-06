"""Mocked harness tests for conftest internals: load_key, src_tarball, run_container.
Every subprocess is faked (no podman/git/network); not live; never reads real .env.test.
"""
import subprocess
import tarfile
from pathlib import Path
from types import SimpleNamespace

import pytest

import conftest

KEY = conftest.KEY_NAME
BANNED_PARTS = ('build-out', '__pycache__', '.pytest_cache')


@pytest.fixture(autouse=True)
def _no_real_key(monkeypatch):
    monkeypatch.delenv(KEY, raising=False)


def _keyfile(tmp_path, text):
    path = tmp_path / 'key.env'
    path.write_text(text)
    return path


# ------------------------------------------------------------------ load_key
def test_load_key_dotenv_quotes_comments(tmp_path):
    path = _keyfile(tmp_path, f'# lead comment\nexport {KEY}="kv-123"  # inline\n')
    assert conftest.load_key(path) == 'kv-123'


def test_load_key_bare_single_line(tmp_path):
    assert conftest.load_key(_keyfile(tmp_path, 'bare-key-42\n')) == 'bare-key-42'


def test_load_key_env_wins_over_file(tmp_path, monkeypatch):
    monkeypatch.setenv(KEY, 'from-env')
    assert conftest.load_key(_keyfile(tmp_path, f'{KEY}=from-file\n')) == 'from-env'


def test_load_key_blank_env_falls_back_to_file(tmp_path, monkeypatch):
    monkeypatch.setenv(KEY, '   \n')
    assert conftest.load_key(_keyfile(tmp_path, f'{KEY}=from-file\n')) == 'from-file'


@pytest.mark.parametrize('content', [
    '',
    f'{KEY}=YOUR_DASHSCOPE_KEY_HERE\n',
    f'{KEY}="spaced key"\n',
    f'{KEY}=two tokens\n',
    f'export {KEY}=\n',
    'OTHER=1\nANOTHER=2\n',
    'a=b\n',
])
def test_load_key_rejects_bad_content(tmp_path, content):
    with pytest.raises(pytest.UsageError):
        conftest.load_key(_keyfile(tmp_path, content))


def test_load_key_placeholder_env_rejected(tmp_path, monkeypatch):
    monkeypatch.setenv(KEY, 'YOUR_DASHSCOPE_KEY_HERE')
    with pytest.raises(pytest.UsageError):
        conftest.load_key(_keyfile(tmp_path, f'{KEY}=real-one\n'))


def test_load_key_missing_file_rejected(tmp_path):
    with pytest.raises(pytest.UsageError):
        conftest.load_key(tmp_path / 'absent.env')


# --------------------------------------------------------------- src_tarball
def _fake_request(output):
    output.mkdir(parents=True, exist_ok=True)
    return SimpleNamespace(config=SimpleNamespace(_voiceinput_output=output))


def _mock_ls_files(monkeypatch, repo, listings):
    monkeypatch.setattr(conftest, 'REPO_ROOT', repo)
    it = iter(listings)
    monkeypatch.setattr(conftest.subprocess, 'check_output',
                        lambda cmd, cwd=None: '\0'.join(next(it)).encode())


def _build_repo(tmp_path):
    repo = tmp_path / 'repo'
    (repo / 'src').mkdir(parents=True)
    (repo / 'tests' / 'integration' / 'build-out').mkdir(parents=True)
    (repo / 'src' / '__pycache__').mkdir()
    (repo / '.pytest_cache' / 'v').mkdir(parents=True)
    (repo / 'Cargo.toml').write_text('[package]\n')
    (repo / 'src' / 'lib.rs').write_text('dirty tracked\n')
    (repo / 'src' / 'brand_new.rs').write_text('just added\n')
    (repo / '.env.test').write_text('SECRET=never-read\n')
    (repo / 'tests' / 'integration' / 'build-out' / 'junk.bin').write_text('x')
    (repo / 'src' / '__pycache__' / 'm.pyc').write_text('')
    (repo / '.pytest_cache' / 'v' / 'cache').write_text('')
    return repo


def test_src_tarball_filters_and_content(tmp_path, monkeypatch):
    repo = _build_repo(tmp_path)
    _mock_ls_files(monkeypatch, repo, [
        ['Cargo.toml', 'src/lib.rs', 'src/brand_new.rs', '.env.test',
         'tests/integration/build-out/junk.bin', 'src/__pycache__/m.pyc',
         '.pytest_cache/v/cache', 'src/deleted.rs']])
    tarball = conftest.src_tarball.__wrapped__(_fake_request(tmp_path / 'out'))
    names = tarfile.open(tarball).getnames()
    assert 'vollminputd/Cargo.toml' in names
    assert 'vollminputd/src/lib.rs' in names
    assert 'vollminputd/src/brand_new.rs' in names
    assert 'vollminputd/src/deleted.rs' not in names
    assert not any(p.startswith('.env') or p in BANNED_PARTS
                   for n in names for p in Path(n).parts)
    assert (tmp_path / 'out' / 'source' / 'vollminputd' / 'src' / 'brand_new.rs').is_file()


def test_src_tarball_rebuilt_per_output_dir(tmp_path, monkeypatch):
    repo = _build_repo(tmp_path)
    _mock_ls_files(monkeypatch, repo, [['Cargo.toml'], ['src/brand_new.rs']])
    first = conftest.src_tarball.__wrapped__(_fake_request(tmp_path / 'out1'))
    second = conftest.src_tarball.__wrapped__(_fake_request(tmp_path / 'out2'))
    assert first != second
    assert first.parent == tmp_path / 'out1' and second.parent == tmp_path / 'out2'
    assert tarfile.open(first).getnames() == ['vollminputd/Cargo.toml']
    assert tarfile.open(second).getnames() == ['vollminputd/src/brand_new.rs']


def test_src_tarball_excludes_custom_key_file(tmp_path, monkeypatch):
    repo = _build_repo(tmp_path)
    key_file = repo / 'tests' / 'local.key'
    key_file.write_text('must-not-be-archived')
    _mock_ls_files(monkeypatch, repo, [['Cargo.toml', 'tests/local.key']])
    request = _fake_request(tmp_path / 'out')
    request.config._voiceinput_key_file = key_file.resolve()
    with tarfile.open(conftest.src_tarball.__wrapped__(request)) as archive:
        assert archive.getnames() == ['vollminputd/Cargo.toml']


# ------------------------------------------------------------- run_container
def _install_fake_podman(monkeypatch, *, log_line='', timeout=False):
    calls = []

    def run(cmd, **kw):
        calls.append((list(cmd), kw))
        if cmd[1] == 'rm':
            return subprocess.CompletedProcess(cmd, 0, '', '')
        if log_line and kw.get('stdout') is not None:
            kw['stdout'].write(log_line)
        if timeout:
            raise subprocess.TimeoutExpired(cmd, kw.get('timeout'))
        return subprocess.CompletedProcess(cmd, 0, '', '')

    monkeypatch.setattr(conftest.subprocess, 'run', run)
    return calls


def test_run_container_offline_strips_key_and_network(tmp_path, monkeypatch):
    monkeypatch.setenv(KEY, 'offline-secret')
    monkeypatch.setenv('http_proxy', 'http://proxy:9')
    calls = _install_fake_podman(monkeypatch)
    out = tmp_path / 'artifacts'
    proc = conftest.run_container('img:1', [('/host', '/ct', 'ro')], ['run-me'], out)
    cmd, kw = calls[0]
    assert '--network=none' in cmd and '--http-proxy=false' in cmd
    assert KEY not in kw['env'] and KEY not in cmd and '--env' not in cmd
    assert cmd[cmd.index('--volume') + 1] == '/host:/ct:ro'
    assert cmd[cmd.index('--volume') + 3] == f'{out}:/artifacts:rw'
    assert cmd[-2:] == ['img:1', 'run-me']
    assert proc.returncode == 0 and (out / 'container.log').exists()
    assert [c[0][1] for c in calls] == ['run', 'rm']
    assert '--force' in calls[1][0] and '--ignore' in calls[1][0]


def test_run_container_live_key_by_env_not_argv(tmp_path, monkeypatch):
    secret = 'sk-live-abc123'
    monkeypatch.setenv(KEY, secret)
    calls = _install_fake_podman(monkeypatch, log_line=f'child saw {secret}\n')
    out = tmp_path / 'live'
    out.mkdir()
    (out / 'child.txt').write_text(f'leak {secret}\n')
    proc = conftest.run_container('img:1', [], ['true'], out, network=True,
                                  env={KEY: secret, 'FOO': 'bar'})
    cmd, kw = calls[0]
    passed = {cmd[i + 1] for i, name in enumerate(cmd) if name == '--env'}
    assert KEY in passed and 'FOO' in passed
    assert secret not in cmd
    assert kw['env'][KEY] == secret and kw['env']['FOO'] == 'bar'
    assert '--network=none' not in cmd and '--http-proxy=false' not in cmd
    assert proc.returncode == 0 and secret not in proc.stdout
    log = (out / 'container.log').read_text()
    assert '[REDACTED]' in log and secret not in log
    assert secret not in (out / 'child.txt').read_text()


def test_run_container_timeout_redacts_logs(tmp_path, monkeypatch):
    secret = 'sk-timeout-xyz'
    monkeypatch.setenv(KEY, secret)
    calls = _install_fake_podman(monkeypatch, log_line=f'building with {secret}\n',
                                 timeout=True)
    out = tmp_path / 'timed'
    out.mkdir()
    (out / 'child.log').write_text(f'leak {secret}\n')
    with pytest.raises(pytest.fail.Exception, match='exceeded 5s'):
        conftest.run_container('img:1', [], ['sleep'], out, network=True,
                               env={KEY: secret}, timeout=5)
    log = (out / 'container.log').read_text()
    assert '[REDACTED]' in log and secret not in log
    assert secret not in (out / 'child.log').read_text()
    assert [c[0][1] for c in calls] == ['run', 'rm']
