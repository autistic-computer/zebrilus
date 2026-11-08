import json
from pathlib import Path
import runpy
import subprocess

from palette_dialog_vectors import uint, blob


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / 'target/port'


def text(output, value):
    blob(output, value.encode())


def optional(output, value):
    output.append(value is not None)
    if value is not None:
        uint(output, value)


def frames(output, values):
    uint(output, len(values))
    for frame in values:
        blob(output, frame)


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    reference = TARGET / 'desktop-nav-vectors.json'
    subprocess.run(['cargo', 'run', '--quiet', '--locked', '--offline', '-p', 'zz-client', '--example', 'desktop_nav_vectors', '--', str(reference)], cwd=ROOT, check=True)
    data = json.loads(reference.read_text())
    output = bytearray()
    uint(output, len(data['activations']))
    for case in data['activations']:
        uint(output, case['host'])
        output.append(case['kind'])
        uint(output, case['id'])
        optional(output, case['owner_session'])
        optional(output, case['owner_window'])
        uint(output, case['attached_host'])
        optional(output, case['attached_session'])
        output.extend((case['connected'], case['value'] is not None))
        if case['value'] is not None:
            value = case['value']
            output.append(value['kind'])
            uint(output, value['host'])
            optional(output, value['session'])
            text(output, value['target'])
            frames(output, value['frames'])
    uint(output, len(data['sessions']))
    for case in data['sessions']:
        output.append(case['local'])
        uint(output, len(case['capabilities']))
        for capability in case['capabilities']:
            text(output, capability)
        output.append(case['directory'] is not None)
        if case['directory'] is not None:
            blob(output, case['directory'])
            text(output, case['lossy'])
        frames(output, case['frames'])
    uint(output, len(data['commands']))
    for case in data['commands']:
        output.append(case['kind'])
        uint(output, case['id'])
        frames(output, case['frames'])
    fixture = TARGET / 'desktop-nav-vectors.bin'
    fixture.write_bytes(output)
    probe = TARGET / 'desktop-nav-probe'
    subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '-I', str(ROOT / 'mojo'), str(ROOT / 'mojo/tests/desktop_nav_probe.mojo'), '-o', str(probe)], cwd=ROOT, check=True)
    subprocess.run([str(probe), str(fixture)], cwd=ROOT, check=True, timeout=60)
    report = {'passed': True, 'activations': len(data['activations']), 'new_sessions': len(data['sessions']), 'command_batches': len(data['commands']), 'bytes': len(output), 'source': 'actual desktop navigation/Mux/AppShell delegated command helpers and native wire encoder', 'live_hook': False}
    (TARGET / 'desktop-nav-verification.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report), flush=True)


if __name__ == '__main__':
    main()
