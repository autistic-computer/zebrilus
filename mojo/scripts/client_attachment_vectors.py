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
    TARGET.mkdir(parents=True, exist_ok=True)
    reference = TARGET / 'client-attachment-vectors.json'
    if not args.skip_reference:
        result = subprocess.run(['cargo', 'run', '--quiet', '--locked', '-p', 'zz-protocol', '--example', 'client_attachment_vectors'], cwd=ROOT, capture_output=True, check=True)
        reference.write_bytes(result.stdout)
    cases = json.loads(reference.read_text())['cases']
    probe = TARGET / 'client-attachment-probe'
    if not args.skip_build:
        subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '--fp-mode=contract=off', '-I', str(ROOT / 'mojo'), str(ROOT / 'mojo/tests/client_attachment_probe.mojo'), '-o', str(probe)], cwd=ROOT, check=True)
    data = bytearray()
    expected = bytearray()
    uint(data, len(cases))
    uint(expected, len(cases))
    totals = [0, 0, 0]
    refusals = 0
    boundaries = []
    for item in cases:
        data.append(item['kind'])
        blob(data, bytes.fromhex(item['bytes']))
        expected.append(item['status'])
        totals[item['status']] += 1
        refusals += item['refusal']
        if item['status'] == 1:
            blob(expected, bytes.fromhex(item['expected']))
        boundaries.append(len(expected))
    with tempfile.TemporaryDirectory(prefix='zz-client-attachment-') as temporary:
        source, output = (Path(temporary) / name for name in ('input.bin', 'output.bin'))
        source.write_bytes(data)
        subprocess.run([str(probe), str(source), str(output)], cwd=ROOT, check=True, timeout=60)
        actual = output.read_bytes()
    if actual != expected:
        position = next((index for index, pair in enumerate(zip(actual, expected)) if pair[0] != pair[1]), min(len(actual), len(expected)))
        index = next((index for index, boundary in enumerate(boundaries) if boundary > position), len(cases) - 1)
        raise AssertionError(f'attachment source differs in case {index} ({cases[index]["name"]}) at byte {position}: actual {actual[max(0,position-8):position+24].hex()}, expected {expected[max(0,position-8):position+24].hex()}')
    report = {'cases': len(cases), 'accepted': totals[1], 'rejected': totals[0] - refusals, 'ignored': totals[2], 'source_valid_refusals': refusals, 'exact_bytes': len(actual)}
    (TARGET / 'client-attachment-report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report))


if __name__ == '__main__':
    main()
