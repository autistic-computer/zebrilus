import json
from pathlib import Path
import runpy
import subprocess


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / 'target/port'


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    source = TARGET / 'desktop-prefix-cancel-vectors.bin'
    subprocess.run(['cargo', 'run', '--quiet', '--locked', '--offline', '-p', 'zz-client', '--example', 'desktop_prefix_cancel_vectors', '--', str(source)], cwd=ROOT, check=True)
    probe = TARGET / 'desktop-prefix-cancel-probe'
    subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '-I', str(ROOT / 'mojo'), str(ROOT / 'mojo/tests/desktop_prefix_cancel_probe.mojo'), '-o', str(probe)], cwd=ROOT, check=True)
    result = subprocess.run([str(probe), str(source)], cwd=ROOT, capture_output=True, text=True, timeout=30)
    if result.returncode:
        raise RuntimeError(result.stdout[-4000:] + result.stderr[-8000:])
    report = {'passed': True, 'cases': int(result.stdout), 'bytes': source.stat().st_size, 'source': 'actual GUI/MuxClient delegated prefix helpers and wire encoder'}
    (TARGET / 'desktop-prefix-cancel-verification.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report), flush=True)


if __name__ == '__main__':
    main()
