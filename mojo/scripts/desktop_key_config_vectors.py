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


def override(output, value):
    output.append(value['bind'])
    for key in ('table', 'key', 'action'):
        text(output, value[key])


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    source = TARGET / 'desktop-key-config-vectors.json'
    subprocess.run(['cargo', 'run', '--quiet', '--locked', '--offline', '-p', 'zz-config', '--example', 'desktop_key_config_vectors', '--', str(source)], cwd=ROOT, check=True)
    data = json.loads(source.read_text())
    output = bytearray([data['macos']])
    uint(output, len(data['strokes']))
    for case in data['strokes']:
        text(output, case['text'])
        value = case['value']
        output.append(value is not None)
        if value is not None:
            text(output, value['key'])
            output.append(value['function'])
            text(output, value['spelling'])
    uint(output, len(data['overrides']))
    for case in data['overrides']:
        text(output, case['text'])
        output.extend((case['bind'], case['value'] is not None))
        if case['value'] is not None:
            override(output, case['value'])
        else:
            text(output, case['error'])
    uint(output, len(data['configs']))
    for case in data['configs']:
        text(output, case['source'])
        text(output, case['hotkey'])
        output.append(case['picker'])
        uint(output, case['delay'])
        uint(output, len(case['overrides']))
        for value in case['overrides']:
            override(output, value)
        uint(output, len(case['diagnostics']))
        for line, message in case['diagnostics']:
            uint(output, line)
            text(output, message)
        uint(output, len(case['bindings']))
        for value in case['bindings']:
            for field in value:
                text(output, field)
    text(output, data['identity'])
    uint(output, len(data['candidates']))
    for case in data['candidates']:
        output.append(case['platform'])
        for value in case['environment']:
            output.append(value is not None)
            if value is not None:
                text(output, value)
        uint(output, len(case['paths']))
        for value in case['paths']:
            text(output, value)
    probe = TARGET / 'desktop-key-config-probe'
    subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '-I', str(ROOT / 'mojo'), str(ROOT / 'mojo/tests/desktop_key_config_probe.mojo'), '-o', str(probe)], cwd=ROOT, check=True)
    with tempfile.TemporaryDirectory(prefix='zz-desktop-key-config-') as temporary:
        path = Path(temporary) / 'input.bin'
        path.write_bytes(output)
        result = subprocess.run([str(probe), str(path)], cwd=ROOT, capture_output=True, text=True, timeout=60)
        if result.returncode:
            raise RuntimeError(result.stdout[-4000:] + result.stderr[-8000:])
    io_probe = TARGET / 'desktop-key-config-io-probe'
    subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '-I', str(ROOT / 'mojo'), str(ROOT / 'mojo/tests/desktop_key_config_io_probe.mojo'), '-o', str(io_probe)], cwd=ROOT, check=True)
    with tempfile.TemporaryDirectory(prefix='zz-desktop-config-') as directory:
        path = Path(directory)
        (path / 'xdg/zz/config').mkdir(parents=True)
        (path / 'home/.config/zz').mkdir(parents=True)
        (path / 'home/.config/zz/config').write_text('chrome-keybind = terminal:C-x=terminal-copy\npicker-focus-sidebar = true\nwhich-key-delay = 125\n')
        (path / 'exact').write_bytes(b'x' * 65536)
        (path / 'oversize').write_bytes(b'x' * 65537)
        (path / 'invalid').write_bytes(b'\xff')
        subprocess.run([str(io_probe), directory], cwd=ROOT, check=True, timeout=30)
    report = {'passed': True, 'strokes': len(data['strokes']), 'overrides': len(data['overrides']), 'configs': len(data['configs']), 'bytes': len(output), 'candidate_environments': len(data['candidates']), 'bounded_file_io': True, 'source': 'actual zz-config parser and configured_keymap'}
    (TARGET / 'desktop-key-config-verification.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report), flush=True)


if __name__ == '__main__':
    main()
