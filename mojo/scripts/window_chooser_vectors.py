import argparse
import itertools
import json
import os
from pathlib import Path
import random
import runpy
import subprocess


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/port"
SETTINGS = ("profile.dev.debug=0", 'profile.dev.package."*".opt-level=0', "profile.dev.package.ala.opt-level=0", "profile.dev.package.ala-macros.opt-level=0", "profile.dev.package.ala-platform.opt-level=0", "profile.dev.package.taffy.opt-level=0")


def uint(output, value):
    while value >= 128:
        output.append((value & 127) | 128)
        value >>= 7
    output.append(value)


def text(output, value):
    encoded = value.encode()
    uint(output, len(encoded))
    output.extend(encoded)


def pair(output, value):
    for item in value:
        uint(output, item)


def optional(output, value, write):
    uint(output, int(value is not None))
    if value is not None:
        write(output, value)


def item(label="", detail="", kind=0, target=0, depth=0, flags=0, pane=None, key="", custom=""):
    return [label, detail, kind, target, depth, flags, pane, key, custom]


def vectors():
    base = [item("dev", "1 window", 0, 1, flags=7), item("3:server", "2 panes", 1, 10, 1, 7, key="1"), item("shell", "cwd /tmp", 2, 11, 2, 4, 0), item("scratch", "2 windows", 0, 2, flags=3), item("client", "hidden", 3, 12, 255, 255), item("4:docs", "files", 1, 20, 1, 6, key="a", custom="<<reference>>"), item("plain:name", "", 1, 21, 1), item("agent", "λ🦀", 2, (1 << 64) - 1, 2, 4, 3)]
    datasets = [[], base]
    prefixes = ("0", "00", "+0", "+4294967295", "4294967295", "4294967296", "-0", " 1", "1 ", "١", "", "+", "0x1", "1_0", "1:other", "1\x00")
    datasets.append([item("session", kind=0, flags=4)] + [item(prefix + ":tail:😀", kind=1, target=index, depth=index % 4, flags=index % 8) for index, prefix in enumerate(prefixes)])
    generator = random.Random(0x55329A77)
    labels = ("", "NAME", "3:name", "λ🦀", "İı", "ß", "a\x00b", "current", " / ", "a\u001cb", "@0", "+1:port")
    for length in range(1, 25):
        rows = []
        for index in range(length):
            rows.append(item(generator.choice(labels), generator.choice(labels), generator.randrange(4), generator.choice((0, 1, 20, (1 << 63), (1 << 64) - 1)), generator.choice((0, 1, 2, 3, 255)), generator.randrange(256), generator.choice((None, 0, 1, 2, 3)), generator.choice(("", "a", "😀")), generator.choice(("", "", "<<custom>>", "λ"))))
        datasets.append(rows)
    phases = []
    for bits in range(8):
        rows = [row.copy() for row in base]
        for row in rows:
            row[5] = bits
        phases.append(len(datasets))
        datasets.append(rows)
    cases = []
    queries = ("", " ", "\u00a0", "\x1c", "ref", "files", "@20", "$2", "%18446744073709551615", "dev", "scratch", "NAME", "😀", "λ", "i\u0307", "s / w", "0", "-", "tail:")
    for dataset in range(len(datasets)):
        for query, grouped in itertools.product(queries, (False, True)):
            cases.append({"dataset": dataset, "query": query, "grouped": grouped})
    datasets.append([item("hidden" * 14000, kind=3, target=1)])
    cases.append({"dataset": len(datasets) - 1, "query": "", "grouped": True})
    datasets.append([item("raw" * 22000, "secret" * 12000, custom="short")])
    for query in ("", "short", "secret", "raw"):
        cases.append({"dataset": len(datasets) - 1, "query": query, "grouped": True})
    datasets.append([item("x" * 65536)])
    cases.append({"dataset": len(datasets) - 1, "query": " " * 65536, "grouped": True})
    datasets.append([item("row" + str(index), kind=index % 3, target=index, depth=index % 4, flags=7) for index in range(65536)])
    for grouped in (False, True):
        cases.append({"dataset": len(datasets) - 1, "query": "", "grouped": grouped})
    targets = ((0, 1), (1, 10), (2, 11), (3, 12), (1, 20), (2, (1 << 64) - 1), (0, 999))
    traces = []
    for target, desired, searching in itertools.product(targets, (False, True), (False, True)):
        for history in ([], [target], [targets[0], target, targets[2]], [target, target]):
            traces.append({"seed": {"expanded": history, "pending": [target, desired], "selection": targets[5]}, "steps": [[phase, searching] for phase in phases] + [[0, not searching], [1, not searching], [1, searching], [1, searching]]})
    traces.append({"seed": {"expanded": [], "pending": None, "selection": None}, "steps": [[index, searching] for index, searching in itertools.product(range(28), (True, False, True))]})
    return {"datasets": datasets, "cases": cases, "traces": traces}


