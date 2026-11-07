import bisect
import itertools
import json
from pathlib import Path
import runpy
import subprocess
import tempfile

from palette_places_vectors import uint, text


ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "mojo"
TARGET = ROOT / "target/port"
ORACLE = ROOT / "target/debug/examples/palette_shortcut_vectors"


def cases_for():
    bases = ["", " ", "Space", "space", "Enter", "enter", "Escape", "Tab", "BSpace", "DC", "Up", "Down", "Left", "Right", "Home", "End", "PPage", "PageUp", "NPage", "PageDown", "F1", "F13", "-", "+", "%", "a", "A", "Z", "λ", "É", "İ", "K", "😀", "\0", "\t", "\n", "Ctrl", "Shift", "minus", "Fn-x", "Super", "--"]
    cases = [chr(value) for value in range(2048)]
    for mask in range(16):
        prefix = "".join(value for index, value in enumerate(["D-", "C-", "M-", "S-"]) if mask & 1 << index)
        cases.extend(prefix + base for base in bases)
    for modifier in ["D-", "Cmd-", "Super-", "C-", "Ctrl-", "M-", "Alt-", "S-", "Shift-", "CTRL-", "ctrl-", "Fn-", "D-Cmd-Super-C-Ctrl-M-Alt-S-Shift-"]:
        for base in bases:
            cases.extend([modifier + base, " \u2003" + modifier + base + "\u00a0", modifier + base + " "])
    for first, second in itertools.product(["C-b", "M-x", "D-S-a", "S-L", "Space", "F1", "λ", "-", "%", "none"], repeat=2):
        cases.extend([first + " " + second, first + "\t\n" + second, " " + first + " " + second + " "])
    cases += ["", "\u2003", "C-b %", "C-b F1", "C-b ", "S-", "C-", "C- ", "D-\u2003", "\u3000b", "b\u2028%", "x" * 65536, "b " * 256, "C-" * 32767 + "b"]
    return cases


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    subprocess.run(["cargo", "build", "--offline", "-j", "1", "-p", "zz-ui", "--example", "palette_shortcut_vectors"], cwd=ROOT, check=True)
    cases = cases_for()
    reference = json.loads(subprocess.check_output([str(ORACLE)], input=json.dumps(cases).encode()))
    (TARGET / "palette-shortcuts-source.json").write_text(json.dumps({"apple": reference["apple"], "cases": cases, "format": reference["format"]}, ensure_ascii=True) + "\n")
    binary = TARGET / "palette-shortcuts-probe"
    subprocess.run(["pixi", "run", "--manifest-path", str(PORT / "pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(PORT), str(PORT / "tests/palette_shortcuts_probe.mojo"), "-o", str(binary)], cwd=ROOT, check=True)
    with tempfile.TemporaryDirectory(prefix="zz-palette-shortcuts-", dir="/tmp") as directory:
        source = Path(directory) / "in"
        target = Path(directory) / "out"
        packet = bytearray()
        uint(packet, len(cases))
        for case in cases:
            text(packet, case)
        source.write_bytes(packet)
        subprocess.run([str(binary), str(source), str(target)], check=True)
        actual = target.read_bytes()
        expected = bytes(reference["bytes"])
        if actual != expected:
            offset = next((index for index in range(min(len(actual), len(expected))) if actual[index] != expected[index]), min(len(actual), len(expected)))
            index = bisect.bisect_right(reference["ends"], offset)
            raise AssertionError((offset, index, repr(cases[index]), actual[max(0, offset - 20):offset + 70].hex(), expected[max(0, offset - 20):offset + 70].hex()))
    report = {"passed": True, "cases": len(cases), "bytes": len(expected), "explicit_refusals": 2, "reference": "actual zz_ui::command::tmux_keystroke and tmux_shortcut", "scope": "exact Keys/Text, whole-original Text fallback, whitespace/prefix aliases, uppercase/minus/Unicode key and key_char, modifier bits, empty text, 64 KiB and 256-key boundaries"}
    (TARGET / "palette-shortcuts-vectors.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
