import argparse
import json
from pathlib import Path
import runpy
import subprocess


PORT = Path(__file__).resolve().parents[1]
ROOT = PORT.parent
TARGET = ROOT / "target/port"


def text_values(value):
    if value is None:
        return [-1]
    encoded = value.encode("utf-8")
    return [len(encoded), *encoded]


def tables_values(value):
    result = [len(value["cells"])]
    for cell in value["cells"]:
        result.extend(cell[key] for key in ("glyph", "style", "flags"))
    result.append(len(value["styles"]))
    for style in value["styles"]:
        result.extend(style[key] for key in ("foreground", "background", "underline_color", "attributes", "underline_kind", "background_index"))
    result.extend((len(value["grapheme_offsets"]), *value["grapheme_offsets"]))
    result.extend((len(value["grapheme_bytes"]), *value["grapheme_bytes"]))
    return result


def viewport_fingerprint(value):
    result = [value["generation"], value["view_generation"], value["dictionary_generation"], value["columns"], value["rows"], *tables_values(value)]
    metadata = value["metadata"]
    result.extend((metadata["foreground"], metadata["background"]))
    for key in ("title", "working_directory", "hovered_uri"):
        result.extend(text_values(metadata[key]))
    result.extend([-1] if metadata["cursor"] is None else [1, *metadata["cursor"]])
    result.extend(metadata["scrollbar"])
    result.append(len(metadata["overlays"]))
    for overlay in metadata["overlays"]:
        result.extend(overlay[key] for key in ("row", "start", "end", "kind_and_flags"))
    result.extend(int(value) for value in metadata["mode"])
    result.extend([-1] if metadata["search"] is None else [1, *(int(value) for value in metadata["search"])])
    result.extend((metadata["unseen_output"], int(metadata["kitty_keyboard"]), int(metadata["mouse_tracking"])))
    status = metadata["status"]
    result.extend((status["kind"], status["exit_code"], *text_values(status["signal"]), *text_values(status["failure"])))
    result.append(len(metadata["kitty"]))
    for placement in metadata["kitty"]:
        result.extend(placement[key] for key in ("image_id", "image_generation", "layer", "viewport_column", "viewport_row", "absolute_row", "cell_offset_x", "cell_offset_y", "grid_columns", "grid_rows", "pixel_width", "pixel_height"))
        result.extend([-1] if placement["source_rect"] is None else [1, *placement["source_rect"]])
    return ",".join(map(str, result)) + ","


def history_fingerprint(value):
    result = [5, 100, 5, value["columns"], value["rows"], *tables_values(value)]
    return ",".join(map(str, result)) + ","


def literal(value):
    if isinstance(value, bool):
        return "True" if value else "False"
    if isinstance(value, list):
        return "[" + ",".join(map(literal, value)) + "]"
    return json.dumps(value, ensure_ascii=False)


def table(name, datatype, rows):
    return f"    var {name}: List[{datatype}] = [\n" + "".join(f"        {literal(row)},\n" for row in rows) + "    ]\n"


def viewport_body(frame):
    cursor = 9
    for _ in range(2):
        while frame[cursor] & 128:
            cursor += 1
        cursor += 1
    return frame[cursor:]


