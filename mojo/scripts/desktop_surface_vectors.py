import json
from pathlib import Path
import runpy
import subprocess


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / 'target/port'


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    TARGET.mkdir(parents=True, exist_ok=True)
    source = TARGET / 'desktop-surface-vectors.bin'
    subprocess.run(['cargo', 'run', '--quiet', '--locked', '--offline', '-p', 'zz-client', '--example', 'desktop_surface_vectors', '--', str(source)], cwd=ROOT, check=True)
    probe = TARGET / 'desktop-surface-probe'
    subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '-I', str(ROOT / 'mojo'), str(ROOT / 'mojo/tests/desktop_surface_probe.mojo'), '-o', str(probe)], cwd=ROOT, check=True)
    result = subprocess.run([str(probe), str(source)], cwd=ROOT, capture_output=True, text=True, timeout=30)
    if result.returncode:
        raise RuntimeError(result.stdout[-4000:] + result.stderr[-8000:])
    decisions, bindings, floats, steps = map(int, result.stdout.split())
    report = {'passed': True, 'terminal_decisions': decisions, 'typed_actions_per_context': bindings, 'font_adjustments': floats, 'font_trace_steps': steps, 'bytes': source.stat().st_size, 'source': 'actual desktop terminal key, native terminal/sidebar/browser binding and font helpers', 'live_hook': False}
    (TARGET / 'desktop-surface-verification.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report), flush=True)


if __name__ == '__main__':
    main()
