import argparse
import bisect
import json
from pathlib import Path
import runpy
import subprocess

from palette_places_vectors import uint, text


ROOT = Path(__file__).resolve().parents[2]
MJUI = ROOT.parent / 'mjui'
TARGET = ROOT / 'target/port/native-truncate'


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    parser = argparse.ArgumentParser()
    parser.add_argument('--skip-native', action='store_true')
    parser.add_argument('--skip-build', action='store_true')
    parser.add_argument('--native-only', action='store_true')
    args = parser.parse_args()
    TARGET.mkdir(parents=True, exist_ok=True)
    if not args.skip_native:
        subprocess.run(['cargo', 'build', '--offline', '--quiet', '--locked', '-p', 'zz-ui', '--no-default-features', '--features', 'scene-fixtures', '--example', 'native_truncate_vectors'], cwd=ROOT, check=True)
        subprocess.run([str(ROOT / 'target/debug/examples/native_truncate_vectors'), str(TARGET / 'source.json')], cwd=ROOT, check=True)
    cases = json.loads((TARGET / 'source.json').read_text())
    characters = json.loads((TARGET / 'source.chars.json').read_text())
    if args.native_only:
        print(json.dumps({'source_cases': len(cases), 'source_scalar_widths': len(characters)}))
        return
    data, expected = bytearray(), bytearray()
    offsets = []
    uint(data, len(characters))
    for case in characters:
        for field in ('font', 'size', 'scalar'):
            uint(data, case[field])
        offsets.append(len(expected))
        text(expected, case['expected'])
    uint(data, len(cases))
    for case in cases:
        for field in ('font', 'size', 'width'):
            uint(data, case[field])
        text(data, case['text'])
        offsets.append(len(expected))
        text(expected, case['expected'])
    (TARGET / 'input.bin').write_bytes(data)
    binary = TARGET / 'probe'
    if not args.skip_build:
        subprocess.run(['pixi', 'run', 'bash', '-c', 'mojo build --fp-mode=contract=off -I src -I "$1/mojo" -D MJ_RASTER=swash -Xlinker build/lib/libmjswash.a $MJ_LINK -Xlinker build/lib/libSDL3.a -Xlinker -lgcc_s -Xlinker -ldl -Xlinker -lrt -Xlinker -lutil "$1/mojo/tests/native_truncate_probe.mojo" -o "$2"', '_', str(ROOT), str(binary)], cwd=MJUI, check=True)
    subprocess.run([str(binary), str(TARGET / 'input.bin'), str(TARGET / 'output.bin')], cwd=MJUI, check=True, timeout=60)
    actual = (TARGET / 'output.bin').read_bytes()
    records = characters + cases
    if actual != expected:
        first = next((index for index, pair in enumerate(zip(actual, expected)) if pair[0] != pair[1]), min(len(actual), len(expected)))
        index = max(bisect.bisect_right(offsets, first) - 1, 0)
        cursor, length, shift = offsets[index], 0, 0
        while cursor < len(actual):
            byte = actual[cursor]
            cursor += 1
            length |= (byte & 127) << shift
            if byte < 128:
                break
            shift += 7
        result = actual[cursor:cursor + length].decode('utf-8')
        raise AssertionError(f'native end truncation differs at record {index}, byte {first}: {records[index]!r}, actual={result!r}')
    report = {'passed': True, 'cases': len(cases), 'scalar_widths': len(characters), 'exact_bytes': len(actual), 'source': 'actual ala LineWrapper.truncate_line End with Inter and JetBrains Mono at 10/11/13px', 'scope': 'scalar width budgeting, strict suffix boundary, floor overflow, trailing Rust whitespace and ASCII punctuation, full-prefix reshaping'}
    (TARGET / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report))


if __name__ == '__main__':
    main()
