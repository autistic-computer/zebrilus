import argparse
import json
import os
from pathlib import Path
import runpy
import subprocess


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/port"
BASE = runpy.run_path(str(Path(__file__).with_name("window_chooser_vectors.py")))
LIFECYCLE = runpy.run_path(str(Path(__file__).with_name("window_chooser_lifecycle_vectors.py")))
uint = BASE["uint"]
text = BASE["text"]
item = BASE["item"]


def units(value, characters):
    return sum(2 if ord(character) > 0xFFFF else 1 for character in value[:characters])


def size(value, characters):
    return len(value[:characters].encode())


def traces():
    big = "x" * 65534
    unicode = "a\x00b\u0085c 😀d"
    return [
        [("set", "de", 2), ("mark", "v", 1, 1), ("mark", "vか", 2, 2), ("commit", "v")],
        [("set", "sh", 1), ("mark", "か", 0, 1), ("mark", "かな", 1, 2), ("mark", "かな", 2, 2), ("commit", "漢字"), ("nav", 1)],
        [("set", "devshell", None), ("select", 3, 8), ("mark", "x", 1, 1), ("mark", "", 0, 0), ("nav", 1)],
        [("set", "devshell", None), ("select", 0, 3), ("mark", "k", 1, 1), ("mark", "ka", 0, 2), ("unmark",), ("nav", -1), ("mark", "y", 1, 1), ("commit", "")],
        [("set", "dev", 1), ("mark", unicode, 0, 0), ("mark", unicode, 2, 4), ("mark", unicode, 3, 7), ("mark", unicode, 7, 8), ("mark", unicode, 8, 9), ("unmark",)],
        [("set", "dev", 3), ("mark", "😀😀", 1, 1), ("mark", "😀😀", 1, 2), ("mark", "\x00\x7f", 1, 2), ("mark", "λ", 0, 1), ("unmark",)],
        [("set", "shell", None), ("select", 1, 4), ("mark", "\u0085", 1, 1), ("mark", "e", 1, 1), ("commit", "el")],
        [("set", big, None), ("mark", "abc", 3, 3), ("mark", "ab", 2, 2), ("mark", "abc", 3, 3), ("commit", "abc"), ("unmark",)],
        [("set", big, None), ("select", 0, 3), ("mark", "abcde", 5, 5), ("commit", "abcdef"), ("unmark",)],
        [("set", "x" * 65536, None), ("mark", "a", 1, 1), ("select", 0, 1), ("mark", "b", 1, 1), ("commit", "b")],
        [("set", "", None), ("mark", "s", 1, 1), ("mark", "sc", 2, 2), ("unmark",), ("mark", "r", 1, 1), ("commit", "ratch"), ("nav", 1)],
        [("set", "4:docs", 2), ("mark", "", 0, 0), ("mark", "\r\n", 1, 2), ("mark", "q", 1, 1), ("mark", "\r\n", 2, 2), ("mark", "", 0, 0)],
    ]


def native(trace):
    operations = []
    for operation in trace:
        kind = operation[0]
        if kind == "set":
            value, cursor = operation[1], operation[2]
            operations.append([1, value, None if cursor is None else size(value, cursor)])
        elif kind == "select":
            operations.append([17, operation[1], operation[2]])
        elif kind == "mark":
            value = operation[1]
            operations.append([14, value, units(value, operation[2]), units(value, operation[3])])
        elif kind == "unmark":
            operations.append([15])
        elif kind == "commit":
            operations.append([16, operation[1]])
        else:
            operations.append([2, operation[1]])
    return operations


def request(datasets, cases, expected):
    output = bytearray()
    uint(output, len(datasets))
    for rows in datasets:
        uint(output, 0)
        uint(output, len(rows))
        for row in rows:
            text(output, row[0])
            text(output, row[1])
            for value in row[2:6]:
                uint(output, value)
            BASE["optional"](output, row[6], uint)
            text(output, row[7])
            text(output, row[8])
    uint(output, len(cases))
    for (dataset, grouped, trace), states in zip(cases, expected["traces"], strict=True):
        uint(output, dataset)
        uint(output, int(grouped))
        uint(output, len(trace))
        for operation in trace:
            kind = operation[0]
            if kind == "set":
                uint(output, 1)
                text(output, operation[1])
                BASE["optional"](output, None if operation[2] is None else size(operation[1], operation[2]), uint)
            elif kind == "select":
                uint(output, 17)
                uint(output, operation[1])
                uint(output, operation[2])
            elif kind == "mark":
                uint(output, 14)
                text(output, operation[1])
                uint(output, size(operation[1], operation[2]))
                uint(output, size(operation[1], operation[3]))
            elif kind == "unmark":
                uint(output, 15)
            elif kind == "commit":
                uint(output, 16)
                text(output, operation[1])
            else:
                uint(output, 2)
                uint(output, int(operation[1] < 0))
                uint(output, abs(operation[1]))
        assert len(states) == len(trace) + 1
    return output


