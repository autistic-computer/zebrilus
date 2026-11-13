import argparse
import json
from pathlib import Path
import runpy
import subprocess

from PIL import Image, ImageChops

ROOT = Path(__file__).resolve().parents[2]
MJUI = ROOT.parent / 'mjui'
TARGET = ROOT / 'target/port/browser-chrome'
FIXTURE = ['cargo', 'build', '--quiet', '--offline', '--locked', '-p', 'zz-ui', '--no-default-features', '--features', 'scene-fixtures', '--example', 'browser_chrome_fixture']
BUILD = 'mojo build --fp-mode=contract=off -j 1 -I src -I "$1/mojo" -D MJ_RASTER=swash -Xlinker build/lib/libmjswash.a $MJ_LINK -Xlinker build/lib/libSDL3.a -Xlinker -lgcc_s -Xlinker -ldl -Xlinker -lrt -Xlinker -lutil "$1/mojo/tests/browser_chrome_probe.mojo" -o "$2"'
NAMES = ['page', 'tabs', 'empty', 'recents', 'error', 'picking']


def compare(directory):
    with Image.open(directory / 'native.png') as source:
        expected = source.convert('RGBA')
    with Image.open(directory / 'mojo.png') as result:
        actual = result.convert('RGBA')
    assert actual.size == expected.size
    bands = ImageChops.difference(actual, expected).split()
    mask = bands[0]
    for band in bands[1:]:
        mask = ImageChops.lighter(mask, band)
    different = mask.width * mask.height - mask.histogram()[0]
    record = {'pixels': expected.width * expected.height, 'different': different, 'exact': different == 0, 'max_error': max(band.getextrema()[1] for band in bands)}
    if different:
        record['difference_bounds'] = list(mask.getbbox())
    return record


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    parser = argparse.ArgumentParser()
    parser.add_argument('--skip-native', action='store_true')
    parser.add_argument('--skip-build', action='store_true')
    parser.add_argument('--diagnose', action='store_true')
    parser.add_argument('--theme', type=int, choices=range(6), action='append')
    parser.add_argument('--scale', type=float, choices=[1, 1.25, 2], action='append')
    parser.add_argument('--variant', type=int, choices=range(len(NAMES)), action='append')
    args = parser.parse_args()
    cases = [(theme, scale, variant) for theme in args.theme or range(6) for scale in args.scale or [1, 1.25, 2] for variant in args.variant or range(len(NAMES))]
    if not args.skip_native:
        subprocess.run(FIXTURE, cwd=ROOT, check=True)
        for theme, scale, variant in cases:
            directory = TARGET / f'theme-{theme}-scale-{scale:g}-variant-{variant}'
            subprocess.run([str(ROOT / 'target/debug/examples/browser_chrome_fixture'), str(directory), str(theme), str(scale), str(variant)], cwd=ROOT, check=True, timeout=60)
    binary = ROOT / 'target/port/browser-chrome-probe'
    if not args.skip_build:
        subprocess.run(['pixi', 'run', 'bash', '-c', BUILD, '_', str(ROOT), str(binary)], cwd=MJUI, check=True)
    records = []
    for theme, scale, variant in cases:
        directory = TARGET / f'theme-{theme}-scale-{scale:g}-variant-{variant}'
        result = subprocess.run([str(binary), str(theme), f'{scale:g}', str(variant), str(directory / 'mojo.png')], cwd=MJUI, check=True, capture_output=True, text=True, timeout=60)
        record = {'theme': theme, 'scale': scale, 'variant': variant, 'state': NAMES[variant], 'replayed': result.stdout.split()[-1] == '1'}
        record.update(compare(directory))
        records.append(record)
    exact = {name: [item['exact'] and item['replayed'] for item in records if item['state'] == name] for name in NAMES}
    report = {'cases': records, 'exact': {name: bool(values) and all(values) for name, values in exact.items()}}
    TARGET.mkdir(parents=True, exist_ok=True)
    filtered = args.theme or args.scale or args.variant
    (TARGET / ('report-filtered.json' if filtered else 'report.json')).write_text(json.dumps(report, indent=2) + '\n')
    failures = [item for item in records if not item['exact'] or not item['replayed']]
    print(json.dumps({'cases': len(records), 'pixels': sum(item['pixels'] for item in records), 'exact_cases': {name: f'{sum(values)}/{len(values)}' for name, values in exact.items()}, 'failures': failures[:8]}), flush=True)
    if failures and not args.diagnose:
        raise SystemExit('the browser chrome differs from the source render')


if __name__ == '__main__':
    main()
