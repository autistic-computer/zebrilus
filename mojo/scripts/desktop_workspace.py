import argparse
import json
from pathlib import Path
import runpy
import subprocess
import time


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / 'target/port'


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    parser = argparse.ArgumentParser()
    parser.add_argument('--skip-build', action='store_true')
    options = parser.parse_args()
    probe = TARGET / 'desktop-workspace-probe'
    if not options.skip_build:
        subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '-I', str(ROOT / 'mojo'), str(ROOT / 'mojo/tests/desktop_workspace_probe.mojo'), '-o', str(probe)], cwd=ROOT, check=True)
    reports = []
    for mode, arguments in [('typed', []), ('native', [str(TARGET / 'zz-bend-input')]), ('main', [str(TARGET / 'zz-bend')])]:
        started = time.monotonic()
        result = subprocess.run(['/usr/bin/time', '-f', 'WORKSPACE_RSS_KIB %M', str(probe), *arguments], cwd=ROOT, capture_output=True, text=True, timeout=60)
        if result.returncode:
            raise RuntimeError(result.stdout[-4000:] + result.stderr[-8000:])
        native, fallback = map(int, result.stdout.split())
        if mode in ('native', 'main'):
            assert native > 0 and fallback == 0
        report = {'mode': mode, 'native': native, 'fallback': fallback, 'seconds': round(time.monotonic() - started, 3)}
        for line in result.stderr.splitlines():
            if line.startswith('WORKSPACE_RSS_KIB '):
                report['maximum_rss_kib'] = int(line.split()[1])
        reports.append(report)
        print(json.dumps(report), flush=True)
    (TARGET / 'desktop-workspace-verification.json').write_text(json.dumps({'passed': True, 'profiles': reports}, indent=2) + '\n')


if __name__ == '__main__':
    main()