def requests(vectors):
    output = bytearray()
    uint(output, len(vectors["datasets"]))
    for dataset in vectors["datasets"]:
        uint(output, len(dataset))
        for row in dataset:
            text(output, row[0])
            text(output, row[1])
            for value in row[2:6]:
                uint(output, value)
            optional(output, row[6], uint)
            text(output, row[7])
            text(output, row[8])
    uint(output, len(vectors["cases"]))
    for case in vectors["cases"]:
        uint(output, case["dataset"])
        text(output, case["query"])
        uint(output, int(case["grouped"]))
    uint(output, len(vectors["traces"]))
    for trace in vectors["traces"]:
        seed = trace["seed"]
        uint(output, len(seed["expanded"]))
        for value in seed["expanded"]:
            pair(output, value)
        optional(output, seed["pending"], lambda output, value: (pair(output, value[0]), uint(output, int(value[1]))))
        optional(output, seed["selection"], pair)
        uint(output, len(trace["steps"]))
        for dataset, searching in trace["steps"]:
            uint(output, dataset)
            uint(output, int(searching))
    return output


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    parser = argparse.ArgumentParser()
    parser.add_argument("--skip-native-build", action="store_true")
    options = parser.parse_args()
    TARGET.mkdir(parents=True, exist_ok=True)
    corpus = vectors()
    environment = os.environ.copy()
    environment["CARGO_TARGET_DIR"] = str(ROOT / "target/penna-native")
    command = ["cargo", "build", "--locked", "--offline", "-p", "zz-ui", "--example", "window_chooser_vectors"]
    for setting in SETTINGS:
        command.extend(["--config", setting])
    if not options.skip_native_build:
        subprocess.run(command, cwd=ROOT, env=environment, check=True, timeout=600)
    native = subprocess.run([str(ROOT / "target/penna-native/debug/examples/window_chooser_vectors")], input=json.dumps(corpus).encode(), cwd=ROOT, capture_output=True, check=True, timeout=120)
    expected = json.loads(native.stdout)
    request = TARGET / "window-chooser-input.bin"
    actual = TARGET / "window-chooser-output.bin"
    binary = TARGET / "window-chooser-probe"
    request.write_bytes(requests(corpus))
    subprocess.run(["pixi", "run", "--manifest-path", str(ROOT / "mojo/pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(ROOT / "mojo"), str(ROOT / "mojo/tests/window_chooser_probe.mojo"), "-o", str(binary)], cwd=ROOT, check=True, timeout=300)
    subprocess.run([str(binary), str(request), str(actual)], cwd=ROOT, check=True, timeout=120)
    wanted = bytes(expected["bytes"])
    found = actual.read_bytes()
    if found != wanted:
        (TARGET / "window-chooser-expected.bin").write_bytes(wanted)
        mismatch = next((index for index, pair in enumerate(zip(found, wanted)) if pair[0] != pair[1]), min(len(found), len(wanted)))
        raise AssertionError(("window chooser source mismatch", mismatch, len(found), len(wanted)))
    report = {"passed": True, "reference": "actual UnifiedPalette.rebuild_window_chooser and GUI-delegated WindowChooserSearch", "model_cases": expected["cases"], "rows": expected["rows"], "search_traces": len(corpus["traces"]), "search_states": expected["steps"], "bytes": len(wanted), "resource_refusals": 5, "scope": "source text/custom formats, active hierarchy, numeric window prefix, stable fuzzy score/highlights/metadata, navigation and expansion/collapse pending state; exact query/row/count/projected-byte/history acceptance boundaries and atomic refusals"}
    (TARGET / "window-chooser-differential.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
