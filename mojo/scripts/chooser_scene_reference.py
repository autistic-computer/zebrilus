import argparse
import json
from pathlib import Path
import runpy
import struct
import subprocess

from PIL import Image

from palette_places_vectors import uint


ROOT = Path(__file__).resolve().parents[2]
MJUI = ROOT.parent / 'mjui'
DIRECTORY = ROOT / 'target/port/chooser-scene-reference'


def contract(colors):
    words = ''.join(f'{word:08x}' for theme in range(6) for color in colors[theme] for word in color)
    return '''from ..native_color import NativeColor, color_float
from .penna_native import _hex

comptime CHOOSER_COLOR_WORDS = "''' + words + '''"


def chooser_color(theme: Int, field: Int) raises -> NativeColor:
    if theme < 0 or theme >= 6 or field < 0 or field >= 2:
        raise Error("unknown chooser color")
    var values = SIMD[.float32, 8](0)
    for index in range(8):
        values[index] = color_float(_hex(CHOOSER_COLOR_WORDS.as_bytes(), ((theme * 2 + field) * 8 + index) * 8, 8))
    return NativeColor.source(SIMD[.float32, 4](values[0], values[1], values[2], values[3]), SIMD[.float32, 4](values[4], values[5], values[6], values[7]))
'''


def packet(config):
    data = bytearray()
    uint(data, config['theme'])
    for field in ['width', 'height', 'scale']:
        uint(data, struct.unpack('<I', struct.pack('<f', config[field]))[0])
    uint(data, len(config['frame']))
    data.extend(config['frame'])
    return data


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    parser = argparse.ArgumentParser()
    parser.add_argument('--skip-build', action='store_true')
    parser.add_argument('--skip-capture', action='store_true')
    parser.add_argument('--generate', action='store_true')
    args = parser.parse_args()
    if not args.skip_build:
        subprocess.run(['cargo', 'build', '--offline', '--quiet', '--locked', '-p', 'zz-ui', '--no-default-features', '--features', 'scene-fixtures', '--example', 'chooser_scene_fixture'], cwd=ROOT, check=True)
    colors = {}
    records = []
    for theme in range(6):
        for scale in [1, 1.25, 2]:
            for variant in range(7):
                directory = DIRECTORY / f'theme-{theme}-scale-{scale:g}-variant-{variant}'
                if not args.skip_capture:
                    subprocess.run([str(ROOT / 'target/debug/examples/chooser_scene_fixture'), str(directory), str(theme), str(scale), str(variant)], cwd=ROOT, check=True, timeout=60)
                config = json.loads((directory / 'input.json').read_text())
                scene = json.loads((directory / 'scene.json').read_text())
                if theme in colors:
                    assert colors[theme] == config['colors'], 'source chooser colors changed with viewport or density'
                colors[theme] = config['colors']
                assert config['font_roles'][0] == config['font_roles'][1], 'fixture medium font no longer resolves to the regular face'
                with Image.open(directory / 'native.png') as image:
                    assert image.size == (int(config['width'] * scale), int(config['height'] * scale))
                    assert image.mode == 'RGBA'
                    assert image.getchannel('A').getextrema() == (255, 255)
                    pixels = image.width * image.height
                assert scene['quads'] and scene['glyphs'], 'source chooser did not paint its surface and text'
                (directory / 'input.bin').write_bytes(packet(config))
                records.append({'theme': theme, 'scale': scale, 'variant': variant, 'pixels': pixels, 'quads': len(scene['quads']), 'glyphs': len(scene['glyphs'])})
    generated = contract(colors)
    destination = MJUI / 'src/mjui/widgets/penna_chooser_data.mojo'
    if args.generate:
        destination.write_text(generated)
    assert destination.read_text() == generated, 'chooser color contract changed; regenerate explicitly with --generate'
    report = {'source_captures': len(records), 'pixels': sum(item['pixels'] for item in records), 'records': records, 'scope': 'actual native chooser modal, rows, search, notices, close button and key hints; source references and color contract only, candidate rendering not compared'}
    (DIRECTORY / 'source-report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({key: value for key, value in report.items() if key != 'records'}))


if __name__ == '__main__':
    main()
