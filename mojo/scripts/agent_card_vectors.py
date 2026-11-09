import json
from pathlib import Path
import runpy
import subprocess

ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / 'target/port'


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    TARGET.mkdir(parents=True, exist_ok=True)
    source = TARGET / 'agent-card-vectors.bin'
    subprocess.run(['cargo', 'run', '--quiet', '--locked', '--offline', '-p', 'zz-client', '--example', 'agent_card_vectors', '--', str(source)], cwd=ROOT, check=True)
    output = TARGET / 'agent-card-probe'
    subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '--fp-mode=contract=off', '-j', '1', '-I', str(ROOT / 'mojo'), '-I', str(ROOT / 'mojo/tests'), str(ROOT / 'mojo/tests/agent_card_probe.mojo'), '-o', str(output)], cwd=ROOT, check=True)
    result = subprocess.run([str(output), str(source)], cwd=ROOT, check=True, capture_output=True, text=True, timeout=600)
    counts = {key: int(value) for key, value in (part.split('=') for part in result.stdout.split())}
    report = {'passed': True, **{f'{key}_cases': value for key, value in counts.items()}, 'cases': sum(counts.values()), 'bytes': source.stat().st_size, 'source': 'actual question card, agent settings and composer completion logic from zz-client'}
    (TARGET / 'agent-card-verification.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report), flush=True)


if __name__ == '__main__':
    main()
