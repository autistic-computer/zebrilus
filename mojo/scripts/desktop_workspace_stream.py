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
    probe = TARGET / 'desktop-workspace-stream-probe'
    subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '-I', str(ROOT / 'mojo'), str(ROOT / 'mojo/tests/desktop_workspace_stream_probe.mojo'), '-o', str(probe)], cwd=ROOT, check=True)
    cli = ROOT / 'target/debug/zz_cli'
    environment = os.environ.copy()
    environment['ZZ_BEND'] = str(TARGET / 'zz-bend')
    with tempfile.TemporaryDirectory(prefix='zz-desktop-stream-', dir='/tmp') as directory:
        socket = str(Path(directory) / 's')
        try:
            subprocess.run([str(cli), '-S', socket, '-f', '/dev/null', 'new-session', '-d', '-s', 'desktop-routing', 'cat'], env=environment, check=True, capture_output=True, timeout=30)
            subprocess.run([str(probe), socket, str(TARGET / 'zz-bend')], env=environment, cwd=ROOT, timeout=60, check=True)
        finally:
            subprocess.run([str(cli), '-S', socket, 'kill-server'], env=environment, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10)
    (TARGET / 'desktop-workspace-stream.json').write_text(json.dumps({'passed': True, 'source': 'actual private daemon and shared verified worker', 'scope': ['real prefix press and subscription', 'configured prefix chrome key intercepted by source policy', 'server cancellation acknowledgement', 'suppressed chrome release', 'bound picker key forwarded to the pane and opened there', 'healthy native execution with no fallback', 'authoritative target independent of optimistic selection'], 'live_app_hook': False}, indent=2) + '\n')


if __name__ == '__main__':
    main()
