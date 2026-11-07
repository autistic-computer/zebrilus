import argparse
import json
from pathlib import Path
import runpy
import subprocess
import time


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / 'target/port'


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    parser = argparse.ArgumentParser()
    parser.add_argument('--skip-build', action='store_true')
    parser.add_argument('--skip-reference', action='store_true')
    options = parser.parse_args()
    directory = TARGET / 'desktop-input-vectors'
    directory.mkdir(parents=True, exist_ok=True)
    if not options.skip_reference:
        subprocess.run(['cargo', 'run', '--quiet', '--locked', '--offline', '-p', 'zz-client', '--example', 'desktop_input_vectors', '--', str(directory)], cwd=ROOT, check=True)
    probe = TARGET / 'desktop-input-probe'
    if not options.skip_build:
        subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '-I', str(ROOT / 'mojo'), str(ROOT / 'mojo/tests/desktop_input_probe.mojo'), '-o', str(probe)], cwd=ROOT, check=True)
    started = time.monotonic()
    completed = subprocess.run([str(probe), str(directory)], cwd=ROOT, capture_output=True, text=True, timeout=120)
    if completed.returncode:
        raise RuntimeError(completed.stdout[-4000:] + completed.stderr[-8000:])
    report = dict(zip(['guards', 'projections', 'inputs', 'updates', 'queries', 'constructor_keys'], map(int, completed.stdout.split()), strict=True))
    report.update({'passed': True, 'seconds': round(time.monotonic() - started, 3), 'source_bytes': sum(path.stat().st_size for path in directory.glob('*.bin'))})
    (TARGET / 'desktop-input-verification.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report), flush=True)


if __name__ == '__main__':
    main()
