import argparse
import json
from pathlib import Path
import runpy
import subprocess

from palette_dialog_vectors import uint, blob


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / 'target/port'


def text(output, value):
    blob(output, value.encode())


def key(output, value):
    text(output, value['key'])
    output.extend((value['modifiers'], value['function'], value['character'] is not None))
    if value['character'] is not None:
        text(output, value['character'])


def bound(output, rows):
    uint(output, len(rows))
    for table, chord, code in rows:
        text(output, table)
        text(output, chord)
        uint(output, code)


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    parser = argparse.ArgumentParser()
    parser.add_argument('--skip-reference', action='store_true')
    args = parser.parse_args()
    reference = TARGET / 'native-chrome-keymap-vectors.json'
    if not args.skip_reference:
        subprocess.run(['cargo', 'run', '--quiet', '--locked', '--offline', '-p', 'zz-ui', '--example', 'native_chrome_keymap_vectors', '--', str(reference)], cwd=ROOT, check=True)
    data = json.loads(reference.read_text())
    output = bytearray((data['macos'], data['windows']))
    uint(output, len(data['parsed']))
    for case in data['parsed']:
        text(output, case['source'])
        output.append(case['value'] is not None)
        if case['value'] is not None:
            key(output, case['value'])
    uint(output, len(data['profiles']))
    queries = emitted = steps = 0
    for profile in data['profiles']:
        output.append(profile['profile'])
        uint(output, len(profile['steps']))
        for step in profile['steps']:
            steps += 1
            operation = step['operation']
            output.append(0 if operation is None else 1 if operation[2] is not None else 2)
            if operation is not None:
                text(output, operation[0])
                text(output, operation[1])
                if operation[2] is not None:
                    text(output, operation[2])
            bound(output, step['bound'])
            bound(output, step['dropped'])
            uint(output, len(step['emitted']))
            emitted += len(step['emitted'])
            for record in step['emitted']:
                output.append(record['context'])
                text(output, record['chord'])
                output.append(record['action'] is not None)
                if record['action'] is not None:
                    uint(output, record['action'])
                output.extend((record['kind'], record['index']))
                uint(output, len(record['strokes']))
                for stroke in record['strokes']:
                    key(output, stroke)
            uint(output, len(step['queries']))
            queries += len(step['queries'])
            for query in step['queries']:
                output.append(query['context'])
                uint(output, len(query['input']))
                for stroke in query['input']:
                    key(output, stroke)
                output.append(query['pending'])
                uint(output, len(query['matches']))
                for value in query['matches']:
                    uint(output, value)
    uint(output, len(data['zooms']))
    for operation, value, changed, zoom in data['zooms']:
        output.extend((operation, changed))
        uint(output, value)
        uint(output, zoom)
    fixture = TARGET / 'native-chrome-keymap-vectors.bin'
    fixture.write_bytes(output)
    probe = TARGET / 'native-chrome-keymap-probe'
    subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '--fp-mode=contract=off', '-I', str(ROOT / 'mojo'), str(ROOT / 'mojo/tests/native_chrome_keymap_probe.mojo'), '-o', str(probe)], cwd=ROOT, check=True)
    subprocess.run([str(probe), str(fixture)], cwd=ROOT, timeout=60, check=True)
    report = {'passed': True, 'parsed': len(data['parsed']), 'profiles': len(data['profiles']), 'installs': steps, 'emitted_records': emitted, 'queries': queries, 'zoom_steps': len(data['zooms']), 'bytes': len(output), 'source': 'actual delegated ChromeState helpers and Ala Keymap matching', 'live_hook': False}
    (TARGET / 'native-chrome-keymap-verification.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report), flush=True)


if __name__ == '__main__':
    main()
