import argparse
import json
from pathlib import Path
import runpy
import subprocess


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / 'target/port/which-key-timing'


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    parser = argparse.ArgumentParser()
    parser.add_argument('--skip-reference', action='store_true')
    parser.add_argument('--skip-build', action='store_true')
    args = parser.parse_args()
    TARGET.mkdir(parents=True, exist_ok=True)
    vectors = TARGET / 'cases.bin'
    if not args.skip_reference:
        subprocess.run(['cargo', 'run', '--quiet', '--locked', '--offline', '-p', 'zz-client', '--example', 'which_key_timing_vectors', '--', str(vectors)], cwd=ROOT, check=True)
    binary = TARGET / 'probe'
    if not args.skip_build:
        subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '-I', str(ROOT / 'mojo'), str(ROOT / 'mojo/tests/which_key_timing_probe.mojo'), '-o', str(binary)], cwd=ROOT, check=True)
    completed = subprocess.run([str(binary), str(vectors)], cwd=ROOT, capture_output=True, text=True, timeout=60)
    if completed.returncode:
        raise RuntimeError(completed.stdout[-6000:] + completed.stderr[-6000:])
    cases, tables, hide, inputs, changes, fires, renders, shows, notified, built = map(int, completed.stdout.split())
    report = {'passed': True, 'cases': cases, 'arm_table_cases': tables, 'operations': {'hide': hide, 'input': inputs, 'table_changed': changes, 'fire': fires, 'render': renders, 'show_all': shows}, 'notified': notified, 'built': built, 'bytes': vectors.stat().st_size, 'source': 'zz_client::which_key::timing, whose hidden_generation, hide_for_input, arm_table and timer_decision the desktop workspace calls'}
    (TARGET / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report))


if __name__ == '__main__':
    main()
