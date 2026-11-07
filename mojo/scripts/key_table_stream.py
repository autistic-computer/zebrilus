import json
import os
from pathlib import Path
import runpy
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / 'target/port'


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    binary = TARGET / 'key-table-stream-probe'
    subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '-I', str(ROOT / 'mojo'), str(ROOT / 'mojo/tests/key_table_stream_probe.mojo'), '-o', str(binary)], cwd=ROOT, check=True)
    cli = ROOT / 'target/debug/zz_cli'
    environment = os.environ.copy()
    environment['ZZ_BEND'] = str(TARGET / 'zz-bend')
    with tempfile.TemporaryDirectory(prefix='zz-key-tables-', dir='/tmp') as directory:
        socket = str(Path(directory) / 's')
        try:
            subprocess.run([str(cli), '-S', socket, '-f', '/dev/null', 'new-session', '-d', '-s', 'keys-proof', 'sleep 120'], env=environment, check=True, capture_output=True, timeout=30)
            subprocess.run([str(binary), socket], env=environment, cwd=ROOT, check=True, timeout=60)
            (TARGET / 'key-table-stream.json').write_text(json.dumps({'passed': True, 'source': 'actual private daemon', 'scope': ['dynamic binds/unbinds', 'full/patch/hash/active retention', 'shared immutable AppModel snapshots', 'source-valid oversize refusal', 'GetKeyTables resync', 'handled metadata does not enter raw event queue']}, indent=2) + '\n')
        finally:
            subprocess.run([str(cli), '-S', socket, 'kill-server'], env=environment, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10)


if __name__ == '__main__':
    main()
