import argparse
import bisect
import json
from pathlib import Path
import runpy
import subprocess
import tempfile

from palette_places_vectors import fixtures, packet, uint, text, optional, place


ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "mojo"
TARGET = ROOT / "target/port"
ORACLE = ROOT / "target/debug/examples/palette_verb_vectors"
PROBE = TARGET / "palette-verbs-probe"


def gui_source(gui):
    records = ["\t".join(record) for record in gui]
    return '''from .palette_verbs import PaletteGuiVerb

comptime PALETTE_GUI_CATALOG = """''' + "\n".join(records) + '''"""


def gui_verb(id: String) -> Optional[PaletteGuiVerb]:
    for line in String(PALETTE_GUI_CATALOG).split("\\n"):
        var fields = line.split("\\t")
        if fields[0] == id:
            return PaletteGuiVerb(String(fields[0]), String(fields[1]), String(fields[2]), String(fields[3]))
    return None
'''


def encode(fleets, cases):
    output = packet(fleets, [])[:-1]
    uint(output, len(cases))
    for case in cases:
        uint(output, case["tag"])
        uint(output, case["fleet"])
        for key in ("browser", "editor", "insert"):
            uint(output, case[key])
        text(output, case["aliases"])
        uint(output, len(case["gui"]))
        for id in case["gui"]:
            text(output, id)
        text(output, case["query"])
        optional(output, case["level"])
        uint(output, case["index"])
        uint(output, len(case["filled"]))
        for kind, at, value in case["filled"]:
            uint(output, kind)
            place(output, at)
            text(output, value)
        place(output, case["place"])
    return output


