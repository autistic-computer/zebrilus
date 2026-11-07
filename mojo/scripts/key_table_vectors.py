import json
from pathlib import Path
import runpy
import subprocess


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / 'target/port'


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    source = TARGET / 'key-table-vectors.bin'
    subprocess.run(['cargo', 'run', '--quiet', '--locked', '--offline', '-p', 'zz-client', '--example', 'key_table_vectors', '--', str(source)], cwd=ROOT, check=True)
    probe = TARGET / 'key-table-probe'
    subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '-I', str(ROOT / 'mojo'), str(ROOT / 'mojo/tests/key_table_probe.mojo'), '-o', str(probe)], cwd=ROOT, check=True)
    result = subprocess.run([str(probe), str(source)], cwd=ROOT, capture_output=True, text=True, timeout=60)
    if result.returncode:
        raise RuntimeError(result.stderr[-8000:])
    hashes, updates = map(int, result.stdout.split())
    report = {'hashes': hashes, 'updates': updates, 'bytes': source.stat().st_size, 'passed': True}
    (TARGET / 'key-table-verification.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report), flush=True)


if __name__ == '__main__':
    main()
