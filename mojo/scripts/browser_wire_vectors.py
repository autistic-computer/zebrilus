import json
from pathlib import Path
import runpy
import subprocess

ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / 'target/port'


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    TARGET.mkdir(parents=True, exist_ok=True)
    source = TARGET / 'browser-wire-vectors.bin'
    subprocess.run(['cargo', 'run', '--quiet', '--locked', '--offline', '-p', 'zz-protocol', '--example', 'browser_port_vectors', '--', str(source)], cwd=ROOT, check=True)
    output = TARGET / 'browser-wire-probe'
    subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '--fp-mode=contract=off', '-I', str(ROOT / 'mojo'), '-I', str(ROOT / 'mojo/tests'), str(ROOT / 'mojo/tests/browser_wire_probe.mojo'), '-o', str(output)], cwd=ROOT, check=True)
    result = subprocess.run([str(output), str(source)], cwd=ROOT, check=True, capture_output=True, text=True, timeout=30)
    report = {'passed': True, 'commands': int(result.stdout), 'bytes': source.stat().st_size, 'source': 'actual protocol encoder BrowserCommand all eight variants, full-width IDs and repeated key counts', 'live_consumer': False}
    (TARGET / 'browser-wire-verification.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report), flush=True)


if __name__ == '__main__':
    main()
