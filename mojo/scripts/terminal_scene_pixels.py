import argparse
import json
from pathlib import Path
import runpy
import subprocess

from terminal_styled_pixels import CASES, EXTRA_CASES, fixture
from terminal_pixels import compare

ROOT = Path(__file__).resolve().parents[2]
MJUI = ROOT.parent / 'mjui'
DIRECTORY = ROOT / 'target/port/terminal-scene-reference'


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    parser = argparse.ArgumentParser()
    parser.add_argument('--skip-native', action='store_true')
    parser.add_argument('--skip-build', action='store_true')
    parser.add_argument('--diagnose', action='store_true')
    parser.add_argument('--case', choices=CASES + EXTRA_CASES, action='append')
    parser.add_argument('--scale', type=float, choices=(1, 1.25, 2), action='append')
    args = parser.parse_args()
    cases = args.case or CASES + EXTRA_CASES
    scales = args.scale or (1, 1.25, 2)
    DIRECTORY.mkdir(parents=True, exist_ok=True)
    if not args.skip_native:
        subprocess.run(['cargo', 'build', '--quiet', '--locked', '-p', 'zz-ui', '--features', 'scene-fixtures', '--example', 'terminal_styled_fixture'], cwd=ROOT, check=True)
        for case in cases:
            for scale in scales:
                directory = DIRECTORY / f'{case}-scale-{scale:g}'
                subprocess.run([str(ROOT / 'target/debug/examples/terminal_styled_fixture'), str(directory), case, '13', str(scale)], cwd=ROOT, check=True, timeout=60)
                fixture(directory)
    binary = ROOT / 'target/port/terminal-scene-probe'
    if not args.skip_build:
        subprocess.run(['pixi', 'run', 'bash', '-c', 'mojo build --fp-mode=contract=off -I src -I "$1/mojo" -D MJ_RASTER=swash -Xlinker build/lib/libmjswash.a $MJ_LINK -Xlinker build/lib/libSDL3.a -Xlinker -lgcc_s -Xlinker -ldl -Xlinker -lrt -Xlinker -lutil "$1/mojo/terminal_styled_probe.mojo" -o "$2"', '_', str(ROOT), str(binary)], cwd=MJUI, check=True)
    records = []
    for case in cases:
        for scale in scales:
            directory = DIRECTORY / f'{case}-scale-{scale:g}'
            subprocess.run([str(binary), str(directory / 'input.bin'), str(directory / 'mojo.png'), str(directory / 'mojo.json'), 'scene'], cwd=MJUI, check=True, timeout=60)
            result = compare(directory)
            expected = [entry['bounds_bits'] for entry in json.loads((directory / 'geometry.json').read_text())['text_layers'] if entry['kind'] == 0]
            actual = json.loads((directory / 'mojo.json').read_text())['text_layers']
            result.update(case=case, scale=scale, text_layers=len(expected), text_layers_exact=actual == expected)
            if actual != expected:
                index = next((index for index, pair in enumerate(zip(actual, expected)) if pair[0] != pair[1]), min(len(actual), len(expected)))
                result['first_layer_difference'] = {'index': index, 'actual': actual[index] if index < len(actual) else None, 'expected': expected[index] if index < len(expected) else None}
            records.append(result)
            print(json.dumps(result), flush=True)
    report = {'cases': records, 'exact': all(item['exact'] and item['text_layers_exact'] for item in records)}
    (DIRECTORY / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    if not report['exact'] and not args.diagnose:
        raise SystemExit('terminal source scene identity is incomplete')


if __name__ == '__main__':
    main()
