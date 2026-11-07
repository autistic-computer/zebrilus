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
ORACLE = ROOT / "target/debug/examples/palette_places_vectors"
PROBE = TARGET / "palette-model-probe"


def encode(fleets, cases, statuses):
    output = packet(fleets, [])[:-1]
    uint(output, len(statuses))
    for index, host, status in statuses:
        uint(output, index)
        uint(output, host)
        uint(output, status)
    uint(output, len(cases))
    for case in cases:
        uint(output, case["fleet"])
        for key in ("browser", "editor", "insert"):
            uint(output, case[key])
        text(output, case["aliases"])
        uint(output, len(case["chips"]))
        for at, automatic in case["chips"]:
            place(output, at)
            uint(output, automatic)
        uint(output, case["draft"] is not None)
        if case["draft"] is not None:
            index, filled = case["draft"]
            uint(output, index)
            uint(output, len(filled))
            for kind, at, value, label in filled:
                uint(output, kind)
                place(output, at)
                text(output, value)
                text(output, label)
        text(output, case["query"])
        uint(output, len(case["history"]))
        for line in case["history"]:
            text(output, line)
    return output


def cases_for(scopes):
    cases = []
    empty = [None] * 4

    def add(**kwargs):
        case = {"fleet": 1, "chips": [], "draft": None, "query": "", "history": [], "aliases": "", "browser": True, "editor": True, "insert": True}
        case.update(kwargs)
        cases.append(case)

    queries = ["", " ", "0", "2", "work", "pane", "λ", "split", "Work cargo", "not found", ":", ":select-pane ", " \u2003:splitw", "@local"]
    for fleet in range(5):
        for scope in scopes:
            for query in queries:
                add(fleet=fleet, chips=[] if scope == empty else [(scope, False)], query=query)
        for query in queries:
            add(fleet=fleet, chips=[([0, None, None, None], True)], query=query)
    for scope in scopes:
        for browser in (False, True):
            for editor in (False, True):
                for query in ("", "split", ":", "pane", "file", "name"):
                    add(chips=[] if scope == empty else [(scope, False)], query=query, browser=browser, editor=editor, aliases="name=rename-session\nsplit=split-window -h\nwrong=unknown", history=["split-window -h", "unknown x", "html file", "split-window -h", "new-session"])
    base = json.loads(subprocess.check_output([str(ROOT / "target/debug/examples/palette_verb_vectors")], input=b'{"fleets":[],"cases":[]}'))["catalog"]
    values = [[], [(0, scopes[4], "", "Pane")], [(0, scopes[4], "", "")], [(1, empty, "name", "typed")], [(0, scopes[4], "", "Pane"), (1, empty, "name", "typed")]]
    for index in range(len(base)):
        for filled in values:
            for query in ("", "typed", "\u2003typed\u00a0"):
                add(draft=(index, filled), chips=[([0, None, None, None], True)], query=query)
    add(chips=[([0, None, None, None], True), ([0, 0, None, None], False), ([0, 0, 10, None], False), ([0, 0, 10, 100], False)])
    return cases


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    subprocess.run(["cargo", "build", "--offline", "-j", "1", "-p", "zz-ui", "--example", "palette_places_vectors"], cwd=ROOT, check=True)
    fleets, _, scopes = fixtures()
    fleets[1][1][0][4][0][3][0][5][1][3] = "running"
    fleets[4][1][0][3] = None
    cases = cases_for(scopes)
    statuses = [(1, 1, 2), (2, 1, 1)]
    reference = json.loads(subprocess.check_output([str(ORACLE)], input=json.dumps({"fleets": fleets, "cases": cases, "statuses": statuses}).encode()))
    subprocess.run(["pixi", "run", "--manifest-path", str(PORT / "pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(PORT), "-I", str(PORT / "tests"), str(PORT / "tests/palette_model_probe.mojo"), "-o", str(PROBE)], cwd=ROOT, check=True)
    with tempfile.TemporaryDirectory(prefix="zz-palette-model-", dir="/tmp") as directory:
        source = Path(directory) / "in"
        target = Path(directory) / "out"
        source.write_bytes(encode(fleets, cases, statuses))
        subprocess.run([str(PROBE), str(source), str(target)], check=True)
        actual = target.read_bytes()
        expected = bytes(reference["bytes"])
        if actual != expected:
            offset = next((index for index in range(min(len(actual), len(expected))) if actual[index] != expected[index]), min(len(actual), len(expected)))
            index = bisect.bisect_right(reference["ends"], offset)
            raise AssertionError((offset, index, cases[index], actual[max(0, offset - 20):offset + 110].hex(), expected[max(0, offset - 20):offset + 110].hex()))
    report = {"passed": True, "cases": len(cases), "bytes": len(expected), "reference": "actual UnifiedPalette.rebuild_places", "scope": "default local places, host/target chips, empty/search/raw colon/slot sections, all verb drafts, Unicode match byte ranges, needs-you/running/current rows, source pill/placeholder/action order, history deduplication, offline data and empty shortcuts", "remaining": "chat summaries, decayed/persisted recents, argument completion and GUI/file levels require host services"}
    (TARGET / "palette-model-vectors.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
