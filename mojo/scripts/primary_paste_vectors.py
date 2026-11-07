import json
from pathlib import Path
import runpy
import subprocess
import tempfile

from palette_places_vectors import uint


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / 'target/port'
PORT = ROOT / 'mojo'


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    subprocess.run(['cargo', 'build', '--quiet', '--locked', '-p', 'zz-client', '--example', 'primary_paste_vectors'], cwd=ROOT, check=True)
    source = json.loads(subprocess.check_output([str(ROOT / 'target/debug/examples/primary_paste_vectors')], cwd=ROOT))
    data = bytearray()
    wanted = bytearray()
    uint(data, len(source['policy']))
    for row in source['policy']:
        uint(data, int(row['output']) | int(row['tracking']) << 1 | int(row['shift']) << 2)
        wanted.append(int(row['paste']))
    uint(data, len(source['traces']))
    steps = 0
    for trace in source['traces']:
        uint(data, len(trace))
        for row in trace:
            uint(data, row['op'])
            wanted.extend((int(row['handled']), int(row['pressed'])))
            steps += 1
    binary = TARGET / 'primary-paste-probe'
    subprocess.run(['pixi', 'run', '--manifest-path', str(PORT / 'pixi.toml'), 'mojo', 'build', '--fp-mode=contract=off', '-I', str(PORT), str(PORT / 'tests/primary_paste_probe.mojo'), '-o', str(binary)], cwd=ROOT, check=True)
    with tempfile.TemporaryDirectory(prefix='zz-primary-paste-') as directory:
        path = Path(directory)
        (path / 'input').write_bytes(data)
        subprocess.run([str(binary), str(path / 'input'), str(path / 'output')], cwd=ROOT, check=True, timeout=60)
        actual = (path / 'output').read_bytes()
    if actual != wanted:
        offset = next((index for index, pair in enumerate(zip(actual, wanted)) if pair[0] != pair[1]), min(len(actual), len(wanted)))
        raise AssertionError(f'PRIMARY source trace differs at byte {offset}; {len(actual)} != {len(wanted)}')
    report = {'passed': True, 'policies': len(source['policy']), 'traces': len(source['traces']), 'steps': steps, 'exact_bytes': len(actual), 'source': 'actual native should_paste_primary and PrimaryPasteState used by TerminalView', 'scope': 'press/release ownership, command output/tracking/Shift policy, same-batch release, one-shot payloads, stale target and 4 MiB bounds; live host routing remains separate'}
    (TARGET / 'primary-paste-differential.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report))


if __name__ == '__main__':
    main()
