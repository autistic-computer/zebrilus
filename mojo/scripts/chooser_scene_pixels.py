import argparse
import json
from pathlib import Path
import runpy
import subprocess

from PIL import Image, ImageChops

from chooser_scene_reference import DIRECTORY, MJUI, ROOT


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    parser = argparse.ArgumentParser()
    parser.add_argument('--skip-native', action='store_true')
    parser.add_argument('--skip-build', action='store_true')
    parser.add_argument('--diagnose', action='store_true')
    parser.add_argument('--theme', type=int, choices=range(6), action='append')
    parser.add_argument('--scale', type=float, choices=[1, 1.25, 2], action='append')
    parser.add_argument('--variant', type=int, choices=range(5), action='append')
    args = parser.parse_args()
    if not args.skip_native:
        subprocess.run(['python3', str(ROOT / 'mojo/scripts/chooser_scene_reference.py')], cwd=ROOT, check=True)
    binary = ROOT / 'target/port/chooser-scene-probe'
    if not args.skip_build:
        subprocess.run(['pixi', 'run', 'bash', '-c', 'mojo build --fp-mode=contract=off -I src -I scripts/penna -I "$1/mojo" -D MJ_RASTER=swash -Xlinker build/lib/libmjswash.a $MJ_LINK -Xlinker build/lib/libSDL3.a -Xlinker -lgcc_s -Xlinker -ldl -Xlinker -lrt -Xlinker -lutil "$1/mojo/chooser_scene_probe.mojo" -o "$2"', '_', str(ROOT), str(binary)], cwd=MJUI, check=True)
    records = []
    for theme in args.theme or range(6):
        for scale in args.scale or [1, 1.25, 2]:
            for variant in args.variant or range(5):
                directory = DIRECTORY / f'theme-{theme}-scale-{scale:g}-variant-{variant}'
                subprocess.run([str(binary), str(directory / 'input.bin'), str(directory / 'mojo.png')], cwd=MJUI, check=True, timeout=60)
                with Image.open(directory / 'native.png') as image:
                    expected = image.convert('RGBA')
                with Image.open(directory / 'mojo.png') as image:
                    actual = image.convert('RGBA')
                assert actual.size == expected.size
                difference = ImageChops.difference(actual, expected)
                bands = difference.split()
                mask = bands[0]
                for band in bands[1:]:
                    mask = ImageChops.lighter(mask, band)
                different = mask.width * mask.height - mask.histogram()[0]
                record = {'theme': theme, 'scale': scale, 'variant': variant, 'pixels': mask.width * mask.height, 'different': different, 'max_error': max(band.getextrema()[1] for band in bands), 'exact': different == 0}
                if different:
                    record['difference_bounds'] = list(mask.getbbox())
                records.append(record)
                print(json.dumps(record), flush=True)
    report = {'cases': records, 'exact': all(item['exact'] for item in records)}
    filtered = args.theme or args.scale or args.variant
    (DIRECTORY / ('report-filtered.json' if filtered else 'report.json')).write_text(json.dumps(report, indent=2) + '\n')
    if not report['exact'] and not args.diagnose:
        raise SystemExit('native chooser pixel identity is incomplete')


if __name__ == '__main__':
    main()
