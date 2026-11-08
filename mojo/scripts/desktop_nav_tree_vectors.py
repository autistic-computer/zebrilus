import json
from pathlib import Path
import runpy
import subprocess

from palette_dialog_vectors import uint, blob

ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / 'target/port'


def optional(output, value):
    output.append(value is not None)
    if value is not None:
        uint(output, value)


def node(output, value):
    uint(output, value[0])
    output.append(value[1])
    uint(output, value[2])


def write_hosts(output, hosts):
    uint(output, len(hosts))
    for host in hosts:
        uint(output, host['id'])
        output.append(host['connected'])
        uint(output, len(host['sessions']))
        for session in host['sessions']:
            uint(output, session['id'])
            output.append(session['active'])
            blob(output, session['name'].encode())
            uint(output, len(session['windows']))
            for window in session['windows']:
                uint(output, window['id'])
                output.append(window['active'])
                uint(output, window['pane'])
                blob(output, window['name'].encode())
                uint(output, len(window['panes']))
                for pane in window['panes']:
                    uint(output, pane)


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    reference = TARGET / 'desktop-nav-tree-vectors.json'
    subprocess.run(['cargo', 'run', '--quiet', '--locked', '--offline', '-p', 'zz-client', '--example', 'desktop_nav_tree_vectors', '--', str(reference)], cwd=ROOT, check=True)
    data = json.loads(reference.read_text())
    output = bytearray()
    uint(output, len(data['cases']))
    patterns = queries = 0
    for case in data['cases']:
        write_hosts(output, case['hosts'])
        uint(output, len(case['queries']))
        queries += len(case['queries'])
        for query in case['queries']:
            uint(output, query['host_index'])
            output.append(query['kind'])
            uint(output, query['id'])
            output.append(query['owner'] is not None)
            if query['owner'] is not None:
                for value in query['owner']:
                    uint(output, value)
            output.append(query['name'] is not None)
            if query['name'] is not None:
                blob(output, query['name'].encode())
            output.append(query['expandable'])
        uint(output, len(case['shortcuts']))
        for host, window in case['shortcuts']:
            uint(output, host)
            uint(output, window)
        uint(output, len(case['patterns']))
        patterns += len(case['patterns'])
        for pattern in case['patterns']:
            uint(output, len(pattern['expanded']))
            for value in pattern['expanded']:
                node(output, value)
            uint(output, len(pattern['entries']))
            for entry in pattern['entries']:
                node(output, entry['node'])
                output.append(entry['parent'] is not None)
                if entry['parent'] is not None:
                    node(output, entry['parent'])
                for value in entry['location']:
                    optional(output, value)
                output.extend((entry['depth'], entry['expandable'], entry['expanded'], entry['active']))
            uint(output, len(pattern['indices']))
            for value, index in pattern['indices']:
                node(output, value)
                uint(output, index)
            uint(output, len(pattern['expandable']))
            for value in pattern['expandable']:
                node(output, value)
            uint(output, len(pattern['navigation']))
            for selected, direction, value in pattern['navigation']:
                optional(output, selected)
                output.extend((direction, value[0]))
                uint(output, value[1])
    fixture = TARGET / 'desktop-nav-tree-vectors.bin'
    fixture.write_bytes(output)
    probe = TARGET / 'desktop-nav-tree-probe'
    subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '-I', str(ROOT / 'mojo'), str(ROOT / 'mojo/tests/desktop_nav_tree_probe.mojo'), '-o', str(probe)], cwd=ROOT, check=True)
    subprocess.run([str(probe), str(fixture)], cwd=ROOT, check=True, timeout=60)
    report = {'passed': True, 'models': len(data['cases']), 'patterns': patterns, 'target_queries': queries, 'visible_rows': data['rows'], 'navigation': data['navigation'], 'bytes': len(output), 'live_hook': False}
    (TARGET / 'desktop-nav-tree-verification.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report), flush=True)


if __name__ == '__main__':
    main()
