import argparse
import itertools
import json
import os
from pathlib import Path
import runpy
import subprocess


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/port"
BASE = runpy.run_path(str(Path(__file__).with_name("window_chooser_vectors.py")))
uint = BASE["uint"]
text = BASE["text"]
pair = BASE["pair"]
optional = BASE["optional"]
item = BASE["item"]


def vectors():
    source = BASE["vectors"]()["datasets"][:35]
    datasets = []
    selected = []
    for rows in source:
        for initial in dict.fromkeys((0, max(len(rows) - 1, 0), len(rows) + 1, (1 << 32) - 1)):
            datasets.append(rows)
            selected.append(initial)
    traces = []
    queries = ("shell", "", " ", "😀", "λ🦀", "@20", "\x00", "\u001c", "other", "missing")
    for initial, grouped in itertools.product(range(len(datasets)), (False, True)):
        query = queries[initial % len(queries)]
        cursor = len(query.encode())
        operations = [[2, 1], [2, -1], [5, 99999], [6, False], [1, query, None], [1, query, 0], [12], [1, "", 0], [3, True], [3, False], [6, True], [10, query], [0, initial, 1], [0, initial, 0], [0, initial, (1 << 64) - 1], [1, query, cursor], [1, "", None], [9], [8], [7]]
        traces.append({"initial": initial, "grouped": grouped, "operations": operations})
    session = lambda flags: item("dev", "", 0, 1, 0, flags)
    window = lambda flags: item("0:editor", "", 1, 10, 1, flags)
    pane = item("shell", "", 2, 11, 2, 4, 0)
    other = lambda flags: item("other", "", 0, 2, 0, flags)
    phases = [[session(6)], [session(7), window(6)], [session(7), window(7), pane], [other(2), session(7), window(7), pane], [other(3), item("0:child", "", 1, 20, 1, 0), session(7), window(7), pane]]
    offset = len(datasets)
    datasets.extend(phases)
    selected.extend([0] * len(phases))
    for grouped in (False, True):
        traces.append({"initial": offset, "grouped": grouped, "operations": [[3, True], [0, offset + 1, 2], [2, 1], [3, True], [0, offset + 2, 3], [2, 1], [3, False], [1, "shell", None], [1, "", None], [7]]})
        traces.append({"initial": offset + 3, "grouped": grouped, "operations": [[1, "shell", None], [4, False], [0, offset + 4, 2], [7], [4, True], [0, offset + 4, 3]]})
        traces.append({"initial": offset + 2, "grouped": grouped, "operations": [[5, 2], [1, "shell", None], [0, offset + 3, 2], [0, offset + 4, 3], [1, "", None], [0, offset + 3, 4], [0, offset + 2, 5], [4, True]]})
    return {"datasets": datasets, "selected": selected, "traces": traces}


def request(corpus, expected):
    output = bytearray()
    uint(output, len(corpus["datasets"]))
    for index, dataset in enumerate(corpus["datasets"]):
        uint(output, corpus["selected"][index])
        uint(output, len(dataset))
        for row in dataset:
            text(output, row[0])
            text(output, row[1])
            for value in row[2:6]:
                uint(output, value)
            optional(output, row[6], uint)
            text(output, row[7])
            text(output, row[8])
    uint(output, len(corpus["traces"]))
    for trace, states in zip(corpus["traces"], expected["traces"], strict=True):
        uint(output, trace["initial"])
        uint(output, int(trace["grouped"]))
        uint(output, len(trace["operations"]))
        for operation, state in zip(trace["operations"], states[1:], strict=True):
            kind = operation[0]
            uint(output, kind)
            if kind == 0:
                uint(output, operation[1])
                uint(output, operation[2])
            elif kind == 1:
                text(output, state["input"])
                uint(output, state["input_cursor"])
            elif kind == 10:
                text(output, operation[1])
            elif kind == 2:
                uint(output, int(operation[1] < 0))
                uint(output, abs(operation[1]))
            elif kind in (3, 4, 6):
                uint(output, int(operation[1]))
            elif kind == 5:
                uint(output, operation[1])
    return output


def state_bytes(state):
    output = bytearray()
    uint(output, state["revision"])
    for name in ("local", "window", "finishing", "navigation"):
        uint(output, int(state[name]))
    optional(output, state["selected"], uint)
    text(output, state["query"])
    uint(output, state["cursor"])
    uint(output, len(state["rows"]))
    for value in state["rows"]:
        uint(output, value)
    uint(output, len(state["expanded"]))
    for value in state["expanded"]:
        pair(output, value)
    optional(output, state["pending"], lambda output, value: (pair(output, value[0]), uint(output, int(value[1]))))
    optional(output, state["selection"], pair)
    optional(output, state["activation"], pair)
    uint(output, len(state["frames"]))
    for frame in state["frames"]:
        uint(output, len(frame))
        output.extend(frame)
    uint(output, len(state["events"]))
    for event in state["events"]:
        uint(output, event)
    optional(output, state["scroll"], lambda output, value: [uint(output, int(item)) for item in value])
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
    command = ["cargo", "build", "--locked", "--offline", "-p", "zz-ui", "--features", "scene-fixtures", "--example", "window_chooser_lifecycle_vectors"]
    for setting in BASE["SETTINGS"]:
        command.extend(["--config", setting])
    if not options.skip_native_build:
        subprocess.run(command, cwd=ROOT, env=environment, check=True, timeout=600)
    native = subprocess.run([str(ROOT / "target/penna-native/debug/examples/window_chooser_lifecycle_vectors")], input=json.dumps(corpus).encode(), cwd=ROOT, capture_output=True, check=True, timeout=120)
    expected = json.loads(native.stdout)
    (TARGET / "window-chooser-lifecycle-reference.json").write_text(json.dumps(expected))
    request_path = TARGET / "window-chooser-lifecycle-input.bin"
    output_path = TARGET / "window-chooser-lifecycle-output.bin"
    binary = TARGET / "window-chooser-lifecycle-probe"
    request_path.write_bytes(request(corpus, expected))
    subprocess.run(["pixi", "run", "--manifest-path", str(ROOT / "mojo/pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(ROOT / "mojo"), str(ROOT / "mojo/tests/window_chooser_lifecycle_probe.mojo"), "-o", str(binary)], cwd=ROOT, check=True, timeout=300)
    subprocess.run([str(binary), str(request_path), str(output_path)], cwd=ROOT, check=True, timeout=120)
    wanted = b"".join(state_bytes(state) for trace in expected["traces"] for state in trace)
    actual = output_path.read_bytes()
    if actual != wanted:
        (TARGET / "window-chooser-lifecycle-expected.bin").write_bytes(wanted)
        mismatch = next((index for index, pair in enumerate(zip(actual, wanted)) if pair[0] != pair[1]), min(len(actual), len(wanted)))
        offset = 0
        location = None
        for trace_index, trace in enumerate(expected["traces"]):
            for step, state in enumerate(trace):
                end = offset + len(state_bytes(state))
                if offset <= mismatch < end:
                    location = (trace_index, step, state)
                offset = end
        raise AssertionError(("window lifecycle source mismatch", mismatch, len(actual), len(wanted), location))
    report = {"passed": True, "reference": "actual CommandPaletteView entity, InputState callbacks and captured native requests under TestAppContext", "traces": len(expected["traces"]), "states": sum(map(len, expected["traces"])), "bytes": len(wanted), "scope": "post-editor query/caret boundary, revision/target selection, navigation, temporary expansion, pending activation, close/default events, native frames and exact deferred scroll requests; no UI or socket integration yet"}
    (TARGET / "window-chooser-lifecycle-differential.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
