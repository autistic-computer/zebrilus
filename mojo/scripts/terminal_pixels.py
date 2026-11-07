import argparse
import json
import os
from pathlib import Path
import runpy
import struct
import subprocess

from PIL import Image, ImageChops


ROOT = Path(__file__).resolve().parents[2]
MJUI = ROOT.parent / "mjui"
DIRECTORY = ROOT / "target/port/terminal-reference"


def packed(value, alpha=False):
    result = value["r"] << 16 | value["g"] << 8 | value["b"]
    return result << 8 | value.get("a", 255) if alpha else result


def fixture(directory):
    source = json.loads((directory / "input.json").read_text())
    geometry = json.loads((directory / "geometry.json").read_text())
    appearance = source["appearance"]
    output = bytearray()

    def number(value):
        output.extend(struct.pack("<I", value))

    for value in (source["width"], source["height"], geometry["columns"], geometry["rows"]):
        number(value)
    output.extend(struct.pack("<ff", source["font_size"], source["scale"]))
    for value in (source["columns"], source["rows"], packed(appearance["foreground"]), packed(appearance["background"]), packed(appearance["selection_background"], True), packed(appearance["selection_foreground"]), packed(appearance["search_match_color"], True), packed(appearance["search_current_color"], True), packed(appearance["link_color"], True), packed(appearance["copy_cursor_color"], True), int(appearance["rounded_selection"])):
        number(value)
    for cell in source["cells"]:
        for value in cell:
            number(value)
    number(len(source["overlays"]))
    for span in source["overlays"]:
        for value in span:
            number(value)
    for value in source["cursor"]:
        number(value)
    path = directory / "input.bin"
    path.write_bytes(output)
    return path


def compare(directory):
    source = json.loads((directory / "input.json").read_text())
    native = Image.open(directory / "native.png").convert("RGBA")
    actual = Image.open(directory / "mojo.png").convert("RGBA")
    if native.size != actual.size:
        raise SystemExit("terminal fixture dimensions disagree")
    expected_pixels = list(native.get_flattened_data())
    actual_pixels = list(actual.get_flattened_data())
    differing = sum(left != right for left, right in zip(expected_pixels, actual_pixels, strict=True))
    delta = ImageChops.difference(native, actual)
    delta.convert("RGB").point(lambda value: min(value * 8, 255)).save(directory / "difference.png")
    geometry = json.loads((directory / "geometry.json").read_text())
    candidate = json.loads((directory / "mojo.json").read_text())
    metrics = ("advance", "ascent", "descent", "line_gap", "baseline", "cell_width", "line_height")
    metric_mismatches = {name: [geometry[name], struct.unpack("<f", struct.pack("<I", candidate["metric_bits"][index]))[0]] for index, name in enumerate(metrics) if struct.unpack("<I", struct.pack("<f", geometry[name]))[0] != candidate["metric_bits"][index]}
    report = {
        "reference": "actual native terminal RowRenderCache prepaint/paint, native font measurements and GPU readback",
        "renderer": "Mojo terminal painter, native raster and GPU readback",
        "pixels": len(expected_pixels),
        "different_pixels": differing,
        "max_channel_delta": max(abs(left - right) for a, b in zip(expected_pixels, actual_pixels, strict=True) for left, right in zip(a, b, strict=True)),
        "metric_mismatches": metric_mismatches,
        "exact": differing == 0 and not metric_mismatches,
        "font_size": source["font_size"],
        "scale": source["scale"],
    }
    (directory / "case.json").write_text(json.dumps(report, indent=2) + "\n")
    return report

def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    parser = argparse.ArgumentParser()
    parser.add_argument("--record", action="store_true")
    args = parser.parse_args()
    DIRECTORY.mkdir(parents=True, exist_ok=True)
    environment = os.environ.copy()
    environment["CARGO_TARGET_DIR"] = str(ROOT / "target/penna-native")
    command = ["cargo", "build", "--locked", "--offline", "-p", "zz-ui", "--example", "terminal_render_fixture"]
    for setting in ("profile.dev.debug=0", 'profile.dev.package."*".opt-level=0', "profile.dev.package.ala.opt-level=0", "profile.dev.package.ala-macros.opt-level=0", "profile.dev.package.ala-platform.opt-level=0", "profile.dev.package.taffy.opt-level=0"):
        command.extend(["--config", setting])
    subprocess.run(command, cwd=ROOT, env=environment, check=True, timeout=1800)
    probe = ROOT / "target/port/terminal-render-probe"
    compile_command = 'mojo build --fp-mode=contract=off -I src -I "$1" -D MJ_RASTER=swash -Xlinker build/lib/libmjswash.a $MJ_LINK -Xlinker /usr/lib/x86_64-linux-gnu/libSDL3.so.0 -Xlinker -lgcc_s -Xlinker -ldl -Xlinker -lrt -Xlinker -lutil "$1/terminal_render_probe.mojo" -o "$2"'
    subprocess.run(["pixi", "run", "bash", "-c", compile_command, "_", str(ROOT / "mojo"), str(probe)], cwd=MJUI, check=True, timeout=300)
    results = []
    native = ROOT / "target/penna-native/debug/examples/terminal_render_fixture"
    for size, scale in ((13, 1), (13.5, 1), (14, 1), (16, 1), (13, 1.25), (13, 2)):
        directory = DIRECTORY if (size, scale) == (13, 1) else DIRECTORY / f"size-{size}-scale-{scale}"
        subprocess.run([str(native), str(directory), str(size), str(scale)], cwd=ROOT, check=True, timeout=60)
        input_path = fixture(directory)
        subprocess.run([str(probe), str(input_path), str(directory / "mojo.png"), str(directory / "mojo.json")], cwd=MJUI, check=True, timeout=60)
        result = compare(directory)
        results.append(result)
        print(json.dumps(result), flush=True)
    report = {"exact": all(case["exact"] for case in results), "cases": results, "scope": "actual native terminal GPU versus Mojo GPU; four font sizes, three device scales, ligatures, selected foreground/background, link hover, copy cursor and underline cursor"}
    (DIRECTORY / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    if not args.record and not report["exact"]:
        raise SystemExit("native terminal pixel identity is incomplete; see " + str(DIRECTORY / "report.json"))


if __name__ == "__main__":
    main()
