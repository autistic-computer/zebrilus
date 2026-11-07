import json
from pathlib import Path
import runpy
import subprocess

ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / 'target/port'


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    TARGET.mkdir(parents=True, exist_ok=True)
    manifest = ROOT / 'mojo/native/browser-url/Cargo.toml'
    subprocess.run(['cargo', 'build', '--locked', '--offline', '--release', '--manifest-path', str(manifest)], cwd=ROOT, check=True)
    source = TARGET / 'browser-state-vectors.bin'
    subprocess.run(['cargo', 'run', '--quiet', '--locked', '--offline', '-p', 'zz-client', '--example', 'browser_state_vectors', '--', str(source)], cwd=ROOT, check=True)
    output = TARGET / 'browser-state-probe'
    library = ROOT / 'mojo/native/browser-url/target/release/libzzbrowserurl.a'
    subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '--fp-mode=contract=off', '-I', str(ROOT / 'mojo'), '-I', str(ROOT / 'mojo/tests'), str(ROOT / 'mojo/tests/browser_state_probe.mojo'), '-Xlinker', str(library), '-Xlinker', '-ldl', '-Xlinker', '-lpthread', '-Xlinker', '-lm', '-o', str(output)], cwd=ROOT, check=True)
    result = subprocess.run([str(output), str(source)], cwd=ROOT, check=True, capture_output=True, text=True, timeout=30)
    urls, profiles, descriptors, switches, errors = map(int, result.stdout.split())
    report = {'passed': True, 'url_cases': urls, 'profile_cases': profiles, 'descriptors': descriptors, 'tab_switches': switches, 'load_errors': errors, 'bytes': source.stat().st_size, 'source': 'actual URL parser and browser tab restoration/metadata/neighbor/cycle helpers used by desktop BrowserView', 'live_browser': False}
    (TARGET / 'browser-state-verification.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report), flush=True)


if __name__ == '__main__':
    main()
