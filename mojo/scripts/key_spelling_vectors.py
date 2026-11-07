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


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    parser = argparse.ArgumentParser()
    parser.add_argument('--shared', action='store_true')
    args = parser.parse_args()
    TARGET.mkdir(parents=True, exist_ok=True)
    reference = TARGET / 'key-spelling-vectors.json'
    subprocess.run(['cargo', 'run', '--quiet', '--locked', '--offline', '-p', 'zz-client', '--example', 'key_spelling_vectors', '--', str(reference)], cwd=ROOT, check=True)
    values = json.loads(reference.read_text())
    output = bytearray()
    uint(output, len(values))
    for row in values:
        for field in ['raw', 'value']:
            data = row[field].encode()
            uint(output, len(data))
            output.extend(data)
    probe = TARGET / 'key-spelling-probe'
    subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '-I', str(ROOT / 'mojo'), str(ROOT / 'mojo/tests/key_spelling_probe.mojo'), '-o', str(probe)], cwd=ROOT, check=True)
    with tempfile.TemporaryDirectory(prefix='zz-key-spelling-') as directory:
        path = Path(directory) / 'vectors.bin'
        path.write_bytes(output)
        result = subprocess.run([str(probe), str(path), *(['shared'] if args.shared else [])], cwd=ROOT, capture_output=True, check=True)
        assert int(result.stdout) == len(values)
    print(json.dumps({'passed': True, 'cases': len(values), 'shared_input_helper': args.shared}), flush=True)


if __name__ == '__main__':
    main()
