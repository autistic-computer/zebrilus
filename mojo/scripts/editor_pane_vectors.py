import hashlib
import json
import os
from pathlib import Path
import runpy
import shutil
import stat
import subprocess

ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / 'target/port'
LIMIT = 8 * 1024 * 1024


def wipe(path):
    if not path.exists() and not path.is_symlink():
        return
    for directory, _, files in os.walk(path):
        os.chmod(directory, 0o755)
        for name in files:
            entry = Path(directory) / name
            if not entry.is_symlink():
                os.chmod(entry, 0o644)
    shutil.rmtree(path)


def read_fixtures(root):
    wipe(root)
    root.mkdir(parents=True)
    (root / 'dir').mkdir()
    files = {
        'plain.txt': b'hello editor\nline two\n',
        'empty': b'',
        'nul.bin': b'abc\x00def',
        'invalid.txt': b'\xff\xfe plain',
        'truncated.txt': b'ok \xe2\x82',
        'overlong.txt': b'\xc0\x80',
        'surrogate.txt': b'\xed\xa0\x80',
        'highest.txt': b'\xf4\x8f\xbf\xbf',
        'beyond.txt': b'\xf4\x90\x80\x80',
        'utf8.md': '# héllo ✓ \U0001f642\n'.encode(),
        'bom.txt': b'\xef\xbb\xbfbom\n',
        'exact.txt': b'a' * LIMIT,
        'noperm.txt': b'secret',
    }
    for name, data in files.items():
        (root / name).write_bytes(data)
    with open(root / 'over.bin', 'wb') as file:
        file.truncate(LIMIT + 1)
    os.chmod(root / 'noperm.txt', 0)
    os.symlink('plain.txt', root / 'link')
    os.symlink('nowhere', root / 'dangling')


def save_fixtures(root):
    wipe(root)
    (root / 'real').mkdir(parents=True)
    (root / 'ro').mkdir()
    (root / 'keep.txt').write_bytes(b'old')
    os.chmod(root / 'keep.txt', 0o600)
    (root / 'real/target.txt').write_bytes(b'old target')
    os.symlink('real/target.txt', root / 'link.txt')
    (root / 'ro/target.txt').write_bytes(b'ro target')
    os.chmod(root / 'ro/target.txt', 0o640)
    os.chmod(root / 'ro', 0o555)
    os.symlink('ro/target.txt', root / 'rolink.txt')
    os.symlink('nowhere/x', root / 'dangling.txt')
    (root / 'plain').write_bytes(b'i am a file')
    (root / 'exec.sh').write_bytes(b'old')
    os.chmod(root / 'exec.sh', 0o755)


def tree(root):
    entries = {}
    for directory, folders, files in os.walk(root):
        for name in folders + files:
            path = Path(directory) / name
            status = path.lstat()
            key = str(path.relative_to(root))
            if stat.S_ISLNK(status.st_mode):
                entries[key] = ['link', os.readlink(path)]
            elif stat.S_ISDIR(status.st_mode):
                entries[key] = ['dir', stat.S_IMODE(status.st_mode)]
            else:
                entries[key] = ['file', stat.S_IMODE(status.st_mode), hashlib.sha256(path.read_bytes()).hexdigest()]
    return entries


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    TARGET.mkdir(parents=True, exist_ok=True)
    reads = TARGET / 'editor-read-fixtures'
    rust_saves = TARGET / 'editor-save-rust'
    mojo_saves = TARGET / 'editor-save-mojo'
    read_fixtures(reads)
    save_fixtures(rust_saves)
    save_fixtures(mojo_saves)
    source = TARGET / 'editor-pane-vectors.bin'
    subprocess.run(['cargo', 'run', '--quiet', '--locked', '--offline', '-p', 'zz-client', '--example', 'editor_pane_vectors', '--', str(source), str(reads), str(rust_saves)], cwd=ROOT, check=True)
    output = TARGET / 'editor-state-probe'
    subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '--fp-mode=contract=off', '-j', '1', '-I', str(ROOT / 'mojo'), '-I', str(ROOT / 'mojo/tests'), str(ROOT / 'mojo/tests/editor_state_probe.mojo'), '-o', str(output)], cwd=ROOT, check=True)
    result = subprocess.run([str(output), str(source), str(reads), str(mojo_saves)], cwd=ROOT, check=True, capture_output=True, text=True, timeout=120)
    syncs, names, validations, read_cases, save_cases, scenarios, steps = map(int, result.stdout.split())
    expected = tree(rust_saves)
    actual = tree(mojo_saves)
    if expected != actual:
        raise SystemExit(f'saved trees differ: {json.dumps(expected, sort_keys=True)} != {json.dumps(actual, sort_keys=True)}')
    leftovers = [name for name in expected if '.tmp-' in name]
    if leftovers:
        raise SystemExit(f'temporary files left behind: {leftovers}')
    if expected['keep.txt'][1] != 0o600 or expected['exec.sh'][1] != 0o755 or expected['link.txt'] != ['link', 'real/target.txt'] or expected['rolink.txt'][0] != 'file' or expected['dangling.txt'][0] != 'file':
        raise SystemExit(f'saved tree is not the expected one: {json.dumps(expected, sort_keys=True)}')
    report = {'passed': True, 'sync_cases': syncs, 'name_cases': names, 'validation_cases': validations, 'read_cases': read_cases, 'save_cases': save_cases, 'saved_entries': len(expected), 'model_scenarios': scenarios, 'model_steps': steps, 'bytes': source.stat().st_size, 'source': 'actual editor pane model, descriptor validation, bounded read and atomic save from zz-client and zz-daemon-client'}
    (TARGET / 'editor-pane-verification.json').write_text(json.dumps(report, indent=2) + '\n')
    for path in (reads, rust_saves, mojo_saves):
        wipe(path)
    print(json.dumps(report), flush=True)


if __name__ == '__main__':
    main()
