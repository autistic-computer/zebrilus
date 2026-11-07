import argparse
import json
import os
from pathlib import Path
import runpy
import struct
import subprocess

from terminal_pixels import compare, packed


ROOT = Path(__file__).resolve().parents[2]
MJUI = ROOT.parent / "mjui"
DIRECTORY = ROOT / "target/port/terminal-styled-reference"
CASES = ("regular-only", "full-faces", "mixed-faces", "no-synthetic", "decorations", "contrast", "glyphs", "composition")


def fixture(directory):
    source = json.loads((directory / "input.json").read_text())
    geometry = json.loads((directory / "geometry.json").read_text())
    appearance = source["appearance"]
    output = bytearray()

    def number(value):
        output.extend(struct.pack("<I", value))

    def text(value):
        value = value.encode()
        number(len(value))
        output.extend(value)

    for value in (source["width"], source["height"], geometry["columns"], geometry["rows"]):
        number(value)
    output.extend(struct.pack("<ff", source["font_size"], source["scale"]))
    for value in (source["columns"], source["rows"], packed(appearance["foreground"]), packed(appearance["background"]), packed(appearance["selection_background"], True), packed(appearance["selection_foreground"]), packed(appearance["search_match_color"], True), packed(appearance["search_current_color"], True), packed(appearance["link_color"], True), packed(appearance["copy_cursor_color"], True), int(appearance["rounded_selection"])):
        number(value)
    output.extend(struct.pack("<f", appearance["minimum_contrast"]))
    number(int(appearance["font_thicken"]))
    number(appearance["font_thicken_strength"])
    synthetic = appearance["font_synthetic_style"]
    number(int(synthetic["bold"]) | int(synthetic["italic"]) << 1 | int(synthetic["bold_italic"]) << 2)
    for path in source["style_paths"]:
        text(path)
    number(len(source["fallback_paths"]))
    for path in source["fallback_paths"]:
        text(path)
    number(len(source["styles"]))
    for style in source["styles"]:
        for value in style:
            number(value)
    number(len(source["grapheme_offsets"]))
    for value in source["grapheme_offsets"]:
        number(value)
    number(len(source["grapheme_bytes"]))
    output.extend(source["grapheme_bytes"])
    for cell in source["cells"]:
        for value in cell:
            number(value)
    number(len(source["overlays"]))
    for overlay in source["overlays"]:
        for value in overlay:
            number(value)
    number(int(source["cursor"] is not None))
    if source["cursor"] is not None:
        for value in source["cursor"]:
            number(value)
    number(int(source["marked"] is not None))
    if source["marked"] is not None:
        text(source["marked"])
    number(len(geometry["glyph_probes"]))
    for probe in geometry["glyph_probes"]:
        text(probe["text"])
    assert len(output) <= 1024 * 1024
    path = directory / "input.bin"
    path.write_bytes(output)
    return path


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    parser = argparse.ArgumentParser()
    parser.add_argument("--record", action="store_true")
    parser.add_argument("--native-only", action="store_true")
    parser.add_argument("--case", choices=CASES, action="append")
    parser.add_argument("--scale", type=float, action="append")
    args = parser.parse_args()
    cases = args.case or CASES
    scales = args.scale or (1.0, 1.25)
    if len(scales) > 8 or any(not 0.5 <= scale <= 3 for scale in scales):
        raise SystemExit("styled fixture scale exceeds limit")
    DIRECTORY.mkdir(parents=True, exist_ok=True)
    environment = os.environ.copy()
    environment["CARGO_TARGET_DIR"] = str(ROOT / "target/penna-native")
    command = ["cargo", "build", "--locked", "--offline", "-p", "zz-ui", "--example", "terminal_styled_fixture"]
    for setting in ("profile.dev.debug=0", 'profile.dev.package."*".opt-level=0', "profile.dev.package.ala.opt-level=0", "profile.dev.package.ala-macros.opt-level=0", "profile.dev.package.ala-platform.opt-level=0", "profile.dev.package.taffy.opt-level=0"):
        command.extend(["--config", setting])
    subprocess.run(command, cwd=ROOT, env=environment, check=True, timeout=1800)
    probe = ROOT / "target/port/terminal-styled-probe"
    if not args.native_only:
        compile_command = 'mojo build --fp-mode=contract=off -I src -I "$1" -D MJ_RASTER=swash -Xlinker build/lib/libmjswash.a $MJ_LINK -Xlinker /usr/lib/x86_64-linux-gnu/libSDL3.so.0 -Xlinker -lgcc_s -Xlinker -ldl -Xlinker -lrt -Xlinker -lutil "$1/terminal_styled_probe.mojo" -o "$2"'
        subprocess.run(["pixi", "run", "bash", "-c", compile_command, "_", str(ROOT / "mojo"), str(probe)], cwd=MJUI, check=True, timeout=300)
    results = []
    native = ROOT / "target/penna-native/debug/examples/terminal_styled_fixture"
    for case in cases:
        for scale in scales:
            directory = DIRECTORY / f"{case}-scale-{scale}"
            subprocess.run([str(native), str(directory), case, "13", str(scale)], cwd=ROOT, check=True, timeout=60)
            input_path = fixture(directory)
            if args.native_only:
                geometry = json.loads((directory / "geometry.json").read_text())
                result = {"case": case, "scale": scale, "faces": geometry["faces"], "columns": geometry["columns"], "rows": geometry["rows"]}
            else:
                subprocess.run([str(probe), str(input_path), str(directory / "mojo.png"), str(directory / "mojo.json")], cwd=MJUI, check=True, timeout=60)
                result = compare(directory)
                result["case"] = case
            results.append(result)
            print(json.dumps(result), flush=True)
    report = {"native_only": args.native_only, "exact": None if args.native_only else all(case["exact"] for case in results), "cases": results, "scope": "isolated explicit font files, actual native RowRenderCache GPU readback, synthetic/real style faces, packed attributes/decorations/contrast, scalar and dictionary graphemes, explicit multilingual fallback, wide/spacer, selection and composition"}
    (DIRECTORY / ("native-report.json" if args.native_only else "report.json")).write_text(json.dumps(report, indent=2) + "\n")
    if not args.native_only and not args.record and not report["exact"]:
        raise SystemExit("styled terminal pixel identity is incomplete; see " + str(DIRECTORY / "report.json"))


if __name__ == "__main__":
    main()
