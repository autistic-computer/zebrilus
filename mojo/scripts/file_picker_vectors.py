import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / 'target/port'


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
    (root / 'dir').mkdir(parents=True)
    (root / 'plain.txt').write_bytes(b'hello picker\n')
    (root / 'noperm.txt').write_bytes(b'secret')
    os.chmod(root / 'noperm.txt', 0)
    os.symlink('plain.txt', root / 'link')
    os.symlink('dir', root / 'dirlink')
    os.symlink('nowhere', root / 'dangling')


def scan_fixtures(root):
    wipe(root)
    for folder in ('workspace/.git', 'workspace/src', 'plain/node_modules/inner', 'target/debug', '.hidden/inside', 'Music/album', 'deep/1/2/3/4/5/6/7/8'):
        (root / folder).mkdir(parents=True)
    for file in ('workspace/.git/config', 'plain/note.md', 'plain/node_modules/pkg.json', '.env', 'Music/song.txt', 'file.txt', 'deep/1/2/3/4/5/6/near.txt', 'deep/1/2/3/4/5/6/7/far.txt'):
        (root / file).write_bytes(b'x')
    os.symlink('plain', root / 'link')


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    TARGET.mkdir(parents=True, exist_ok=True)
    reads = TARGET / 'file-picker-read-fixtures'
    scans = TARGET / 'file-picker-scan-fixtures'
    read_fixtures(reads)
    scan_fixtures(scans)
    source = TARGET / 'file-picker-vectors.bin'
    subprocess.run(['cargo', 'run', '--quiet', '--locked', '--offline', '-p', 'zz-client', '--example', 'file_picker_vectors', '--', str(source), str(reads)], cwd=ROOT, check=True)
    output = TARGET / 'file-picker-probe'
    subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '--fp-mode=contract=off', '-j', '1', '-I', str(ROOT / 'mojo'), '-I', str(ROOT / 'mojo/tests'), str(ROOT / 'mojo/tests/file_picker_probe.mojo'), '-o', str(output)], cwd=ROOT, check=True)
    result = subprocess.run([str(output), str(source), str(reads), str(scans)], cwd=ROOT, check=True, capture_output=True, text=True, timeout=120)
    priors, selections, queries, sets, ranked_queries, ranked_rows, validations, sequences, operations = map(int, result.stdout.split())
    report = {'passed': True, 'prior_cases': priors, 'selection_cases': selections, 'query_cases': queries, 'rank_sets': sets, 'rank_queries': ranked_queries, 'ranked_rows': ranked_rows, 'validation_cases': validations, 'flow_sequences': sequences, 'flow_operations': operations, 'scan_entries': 18, 'scan_rows': 12, 'bytes': source.stat().st_size, 'source': 'actual file picker priors, selection, query, ranking, validation and flow from zz-client; the scan is checked against a fixed tree'}
    (TARGET / 'file-picker-verification.json').write_text(json.dumps(report, indent=2) + '\n')
    for path in (reads, scans):
        wipe(path)
    print(json.dumps(report), flush=True)


if __name__ == '__main__':
    main()
