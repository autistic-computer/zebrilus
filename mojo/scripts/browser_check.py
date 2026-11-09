import json
from pathlib import Path
import runpy
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    for name in ['browser_state_vectors.py', 'browser_host_vectors.py', 'browser_controller_check.py', 'browser_actions_check.py', 'browser_wire_vectors.py']:
        subprocess.run(['python3', str(ROOT / 'mojo/scripts' / name)], cwd=ROOT, check=True)
    subprocess.run(['python3', str(ROOT.parent / 'mjui/scripts/web/zoom_check.py')], cwd=ROOT, check=True)
    commands = [
        ['cargo', 'clippy', '--locked', '--offline', '-p', 'zz-client', '--example', 'browser_state_vectors', '--example', 'browser_host_vectors', '--', '-D', 'warnings'],
        ['cargo', 'clippy', '--locked', '--offline', '-p', 'zz-protocol', '--example', 'browser_port_vectors', '--', '-D', 'warnings'],
        ['cargo', 'clippy', '--locked', '--offline', '--manifest-path', str(ROOT / 'mojo/native/browser-url/Cargo.toml'), '--lib', '--', '-D', 'warnings'],
    ]
    for command in commands:
        subprocess.run(command, cwd=ROOT, check=True)
    report = {key: json.loads((ROOT / 'target/port' / name).read_text()) for key, name in [('state', 'browser-state-verification.json'), ('source', 'browser-host-verification.json'), ('wire', 'browser-wire-verification.json')]}
    report.update({'passed': True, 'live_cef': False, 'metadata_controller': 'focused native-source-derived scenarios and transactional refusals'})
    (ROOT / 'target/port/browser-verification.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report), flush=True)


if __name__ == '__main__':
    main()
