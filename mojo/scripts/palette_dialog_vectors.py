import argparse
import json
from pathlib import Path
import runpy
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / 'target/port'


def uint(output, value):
    while value >= 128:
        output.append((value & 127) | 128)
        value >>= 7
    output.append(value)


def blob(output, value):
    uint(output, len(value))
    output.extend(value)


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    parser = argparse.ArgumentParser()
    parser.add_argument('--skip-reference', action='store_true')
    parser.add_argument('--skip-build', action='store_true')
    args = parser.parse_args()
    reference = TARGET / 'palette-dialog-vectors.json'
    if not args.skip_reference:
        result = subprocess.run(['cargo', 'run', '--quiet', '--locked', '-p', 'zz-protocol', '--example', 'palette_dialog_vectors'], cwd=ROOT, capture_output=True, check=True)
        reference.write_bytes(result.stdout)
    cases = json.loads(reference.read_text())['cases']
    probe = TARGET / 'palette-dialog-probe'
    if not args.skip_build:
        subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '--fp-mode=contract=off', '-I', str(ROOT / 'mojo'), str(ROOT / 'mojo/tests/palette_dialog_probe.mojo'), '-o', str(probe)], cwd=ROOT, check=True)
    data = bytearray()
    expected = bytearray()
    uint(data, len(cases))
    uint(expected, len(cases))
    accepted = rejected = refusals = 0
    for case in cases:
        data.append(case['kind'])
        blob(data, case['bytes'])
        valid = case['expected'] is not None and not case['refusal']
        expected.append(valid)
        if valid:
            blob(expected, case['expected'])
            accepted += 1
        elif case['refusal']:
            refusals += 1
        else:
            rejected += 1
    with tempfile.TemporaryDirectory(prefix='zz-palette-dialog-') as temporary:
        source, output = (Path(temporary) / name for name in ('input.bin', 'output.bin'))
        source.write_bytes(data)
        subprocess.run([str(probe), str(source), str(output)], cwd=ROOT, check=True, timeout=60)
        actual = output.read_bytes()
    if actual != expected:
        position = next((index for index, pair in enumerate(zip(actual, expected)) if pair[0] != pair[1]), min(len(actual), len(expected)))
        raise AssertionError(f'dialog source bytes differ at {position}: actual {actual[max(0,position-16):position+32].hex()}, expected {expected[max(0,position-16):position+32].hex()}')
    report = {'cases': len(cases), 'accepted': accepted, 'rejected': rejected, 'refusals': refusals, 'bytes': len(actual)}
    (TARGET / 'palette-dialog-report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report))


if __name__ == '__main__':
    main()
