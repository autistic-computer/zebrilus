#!/usr/bin/env python3
import argparse
import json
from pathlib import Path
import subprocess


ROOT = Path(__file__).resolve().parents[2]


def literal(value):
    if isinstance(value, bool):
        return "True" if value else "False"
    if isinstance(value, list):
        return "[" + ", ".join(literal(item) for item in value) + "]"
    return repr(value)


def table(name, element_type, rows):
    return (
        f"    var {name}: List[{element_type}] = [\n"
        + "".join(f"        {literal(row)},\n" for row in rows)
        + "    ]\n"
    )


def generate(terminal):
    source = """from std.testing import assert_equal
from zzmojo.terminal_view import PointerGrid, accumulate_scroll, cursor_should_blink, scroll_fraction, scrollbar_strip_hit, selection_autoscroll_lines

def main() raises:
"""
    source += table("pointer_inputs", "List[Float32]", [row["grid"] + row["point"] for row in terminal["pointers"]])
    source += table("pointer_expected", "List[Int]", [row["size"] + [int(row["contains"])] + row["expected"] for row in terminal["pointers"]])
    source += """    for i in range(len(pointer_inputs)):
        var p = pointer_inputs[i].copy()
        var expected = pointer_expected[i].copy()
        var grid = PointerGrid(p[0], p[1], p[2], p[3], p[4], p[5], p[6], p[7], expected[0], expected[1])
        var actual = grid.project(p[8], p[9], 9, False)
        assert_equal(Int(grid.contains(p[8], p[9])), expected[2])
        assert_equal(actual.column, expected[3])
        assert_equal(actual.row, expected[4])
        assert_equal(actual.click_count, expected[5])
        assert_equal(actual.x, expected[6])
        assert_equal(actual.y, expected[7])
        assert_equal(actual.screen_width, expected[8])
        assert_equal(actual.screen_height, expected[9])
        assert_equal(actual.cell_width, expected[10])
        assert_equal(actual.cell_height, expected[11])
"""
    source += table("scroll_inputs", "List[Float32]", [row["args"] for row in terminal["scrolling"]])
    source += table("scroll_expected", "Int", [row["expected"] for row in terminal["scrolling"]])
    source += """    for i in range(len(scroll_inputs)):
        var p = scroll_inputs[i].copy()
        assert_equal(selection_autoscroll_lines(p[0], p[1], p[2], p[3]), scroll_expected[i])
"""
    source += table("fraction_inputs", "List[Float32]", [row["args"] for row in terminal["fractions"]])
    source += table("fraction_expected", "Int", [row["expected"] for row in terminal["fractions"]])
    source += """    for i in range(len(fraction_inputs)):
        var p = fraction_inputs[i].copy()
        assert_equal(Int(scroll_fraction(p[0], p[1], p[2])), fraction_expected[i])
"""
    source += table("accumulation_inputs", "List[Float32]", [row["args"] + [row["remainder"]] for row in terminal["accumulations"]])
    source += table("accumulation_expected", "Int", [row["lines"] for row in terminal["accumulations"]])
    source += """    for i in range(len(accumulation_inputs)):
        var p = accumulation_inputs[i].copy()
        var remainder = p[0]
        assert_equal(accumulate_scroll(remainder, p[1]), accumulation_expected[i])
        assert_equal(remainder, p[2])
"""
    source += table("cursor_inputs", "List[Int]", [[int(row[key]) for key in ("visible", "blinking", "focused", "policy", "expected")] for row in terminal["cursors"]])
    source += """    for p in cursor_inputs:
        var bits = (UInt64(p[0]) << 58) | (UInt64(p[1]) << 59) | (UInt64(1) << 63)
        assert_equal(cursor_should_blink(bits, p[3], Bool(p[2])), Bool(p[4]))
"""
    source += table("strip_inputs", "List[Float32]", [row["point"] for row in terminal["strips"]])
    source += table("strip_expected", "List[Int]", [[row["total"], int(row["expected"])] for row in terminal["strips"]])
    source += """    for i in range(len(strip_inputs)):
        var p = strip_inputs[i].copy()
        var expected = strip_expected[i].copy()
        assert_equal(scrollbar_strip_hit(90, 20, 52, 10, expected[0], 20, p[0], p[1]), Bool(expected[1]))
"""
    count = sum(len(rows) for rows in terminal.values())
    source += f'    print("Terminal differential: {count} Rust reference cases passed")\n'
    return source, count


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--oracle", type=Path, default=ROOT / "target/port/oracle.json")
    parser.add_argument("--refresh", action="store_true")
    args = parser.parse_args()
    args.oracle.parent.mkdir(parents=True, exist_ok=True)
    if args.refresh or not args.oracle.exists():
        with args.oracle.open("w") as output:
            subprocess.run(["cargo", "run", "--locked", "-p", "zz-client", "--example", "port_oracle"], cwd=ROOT, stdout=output, check=True)
    terminal = json.loads(args.oracle.read_text())["terminal"]
    if not terminal or not all(terminal.values()):
        raise SystemExit("Rust oracle contains no terminal vectors; regenerate it with --refresh")
    source, count = generate(terminal)
    generated = ROOT / "target/port/terminal_differential.mojo"
    generated.write_text(source)
    subprocess.run(["pixi", "run", "--manifest-path", str(ROOT / "mojo/pixi.toml"), "mojo", "run", "-D", "ASSERT=all", "-I", str(ROOT / "mojo"), str(generated)], cwd=ROOT, check=True)
    (ROOT / "target/port/terminal-differential.json").write_text(json.dumps({"reference": "zz-client::terminal_view", "cases": count, "passed": True}, indent=2) + "\n")


if __name__ == "__main__":
    main()