def cases_for(catalog, gui, scopes):
    cases = []
    empty = [None] * 4
    aliases = ["", "name=split-window", "name=split-window -h", "name=split-window -t %100", "name=split-window -- -h", "name=split-window -x 80", "name=split-window -Zt%100", "name=split-window -?", "name=split-window -λ", "name=split-window -F '#{pane_id}'", "name=split-window -J -F x", "name=send-keys", "name=send-keys 'hello world'", "name=new-session", "name=new-window", "name=rename-session", "name=attach", "name=app", "name=show-options", "name=display-message", "name=if-shell", "name=select-pane ; split-window", "name=select-pane '\\;'", "name=select-pane {", "name='split-window' -h", "name=split\\-window", "name=split-w", "name=unknown", " =select-pane", "two words=select-pane", "name=", "name=\"\"", "name=select-pane -t\"x\"", "name=select-pane -Zt", "name=select-pane -topic", "name=select-pane\u00a0-t%1", "split-pane=split-window", "split-pane = split-window", "info=show-messages -JT", "λ=split-window", "λ=rename-session\nλ=select-pane\r\nBad\u2003Name=split-window", "a=split-window\nb=select-pane\nc=new-window\nd=rename-window"]

    def add(tag, **kwargs):
        case = {"tag": tag, "fleet": 1, "browser": True, "editor": True, "insert": True, "aliases": "", "gui": [], "query": "", "level": None, "index": 0, "filled": [], "place": empty}
        case.update(kwargs)
        cases.append(case)

    for browser in (False, True):
        for editor in (False, True):
            for insert in (False, True):
                for alias in aliases:
                    add(0, browser=browser, editor=editor, insert=insert, aliases=alias)
                add(0, browser=browser, editor=editor, insert=insert, gui=[item[0] for item in gui] + ["not-a-gui-action", gui[0][0]])
    queries = ["", " ", "split", "read", "pane", "new window", "Window", "type", "file", "toggle", "λ", "\u2003", "not found"]
    for query in queries:
        for level in (None, 0, 1, 2, 3):
            add(1, query=query, level=level)
            add(1, query=query, level=level, aliases="s=split-window\nλ=rename-session\nread=select-pane")
    strings = ["", "safe_0-/:%@+,=", "a b", "λ", "a'b", 'a"b', "a\\b", "a'b\"\\$", "$x", "\n", "\0", "  a  ", "a\u2003b"]
    for value in strings:
        add(3, query=value)
    tokens = strings + ["a;b", "a\\;b", "a' b'c", "'' \"\"", "a\\", "'a\\b'", '"a\\b"', "a 'unterminated", 'a "unterminated', "a\\\nb", "{ a ; b }", "a\\ b c", "\\'a\\'", "\\", "'\\", "\u00a0one\u2003two\nthree"]
    for value in tokens:
        add(4, query=value)
    for fleet in range(5):
        for scope in scopes:
            add(5, fleet=fleet, place=scope)
    filled_values = [[], [(1, empty, "hello world")]]
    for scope in scopes:
        filled_values.append([(0, scope, "")])
        for value in ("", "hello world", "quote'\"\\$", "\n", "界"):
            filled_values.append([(0, scope, ""), (1, empty, value)])
    filled_values += [[(1, empty, value)] for value in strings]
    filled_values += [[(0, scopes[4], ""), (0, scopes[4], "")], [(1, empty, "text"), (0, scopes[4], "")], [(0, scopes[4], ""), (1, empty, "x"), (1, empty, "extra")]]
    for index in range(len(catalog)):
        for filled in filled_values:
            add(2, index=index, filled=filled)
    for alias in aliases:
        for index in (len(catalog) - 1, len(catalog), len(catalog) + 1):
            for filled in filled_values[:14]:
                add(2, aliases=alias, index=index, filled=filled)
    return cases


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    parser = argparse.ArgumentParser()
    parser.add_argument("--generate", action="store_true")
    options = parser.parse_args()
    subprocess.run(["cargo", "build", "--offline", "-j", "1", "-p", "zz-client", "--example", "palette_verb_vectors"], cwd=ROOT, check=True)
    metadata = json.loads(subprocess.check_output([str(ORACLE)], input=b'{"fleets":[],"cases":[]}'))
    generated = gui_source(metadata["gui"])
    destination = PORT / "zzmojo/palette_gui_catalog.mojo"
    if options.generate:
        destination.write_text(generated)
    assert destination.read_text() == generated, "palette GUI metadata changed; regenerate with --generate"
    fleets, _, scopes = fixtures()
    cases = cases_for(metadata["catalog"], metadata["gui"], scopes)
    reference = json.loads(subprocess.check_output([str(ORACLE)], input=json.dumps({"fleets": fleets, "cases": cases}).encode()))
    subprocess.run(["pixi", "run", "--manifest-path", str(PORT / "pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(PORT), "-I", str(PORT / "tests"), str(PORT / "tests/palette_verbs_probe.mojo"), "-o", str(PROBE)], cwd=ROOT, check=True)
    with tempfile.TemporaryDirectory(prefix="zz-palette-verbs-", dir="/tmp") as directory:
        source = Path(directory) / "in"
        target = Path(directory) / "out"
        source.write_bytes(encode(fleets, cases))
        subprocess.run([str(PROBE), str(source), str(target)], check=True)
        actual = target.read_bytes()
        expected = bytes(reference["bytes"])
        if actual != expected:
            offset = next((index for index in range(min(len(actual), len(expected))) if actual[index] != expected[index]), min(len(actual), len(expected)))
            index = bisect.bisect_right(reference["ends"], offset)
            raise AssertionError((offset, index, cases[index], actual[max(0, offset - 20):offset + 90].hex(), expected[max(0, offset - 20):offset + 90].hex()))
    report = {"passed": True, "cases": len(cases), "bytes": len(expected), "reference": "actual zz-client verbs/completion helpers", "scope": "all availability combinations, curated/catalog/GUI/alias order and slots, alias option parsing, raw Unicode tokenization/quoting, stable fuzzy ranking, typed compile/display/target/structured command output"}
    (TARGET / "palette-verbs-vectors.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
