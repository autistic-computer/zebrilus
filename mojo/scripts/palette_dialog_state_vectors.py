import json
from pathlib import Path
import runpy
import subprocess
import tempfile

from palette_dialog_vectors import uint, blob

ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / 'target/port'


def text(output, value):
    blob(output, value.encode())


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    result = subprocess.run(['cargo', 'run', '--quiet', '--locked', '-p', 'zz-client', '--example', 'palette_dialog_state_vectors'], cwd=ROOT, capture_output=True, check=True)
    cases = json.loads(result.stdout)['cases']
    probe = TARGET / 'palette-dialog-state-probe'
    subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '--fp-mode=contract=off', '-I', str(ROOT / 'mojo'), str(ROOT / 'mojo/tests/palette_dialog_state_probe.mojo'), '-o', str(probe)], cwd=ROOT, check=True)
    data = bytearray()
    expected = bytearray()
    uint(data, len(cases))
    for case in cases:
        blob(data, case['bytes'])
        beginning = len(expected)
        uint(expected, case['prompt_revision'])
        uint(expected, case['chooser_revision'])
        expected.append(case['prompt'] is not None)
        if case['prompt'] is not None:
            text(expected, case['prompt'])
        chooser = case['chooser']
        expected.append(chooser is not None)
        if chooser is not None:
            uint(expected, chooser['rows'])
            uint(expected, chooser['selected'])
            expected.append(chooser['search'] is not None)
            if chooser['search'] is not None:
                text(expected, chooser['search']['query'])
                expected.append(chooser['search']['reverse'])
        summary = expected[beginning:]
        expected.extend(summary)
        expected.extend(summary)
    with tempfile.TemporaryDirectory(prefix='zz-palette-dialog-state-') as temporary:
        source, output = (Path(temporary) / name for name in ('input.bin', 'output.bin'))
        source.write_bytes(data)
        subprocess.run([str(probe), str(source), str(output), str(TARGET / 'zz-bend')], cwd=ROOT, check=True, timeout=60)
        actual = output.read_bytes()
    if actual != expected:
        position = next((index for index, pair in enumerate(zip(actual, expected)) if pair[0] != pair[1]), min(len(actual), len(expected)))
        raise AssertionError(f'dialog state differs at {position}: {actual[max(0,position-16):position+32].hex()} != {expected[max(0,position-16):position+32].hex()}')
    report = {'cases': len(cases), 'bytes': len(actual), 'retained_rows': 65536, 'snapshot_updates': 10000}
    (TARGET / 'palette-dialog-state-report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report))


if __name__ == '__main__':
    main()
