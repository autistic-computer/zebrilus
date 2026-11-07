import json
from pathlib import Path
import runpy
import subprocess
import tempfile

from palette_places_vectors import uint, text


ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / 'mojo'
TARGET = ROOT / 'target/port'


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    subprocess.run(['cargo', 'build', '--offline', '--quiet', '--locked', '-p', 'zz-client', '--example', 'chooser_presentation_vectors'], cwd=ROOT, check=True)
    source = json.loads(subprocess.check_output([str(ROOT / 'target/debug/examples/chooser_presentation_vectors')], cwd=ROOT))
    data = bytearray()
    expected = bytearray()
    uint(data, len(source['cases']))
    rows = 0
    kinds = {'Terminal': 0, 'Browser': 1, 'Editor': 2, 'Agent': 3}
    for case in source['cases']:
        uint(data, len(case['frame']))
        data.extend(case['frame'])
        text(data, case['marked'])
        result = case['expected']
        text(expected, result['title'])
        text(expected, result['subtitle'])
        uint(expected, result['height'])
        expected.append(result['gutter'])
        uint(expected, result['selected'])
        uint(expected, len(result['rows']))
        for row in result['rows']:
            for value in row[:4]:
                text(expected, value)
            expected.extend(row[4:8])
            expected.append(row[8] is not None)
            if row[8] is not None:
                expected.append(kinds[row[8]])
            rows += 1
        expected.append(result['search'] is not None)
        if result['search'] is not None:
            for value in result['search']:
                text(expected, value)
    uint(data, len(source['policies']))
    for flags, result in source['policies']:
        data.append(flags)
        expected.append(result)
    binary = TARGET / 'chooser-presentation-probe'
    subprocess.run(['pixi', 'run', '--manifest-path', str(PORT / 'pixi.toml'), 'mojo', 'build', '--fp-mode=contract=off', '-I', str(PORT), str(PORT / 'tests/chooser_presentation_probe.mojo'), '-o', str(binary)], cwd=ROOT, check=True)
    with tempfile.TemporaryDirectory(prefix='zz-chooser-presentation-') as directory:
        path = Path(directory)
        (path / 'input').write_bytes(data)
        subprocess.run([str(binary), str(path / 'input'), str(path / 'output')], cwd=ROOT, check=True, timeout=60)
        actual = (path / 'output').read_bytes()
    if actual != expected:
        offset = next((index for index, pair in enumerate(zip(actual, expected)) if pair[0] != pair[1]), min(len(actual), len(expected)))
        raise AssertionError(f'chooser presentation differs at byte {offset}; {len(actual)} != {len(expected)}')
    report = {'passed': True, 'cases': len(source['cases']), 'rows': rows, 'policies': len(source['policies']), 'exact_bytes': len(actual), 'source': 'actual native chooser row, metadata, sizing and search-key helpers used by the GUI', 'scope': 'typed tree chooser presentation, shared row allocation, lazy row projection, custom formats, composition and explicit display limits; rendering and live routing remain separate'}
    (TARGET / 'chooser-presentation.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report))


if __name__ == '__main__':
    main()
