import bisect
import json
from pathlib import Path
import runpy
import subprocess

ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / 'target/port'


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    TARGET.mkdir(parents=True, exist_ok=True)
    request = TARGET / 'desktop-fleet-request.bin'
    expected_path = TARGET / 'desktop-fleet-expected.bin'
    actual_path = TARGET / 'desktop-fleet-actual.bin'
    summary = json.loads(subprocess.check_output(['cargo', 'run', '--quiet', '--locked', '--offline', '-p', 'zz-client', '--example', 'desktop_fleet_vectors', '--', str(request), str(expected_path)], cwd=ROOT))
    probe = TARGET / 'desktop-fleet-probe'
    subprocess.run(['pixi', 'run', '--manifest-path', str(ROOT / 'mojo/pixi.toml'), 'mojo', 'build', '-I', str(ROOT / 'mojo'), str(ROOT / 'mojo/tests/desktop_fleet_probe.mojo'), '-o', str(probe)], cwd=ROOT, check=True)
    subprocess.run([str(probe), str(request), str(actual_path)], cwd=ROOT, check=True, timeout=300)
    expected = expected_path.read_bytes()
    actual = actual_path.read_bytes()
    if actual != expected:
        offset = next((index for index in range(min(len(actual), len(expected))) if actual[index] != expected[index]), min(len(actual), len(expected)))
        case = bisect.bisect_right(summary['ends'], offset)
        start = summary['ends'][case - 1] if case else 0
        raise AssertionError({'case': case, 'offset': offset, 'case_offset': offset - start, 'actual': actual[max(0, offset - 24):offset + 48].hex(), 'expected': expected[max(0, offset - 24):offset + 48].hex()})
    report = {'passed': True, 'cases': summary['cases'], **summary['counts'], 'bytes': summary['bytes'], 'source': 'actual zz-client desktop_fleet sessions, per-host activation, rename, bell and shortcut helpers over protocol snapshots', 'refusals': 'invalid host state and platform, 65,537 nodes, fleet byte limit'}
    (TARGET / 'desktop-fleet-verification.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report), flush=True)


if __name__ == '__main__':
    main()
