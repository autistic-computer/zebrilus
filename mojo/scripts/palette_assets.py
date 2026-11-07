import argparse
import json
import os
from pathlib import Path
import re
import runpy
import subprocess

ROOT = Path(__file__).resolve().parents[2]
MJUI = ROOT.parent / 'mjui'
TARGET = ROOT / 'target/port/palette-assets'
NATIVE = ROOT / 'target/penna-native/debug/examples/palette_assets'


def check_sources():
    source = (ROOT / 'mojo/zzmojo/palette_assets.mojo').read_text()
    for name in ('openai', 'claude'):
        match = re.search(r'comptime ' + name.upper() + r'_SVG: StaticString = ("[^\n]*")\n', source)
        if match is None:
            raise AssertionError('missing palette SVG source: ' + name)
        expected = (ROOT / f'crates/zz-ui/assets/icons/{name}.svg').read_bytes()
        if json.loads(match[1]).encode() != expected:
            raise AssertionError('palette SVG source changed: ' + name)


def main():
    runpy.run_path(str(ROOT / 'scripts/port/guard.py'))['ensure']()
    parser = argparse.ArgumentParser()
    parser.add_argument('--skip-native', action='store_true')
    parser.add_argument('--skip-build', action='store_true')
    parser.add_argument('--diagnose', action='store_true')
    args = parser.parse_args()
    check_sources()
    TARGET.mkdir(parents=True, exist_ok=True)
    if not args.skip_native:
        environment = os.environ.copy()
        environment.update(CARGO_TARGET_DIR=str(ROOT / 'target/penna-native'), CC='/usr/bin/clang', CXX='/usr/bin/clang++')
        subprocess.run(['cargo', '+1.99.0', 'build', '--locked', '--offline', '-j', '1', '--manifest-path', 'scripts/penna/native/Cargo.toml', '--example', 'palette_assets'], cwd=MJUI, env=environment, check=True)
        for theme in range(6):
            for scale in ('1', '1.25', '2'):
                subprocess.run([str(NATIVE), str(TARGET / str(theme) / scale), str(theme), scale], cwd=MJUI, check=True)
    binary = TARGET / 'palette-assets'
    if not args.skip_build:
        subprocess.run(['pixi', 'run', 'bash', '-c', 'mojo build --fp-mode=contract=off -I src -I "$1/mojo" -D MJ_RASTER=swash -Xlinker build/lib/libmjswash.a $MJ_LINK -Xlinker -lgcc_s -Xlinker -ldl -Xlinker -lrt -Xlinker -lutil -Xlinker build/lib/libSDL3.a "$1/mojo/tests/palette_asset_probe.mojo" -o "$2"', '_', str(ROOT), str(binary)], cwd=MJUI, check=True)
    from PIL import Image
    total = different = 0
    records = []
    for theme in range(6):
        for scale in ('1', '1.25', '2'):
            directory = TARGET / str(theme) / scale
            subprocess.run([str(binary), str(directory / 'mjui.png'), str(theme), scale], cwd=MJUI, check=True)
            reference = Image.open(directory / 'native.png').convert('RGBA')
            candidate = Image.open(directory / 'mjui.png').convert('RGBA')
            if reference.size != candidate.size:
                raise AssertionError('palette asset frame dimensions differ')
            a, b = reference.tobytes(), candidate.tobytes()
            count = sum(a[index:index + 4] != b[index:index + 4] for index in range(0, len(a), 4))
            maximum = max((abs(left - right) for left, right in zip(a, b)), default=0)
            pixels = reference.width * reference.height
            total += pixels
            different += count
            records.append({'theme': theme, 'scale': float(scale), 'pixels': pixels, 'different': count, 'maximum': maximum})
            print(records[-1], flush=True)
    report = {'frames': records, 'pixels': total, 'different': different, 'source_svg_bytes': 3429}
    (TARGET / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(f'Palette asset icons: {total - different}/{total} exact pixels')
    if different and not args.diagnose:
        raise SystemExit('native palette asset icons differ')


if __name__ == '__main__':
    main()
