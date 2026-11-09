import json
from pathlib import Path
import runpy
import subprocess

ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / 'target/port'


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    TARGET.mkdir(parents=True, exist_ok=True)
    source = TARGET / 'agent-logic-vectors.bin'
    subprocess.run(['cargo', 'run', '--quiet', '--locked', '--offline', '-p', 'zz-client', '--example', 'agent_logic_vectors', '--', str(source)], cwd=ROOT, check=True)
    output = TARGET / 'agent-logic-probe'
    subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '--fp-mode=contract=off', '-j', '1', '-I', str(ROOT / 'mojo'), '-I', str(ROOT / 'mojo/tests'), str(ROOT / 'mojo/tests/agent_logic_probe.mojo'), '-o', str(output)], cwd=ROOT, check=True)
    result = subprocess.run([str(output), str(source)], cwd=ROOT, check=True, capture_output=True, text=True, timeout=120)
    timeline, flips, outputs = map(int, result.stdout.split())
    report = {'passed': True, 'timeline_cases': timeline, 'flip_cases': flips, 'output_cases': outputs, 'bytes': source.stat().st_size, 'source': 'actual agent timeline folding, trace headers, plan and usage labels, flip state machine and output pane arguments from zz-client'}
    (TARGET / 'agent-logic-verification.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report), flush=True)


if __name__ == '__main__':
    main()