def state_bytes(state):
    output = LIFECYCLE["state_bytes"](state)
    text(output, state["input"])
    uint(output, state["input_cursor"])
    return output


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    parser = argparse.ArgumentParser()
    parser.add_argument("--skip-native-build", action="store_true")
    options = parser.parse_args()
    TARGET.mkdir(parents=True, exist_ok=True)
    base = BASE["vectors"]()["datasets"][1]
    unicode = [item("かな", "", 0, 3, flags=7), item("1:漢字", "", 1, 30, 1, 6), item("a\x00b", "", 2, 31, 2, 4, 0), item("😀d", "", 0, 4, flags=2)]
    datasets = [base, base + unicode]
    cases = [(index % 2, grouped, trace) for index, trace in enumerate(traces()) for grouped in (False, True)]
    corpus = {"datasets": datasets, "selected": [0, 0], "traces": [{"initial": dataset, "grouped": grouped, "operations": native(trace)} for dataset, grouped, trace in cases]}
    environment = os.environ.copy()
    environment["CARGO_TARGET_DIR"] = str(ROOT / "target/penna-native")
    command = ["cargo", "build", "--locked", "--offline", "-p", "zz-ui", "--features", "scene-fixtures", "--example", "window_chooser_lifecycle_vectors"]
    for setting in BASE["SETTINGS"]:
        command.extend(["--config", setting])
    if not options.skip_native_build:
        subprocess.run(command, cwd=ROOT, env=environment, check=True, timeout=600)
    source = subprocess.run([str(ROOT / "target/penna-native/debug/examples/window_chooser_lifecycle_vectors")], input=json.dumps(corpus).encode(), cwd=ROOT, capture_output=True, check=True, timeout=120)
    expected = json.loads(source.stdout)
    (TARGET / "window-chooser-composition-reference.json").write_text(json.dumps(expected))
    request_path = TARGET / "window-chooser-composition-input.bin"
    output_path = TARGET / "window-chooser-composition-output.bin"
    request_path.write_bytes(request(datasets, cases, expected))
    binary = TARGET / "window-chooser-composition-probe"
    subprocess.run(["pixi", "run", "bash", "-c", 'mojo build --fp-mode=contract=off -I src -I "$1/mojo" -D MJ_RASTER=swash -Xlinker build/lib/libmjswash.a $MJ_LINK -Xlinker build/lib/libSDL3.a -Xlinker -lgcc_s -Xlinker -ldl -Xlinker -lrt -Xlinker -lutil "$1/mojo/tests/window_chooser_composition_probe.mojo" -o "$2"', "_", str(ROOT), str(binary)], cwd=ROOT.parent / "mjui", check=True, timeout=600)
    subprocess.run([str(binary), str(request_path), str(output_path)], cwd=ROOT, check=True, timeout=120)
    actual = output_path.read_bytes()
    offset = 0
    for trace_index, states in enumerate(expected["traces"]):
        for step, state in enumerate(states):
            wanted = state_bytes(state)
            if actual[offset:offset + len(wanted)] != wanted:
                raise AssertionError(("window chooser composition mismatch", trace_index, step, cases[trace_index][2][step - 1] if step else None, {key: state[key] if not isinstance(state[key], str) or len(state[key]) < 80 else len(state[key]) for key in ("query", "cursor", "input", "input_cursor", "selected", "rows")}))
            offset += len(wanted)
    if offset != len(actual):
        raise AssertionError(("window chooser composition output length", offset, len(actual)))
    report = {"passed": True, "reference": "actual CommandPaletteView with InputState replace_and_mark_text_in_range, unmark_text, replace_text_in_range and set_selected_range", "traces": len(cases), "states": sum(map(len, expected["traces"])), "bytes": offset}
    (TARGET / "window-chooser-composition-differential.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
