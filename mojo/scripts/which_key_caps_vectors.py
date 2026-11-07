import argparse
import json
from pathlib import Path
import runpy
import subprocess

from palette_places_vectors import uint, text


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / 'target/port/which-key-caps'


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    parser = argparse.ArgumentParser()
    parser.add_argument('--skip-reference', action='store_true')
    parser.add_argument('--skip-build', action='store_true')
    parser.add_argument('--update-data', action='store_true')
    args = parser.parse_args()
    TARGET.mkdir(parents=True, exist_ok=True)
    if not args.skip_reference:
        subprocess.run(['cargo', 'run', '--quiet', '--locked', '--offline', '-p', 'zz-ui', '--no-default-features', '--features', 'scene-fixtures', '--example', 'which_key_caps_vectors', '--', str(TARGET)], cwd=ROOT, check=True)
    generated = '\n'.join('comptime WHICH_KEY_' + name.upper() + ': StaticString = ' + json.dumps((TARGET / (name + '.txt')).read_text()) + '\n' for name in ('alphanumeric', 'cased', 'ignorable'))
    path = ROOT / 'mojo/zzmojo/which_key_caps_data.mojo'
    if args.update_data:
        path.write_text(generated)
    if path.read_text() != generated:
        raise AssertionError('source Unicode alphanumeric ranges changed; rerun with --update-data')
    cases = json.loads((TARGET / 'caps.json').read_text())
    data = bytearray()
    uint(data, len(cases))
    for case in cases:
        text(data, case['input'])
        value = case['value']
        uint(data, value is not None)
        if value is not None:
            text(data, value['key'])
            uint(data, value['modifiers'])
            uint(data, value['key_char'] is not None)
            if value['key_char'] is not None:
                text(data, value['key_char'])
            text(data, value['text'])
    (TARGET / 'input.bin').write_bytes(data)
    binary = TARGET / 'probe'
    if not args.skip_build:
        subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '-I', str(ROOT / 'mojo'), '-I', str(ROOT.parent / 'mjui/src'), str(ROOT / 'mojo/tests/which_key_caps_probe.mojo'), '-o', str(binary)], cwd=ROOT, check=True)
    completed = subprocess.run([str(binary), str(TARGET / 'input.bin')], cwd=ROOT, capture_output=True, text=True, timeout=120)
    if completed.returncode:
        raise RuntimeError(completed.stdout[-6000:] + completed.stderr[-6000:])
    amount, accepted = map(int, completed.stdout.split())
    report = {'passed': True, 'cases': amount, 'accepted': accepted, 'rejected': amount - accepted, 'bytes': len(data), 'source': 'actual ChromeKey::parse, ala_source, Keystroke::parse, Kbd::format_key(lowercase=true); source Unicode alphanumeric ranges'}
    (TARGET / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report))


if __name__ == '__main__':
    main()
