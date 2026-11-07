import argparse
import json
from pathlib import Path
import runpy
import struct
import subprocess

from PIL import Image, ImageChops

from palette_places_vectors import uint, text


ROOT = Path(__file__).resolve().parents[2]
MJUI = ROOT.parent / 'mjui'
TARGET = ROOT / 'target/port/which-key-scene'


def color_contract(colors):
    words = ''.join(f'{word:08x}' for theme in range(6) for word in colors[theme])
    return '''from ..native_color import NativeColor, color_float
from .penna_native import _hex

comptime WHICH_KEY_ACCENTS = "''' + words + '''"


def which_key_accent(theme: Int) raises -> NativeColor:
    if theme < 0 or theme >= 6:
        raise Error("unknown which-key theme")
    var values = SIMD[.float32, 8](0)
    for index in range(8):
        values[index] = color_float(_hex(WHICH_KEY_ACCENTS.as_bytes(), (theme * 8 + index) * 8, 8))
    return NativeColor.source(SIMD[.float32, 4](values[0], values[1], values[2], values[3]), SIMD[.float32, 4](values[4], values[5], values[6], values[7]))
'''


def packet(config):
    data = bytearray()
    uint(data, config['theme'])
    for field in ('width', 'height', 'scale'):
        uint(data, struct.unpack('<I', struct.pack('<f', config[field]))[0])
    text(data, config['table'])
    uint(data, config['prefix'] is not None)
    if config['prefix'] is not None:
        text(data, config['prefix'])
    uint(data, config['all'])
    uint(data, len(config['frame']))
    data.extend(config['frame'])
    return data


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    parser = argparse.ArgumentParser()
    parser.add_argument('--skip-native', action='store_true')
    parser.add_argument('--skip-native-build', action='store_true')
    parser.add_argument('--skip-build', action='store_true')
    parser.add_argument('--native-only', action='store_true')
    parser.add_argument('--generate-colors', action='store_true')
    parser.add_argument('--diagnose', action='store_true')
    parser.add_argument('--theme', type=int, choices=range(6), action='append')
    parser.add_argument('--scale', type=float, choices=[1, 1.25, 2], action='append')
    parser.add_argument('--variant', type=int, choices=range(8), action='append')
    args = parser.parse_args()
    if not args.skip_native and not args.skip_native_build:
        subprocess.run(['cargo', 'build', '--quiet', '--offline', '--locked', '-p', 'zz-ui', '--no-default-features', '--features', 'scene-fixtures', '--example', 'which_key_scene_fixture'], cwd=ROOT, check=True)
    if not args.skip_native:
        for theme in args.theme or range(6):
            for scale in args.scale or [1, 1.25, 2]:
                for variant in args.variant or range(8):
                    directory = TARGET / f'theme-{theme}-scale-{scale:g}-variant-{variant}'
                    subprocess.run([str(ROOT / 'target/debug/examples/which_key_scene_fixture'), str(directory), str(theme), str(scale), str(variant)], cwd=ROOT, check=True, timeout=60)
    colors = {theme: json.loads((TARGET / f'theme-{theme}-scale-1-variant-4/input.json').read_text())['accent'] for theme in range(6)}
    generated = color_contract(colors)
    destination = MJUI / 'src/mjui/widgets/penna_which_key_data.mojo'
    if args.generate_colors:
        destination.write_text(generated)
    if destination.read_text() != generated:
        raise AssertionError('which-key accent contract changed; regenerate explicitly with --generate-colors')
    binary = ROOT / 'target/port/which-key-scene-probe'
    if not args.native_only and not args.skip_build:
        subprocess.run(['pixi', 'run', 'bash', '-c', 'mojo build --fp-mode=contract=off -I src -I scripts/penna -I "$1/mojo" -D MJ_RASTER=swash -Xlinker build/lib/libmjswash.a $MJ_LINK -Xlinker build/lib/libSDL3.a -Xlinker -lgcc_s -Xlinker -ldl -Xlinker -lrt -Xlinker -lutil "$1/mojo/which_key_scene_probe.mojo" -o "$2"', '_', str(ROOT), str(binary)], cwd=MJUI, check=True)
    records = []
    for theme in args.theme or range(6):
        for scale in args.scale or [1, 1.25, 2]:
            for variant in args.variant or range(8):
                directory = TARGET / f'theme-{theme}-scale-{scale:g}-variant-{variant}'
                config = json.loads((directory / 'input.json').read_text())
                if 'accent' in config:
                    assert config['accent'] == colors[theme], 'source accent changed with viewport or density'
                (directory / 'input.bin').write_bytes(packet(config))
                with Image.open(directory / 'native.png') as source:
                    expected = source.convert('RGBA')
                record = {'theme': theme, 'scale': scale, 'variant': variant, 'pixels': expected.width * expected.height}
                if not args.native_only:
                    subprocess.run([str(binary), str(directory / 'input.bin'), str(directory / 'mojo.png')], cwd=MJUI, check=True, timeout=60)
                    with Image.open(directory / 'mojo.png') as result:
                        actual = result.convert('RGBA')
                    assert actual.size == expected.size
                    bands = ImageChops.difference(actual, expected).split()
                    mask = bands[0]
                    for band in bands[1:]:
                        mask = ImageChops.lighter(mask, band)
                    different = mask.width * mask.height - mask.histogram()[0]
                    record.update({'different': different, 'exact': different == 0, 'max_error': max(band.getextrema()[1] for band in bands)})
                    if different:
                        record['difference_bounds'] = list(mask.getbbox())
                records.append(record)
    report = {'cases': records, 'source_only': args.native_only, 'exact': not args.native_only and all(item['exact'] for item in records)}
    filtered = args.theme or args.scale or args.variant
    TARGET.mkdir(parents=True, exist_ok=True)
    name = ('source-report' if args.native_only else 'report') + ('-filtered' if filtered else '') + '.json'
    (TARGET / name).write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'source_only': args.native_only, 'cases': len(records), 'pixels': sum(item['pixels'] for item in records), 'exact_cases': sum(item.get('exact', False) for item in records), 'exact': report['exact'], 'failures': [item for item in records if not args.native_only and not item['exact']][:8]}), flush=True)
    if not args.native_only and not report['exact'] and not args.diagnose:
        raise SystemExit('native WhichKey pixel identity is incomplete')


if __name__ == '__main__':
    main()
