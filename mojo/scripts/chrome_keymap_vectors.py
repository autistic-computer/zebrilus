import argparse
import json
import os
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile
import time


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / 'target/port'
sys.path.insert(0, str(ROOT / 'scripts/port'))
from chrome_catalog import write_catalog
from chrome_packets import write_packets


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    parser = argparse.ArgumentParser()
    parser.add_argument('--skip-build', action='store_true')
    parser.add_argument('--skip-reference', action='store_true')
    options = parser.parse_args()
    TARGET.mkdir(parents=True, exist_ok=True)
    reference = TARGET / 'chrome-keymap-vectors.json'
    if not options.skip_reference:
        subprocess.run(['cargo', 'run', '--quiet', '--locked', '--offline', '-p', 'zz-client', '--example', 'chrome_keymap_vectors', '--', str(reference)], cwd=ROOT, check=True)
    if reference.stat().st_size > 64 * 1024 * 1024:
        raise RuntimeError('chrome source corpus exceeds 64 MiB')
    source = json.loads(reference.read_text())
    write_catalog(source, ROOT)
    probe = TARGET / 'chrome-keymap-conformance'
    budget = TARGET / 'chrome-keymap-budget'
    lifecycle = TARGET / 'chrome-router-lifecycle'
    if not options.skip_build:
        kit = runpy.run_path(str(ROOT / 'bend2/toolchain.py'))['ensure']()
        environment = os.environ.copy()
        environment['PATH'] = '/usr/bin:' + environment['PATH']
        temporary = TARGET / 'zz-bend-input.next'
        subprocess.run([kit['bend'], 'bend2/input_router_bridge.bend', '-o', str(temporary)], cwd=ROOT, env=environment, check=True)
        temporary.replace(TARGET / 'zz-bend-input')
        subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '-I', str(ROOT / 'mojo'), str(ROOT / 'mojo/tests/chrome_keymap_conformance_probe.mojo'), '-o', str(probe)], cwd=ROOT, check=True)
        for path, probe_source in [(budget, 'chrome_keymap_budget_probe.mojo'), (lifecycle, 'chrome_router_lifecycle_probe.mojo')]:
            subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '-I', str(ROOT / 'mojo'), str(ROOT / 'mojo/tests' / probe_source), '-o', str(path)], cwd=ROOT, check=True)
    subprocess.run([str(budget)], cwd=ROOT, check=True, timeout=60)
    for mode in [[], [str(TARGET / 'zz-bend-input')]]:
        subprocess.run([str(lifecycle), *mode], cwd=ROOT, check=True, timeout=60)
    with tempfile.TemporaryDirectory(prefix='zz-chrome-') as directory:
        write_packets(source, Path(directory))
        started = time.monotonic()
        result = subprocess.run([str(probe), directory], cwd=ROOT, capture_output=True, timeout=300)
        if result.returncode:
            raise RuntimeError(result.stderr.decode()[-8000:])
        keys = ['typed_actions', 'action_names', 'parsed_chords', 'normalized_chords', 'wire_inputs', 'map_cases', 'mutation_steps', 'resolve_queries']
        report = dict(zip(keys, map(int, result.stdout.split()), strict=True))
        report.update({'passed': True, 'seconds': round(time.monotonic() - started, 3), 'source_bytes': reference.stat().st_size})
        (TARGET / 'chrome-keymap-verification.json').write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps(report), flush=True)


if __name__ == '__main__':
    main()
