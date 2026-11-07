import argparse
import json
from pathlib import Path
import runpy
import subprocess


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / 'target/port/buffer-presentation'


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    parser = argparse.ArgumentParser()
    parser.add_argument('--skip-reference', action='store_true')
    parser.add_argument('--skip-build', action='store_true')
    args = parser.parse_args()
    TARGET.mkdir(parents=True, exist_ok=True)
    if not args.skip_reference:
        subprocess.run(['cargo', 'run', '--quiet', '--locked', '--offline', '-p', 'zz-client', '--example', 'buffer_presentation_vectors', '--', str(TARGET / 'input.bin')], cwd=ROOT, check=True)
    binary = TARGET / 'probe'
    if not args.skip_build:
        subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '-I', str(ROOT / 'mojo'), str(ROOT / 'mojo/tests/buffer_presentation_probe.mojo'), '-o', str(binary)], cwd=ROOT, check=True)
    completed = subprocess.run([str(binary), str(TARGET / 'input.bin')], cwd=ROOT, capture_output=True, text=True, timeout=60)
    if completed.returncode:
        raise RuntimeError(completed.stdout[-4000:] + completed.stderr[-6000:])
    numeric, cases, rows, bytes = map(int, completed.stdout.split())
    report = {'passed': True, 'numeric_cases': numeric, 'states': cases, 'rows': rows, 'exact_bytes': bytes, 'scope': 'actual source buffer_row_text with explicit captured now, chooser title/subtitle/height/gutter, all unsigned rounding/saturation boundaries, custom row formats, search composition and shared retained allocation; visual/live behavior checked separately'}
    (TARGET / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report))


if __name__ == '__main__':
    main()
