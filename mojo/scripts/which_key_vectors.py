import argparse
import json
from pathlib import Path
import runpy
import subprocess
import time


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / 'target/port/which-key'


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    parser = argparse.ArgumentParser()
    parser.add_argument('--skip-reference', action='store_true')
    parser.add_argument('--skip-build', action='store_true')
    parser.add_argument('--update-data', action='store_true')
    options = parser.parse_args()
    TARGET.mkdir(parents=True, exist_ok=True)
    if not options.skip_reference:
        subprocess.run(['cargo', 'run', '--quiet', '--locked', '--offline', '-p', 'zz-client', '--example', 'which_key_vectors', '--', str(TARGET)], cwd=ROOT, check=True)
    stock = (TARGET / 'stock.bin').read_bytes().hex()
    names = '\n'.join(json.loads((TARGET / 'implemented.json').read_text()))
    expected = 'comptime WHICH_KEY_STOCK = ' + json.dumps(stock) + '\n\ncomptime WHICH_KEY_IMPLEMENTED = ' + json.dumps(names) + '\n'
    data = ROOT / 'mojo/zzmojo/which_key_data.mojo'
    if options.update_data:
        data.write_text(expected)
    if data.read_text() != expected:
        raise AssertionError('which-key stock/catalog differs from source; rerun with --update-data')
    binary = TARGET / 'probe'
    if not options.skip_build:
        subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '-I', str(ROOT / 'mojo'), str(ROOT / 'mojo/tests/which_key_probe.mojo'), '-o', str(binary)], cwd=ROOT, check=True)
    started = time.monotonic()
    completed = subprocess.run([str(binary), str(TARGET / 'cases.bin')], cwd=ROOT, check=True, capture_output=True, text=True, timeout=120)
    cases, rows, sets, keys, commands = map(int, completed.stdout.split())
    report = {'passed': True, 'cases': cases, 'rows': rows, 'sets': sets, 'keys': keys, 'opens_all_keys_queries': commands, 'source_bytes': (TARGET / 'cases.bin').stat().st_size, 'seconds': round(time.monotonic() - started, 3), 'scope': 'actual zz-client rows/action_rows/opens_all_keys; source stock tables and exact implemented catalog; full serialized invocation metadata'}
    (TARGET / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report))


if __name__ == '__main__':
    main()
