import json
from pathlib import Path
import runpy
import subprocess

from desktop_nav_tree_vectors import write_hosts, node, optional
from palette_dialog_vectors import uint

ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / 'target/port'


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    tree = TARGET / 'desktop-nav-tree-vectors.json'
    reference = TARGET / 'desktop-sidebar-vectors.json'
    subprocess.run(['cargo', 'run', '--quiet', '--locked', '--offline', '-p', 'zz-client', '--example', 'desktop_nav_tree_vectors', '--', str(tree)], cwd=ROOT, check=True)
    subprocess.run(['cargo', 'run', '--quiet', '--locked', '--offline', '-p', 'zz-client', '--example', 'desktop_sidebar_vectors', '--', str(tree), str(reference)], cwd=ROOT, check=True)
    models = json.loads(tree.read_text())['cases']
    data = json.loads(reference.read_text())
    output = bytearray()
    uint(output, len(models))
    for model in models:
        write_hosts(output, model['hosts'])
    uint(output, len(data))
    transitions = 0
    for trace in data:
        uint(output, len(trace))
        transitions += len(trace)
        for event in trace:
            value = event['input']
            op = value[0]
            output.append(op)
            if op == 0:
                uint(output, value[1])
                uint(output, value[2])
                optional(output, value[3])
                uint(output, value[4])
            elif op in (3, 5):
                uint(output, value[1])
            else:
                node(output, value[1])
            for key in ('selected', 'active'):
                output.append(event[key] is not None)
                if event[key] is not None:
                    node(output, event[key])
            output.append(event['pointer'])
            uint(output, len(event['expanded']))
            for item in event['expanded']:
                node(output, item)
            optional(output, event['scroll'])
            uint(output, len(event['rows']))
            for item in event['rows']:
                node(output, item)
    fixture = TARGET / 'desktop-sidebar-vectors.bin'
    fixture.write_bytes(output)
    probe = TARGET / 'desktop-sidebar-probe'
    subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '-I', str(ROOT / 'mojo'), '-I', str(ROOT / 'mojo/tests'), str(ROOT / 'mojo/tests/desktop_sidebar_probe.mojo'), '-o', str(probe)], cwd=ROOT, check=True)
    subprocess.run([str(probe), str(fixture)], cwd=ROOT, check=True, timeout=60)
    report = {'passed': True, 'models': len(models), 'traces': len(data), 'transitions': transitions, 'bytes': len(output), 'source': 'actual delegated sidebar reconcile and path helpers plus actual tree_navigation', 'live_hook': False}
    (TARGET / 'desktop-sidebar-verification.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report), flush=True)


if __name__ == '__main__':
    main()