def generate(vectors):
    source = """from std.testing import assert_equal, assert_true
from zzmojo.terminal_wire import decode_terminal_frame, decode_viewport_bytes
from terminal_codec_support import viewport_fingerprint, history_fingerprint

def main() raises:
"""
    source += table("full_frames", "List[UInt8]", [case["bytes"] for case in vectors["full"]])
    source += table("full_bodies", "List[UInt8]", [viewport_body(case["bytes"]) for case in vectors["full"]])
    source += table("full_expected", "String", [viewport_fingerprint(case["viewport"]) for case in vectors["full"]])
    source += table("full_panes", "String", [case["pane"] for case in vectors["full"]])
    source += table("full_sequences", "String", [case["sequence"] for case in vectors["full"]])
    source += """    for i in range(len(full_frames)):
        var update = decode_terminal_frame(full_frames[i].copy())
        assert_equal(update.kind, UInt8(0))
        assert_equal(String(update.pane), full_panes[i])
        assert_equal(String(update.sequence), full_sequences[i])
        assert_true(Bool(update.viewport))
        assert_equal(viewport_fingerprint(update.viewport.value()), full_expected[i])
        var body = decode_viewport_bytes(full_bodies[i].copy())
        assert_equal(viewport_fingerprint(body), full_expected[i])
"""
    source += table("patch_bases", "List[UInt8]", [case["base"] for case in vectors["patches"]])
    source += table("patch_frames", "List[UInt8]", [case["bytes"] for case in vectors["patches"]])
    source += table("patch_expected", "String", [viewport_fingerprint(case["viewport"]) for case in vectors["patches"]])
    source += table("patch_scrolls", "Int", [case["scroll"] for case in vectors["patches"]])
    source += """    for i in range(len(patch_frames)):
        var base = decode_terminal_frame(patch_bases[i].copy())
        var update = decode_terminal_frame(patch_frames[i].copy())
        assert_equal(update.kind, UInt8(1))
        assert_true(Bool(update.patch))
        assert_equal(update.patch.value().scroll, patch_scrolls[i])
        var retained = base.viewport.value().copy()
        retained.apply(update.patch.value())
        assert_equal(viewport_fingerprint(retained), patch_expected[i])
"""
    rejected = vectors["rejected_patches"]
    source += table("rejected_bases", "List[UInt8]", [case["base"] for case in rejected])
    source += table("rejected_frames", "List[UInt8]", [case["bytes"] for case in rejected])
    source += table("rejected_unchanged", "String", [viewport_fingerprint(case["unchanged"]) for case in rejected])
    source += """    for i in range(len(rejected_frames)):
        var base = decode_terminal_frame(rejected_bases[i].copy())
        var update = decode_terminal_frame(rejected_frames[i].copy())
        var retained = base.viewport.value().copy()
        var rejected = False
        try:
            retained.apply(update.patch.value())
        except:
            rejected = True
        assert_true(rejected)
        assert_equal(viewport_fingerprint(retained), rejected_unchanged[i])
"""
    output = vectors["command_output"]
    source += f'    var output_bytes: List[UInt8] = {literal(output["bytes"])}\n'
    source += "    var output = decode_terminal_frame(output_bytes^)\n"
    source += "    assert_equal(output.kind, UInt8(2))\n"
    source += f'    assert_equal(String(output.output_id.value()), {literal(output["output_id"])})\n'
    source += f'    assert_equal(viewport_fingerprint(output.viewport.value()), {literal(viewport_fingerprint(output["viewport"]))})\n'
    history = vectors["history"]
    source += f'    var history_bytes: List[UInt8] = {literal(history["bytes"])}\n'
    source += "    var history = decode_terminal_frame(history_bytes^)\n"
    source += "    assert_equal(history.kind, UInt8(3))\n"
    source += f'    assert_equal(history_fingerprint(history.history.value()), {literal(history_fingerprint(history))})\n'
    source += table("mutation_frames", "List[UInt8]", [case["bytes"] for case in vectors["mutations"]])
    source += table("mutation_accepted", "Bool", [case["accepted"] for case in vectors["mutations"]])
    source += table("mutation_expected", "String", [viewport_fingerprint(case["expected"]["viewport"]) if case["expected"] else "" for case in vectors["mutations"]])
    source += """    for i in range(len(mutation_frames)):
        var accepted = False
        var actual = String()
        try:
            var update = decode_terminal_frame(mutation_frames[i].copy())
            accepted = True
            if update.viewport:
                actual = viewport_fingerprint(update.viewport.value())
        except:
            pass
        assert_equal(accepted, mutation_accepted[i])
        if mutation_expected[i].byte_length() > 0:
            assert_equal(actual, mutation_expected[i])
"""
    count = len(vectors["full"]) + len(vectors["patches"]) + len(rejected) + len(vectors["mutations"]) + 2
    source += f'    print("Packed terminal differential: {count} real encoder cases passed")\n'
    return source, count


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    parser = argparse.ArgumentParser()
    parser.add_argument("--fixture", type=Path)
    args = parser.parse_args()
    TARGET.mkdir(parents=True, exist_ok=True)
    fixture = args.fixture or TARGET / "terminal-codec-vectors.json"
    if args.fixture is None:
        with fixture.open("w") as destination:
            subprocess.run(["cargo", "run", "--locked", "-p", "zz-protocol", "--example", "terminal_port_vectors"], cwd=ROOT, stdout=destination, check=True)
    vectors = json.loads(fixture.read_text())
    source, count = generate(vectors)
    generated = TARGET / "terminal_codec_vectors.mojo"
    generated.write_text(source)
    subprocess.run(["mojo", "build", "-D", "ASSERT=all", "-I", str(PORT), "-I", str(PORT / "tests"), str(generated), "-o", str(TARGET / "terminal-codec-vectors")], cwd=ROOT, check=True)
    subprocess.run([str(TARGET / "terminal-codec-vectors")], cwd=ROOT, check=True)
    (TARGET / "terminal-codec-differential.json").write_text(json.dumps({"reference": "zz-protocol terminal encoder and decoder", "cases": count, "passed": True}, indent=2) + "\n")


if __name__ == "__main__":
    main()
