import argparse
from collections import Counter
import json
from pathlib import Path
import re
import runpy
import subprocess
import tempfile
import time


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/port"
COPY = "Left Right Up Down PageUp PageDown HalfPageUp HalfPageDown Top Bottom TopLine MiddleLine BottomLine StartOfLine BackToIndentation EndOfLine NextWord PreviousWord NextWordEnd NextParagraph PreviousParagraph NextPrompt PreviousPrompt SearchAgain StartSelection SelectWord SelectLine ClearSelection ClearSelectionOrCancel ToggleRectangle RectangleOn RectangleOff OtherEnd SetMark JumpToMark Jump RepeatJump CopySelection Cancel NextSpace PreviousSpace NextSpaceEnd ScrollUp ScrollDown ScrollMiddle NextMatchingBracket SearchCursorWord CopyEndOfLine GotoLine PageDownScrollExit SelectionMode StopSelection HalfPageDownScrollExit ScrollDownAndCancel CursorDownAndCancel ScrollExitOn ScrollExitOff ScrollExitToggle CursorCentreVertical CursorCentreHorizontal ScrollTop ScrollBottom TogglePosition RecentreTopBottom PreviousMatchingBracket RefreshOn RefreshOff RefreshToggle RefreshNow LineNumbersOn LineNumbersOff LineNumbersToggle RefreshRevision CopyLine Search MouseCursor".split()
VIEW = {"CopyMode": 16, "SearchBegin": 17, "SearchUpdate": 18, "SearchNext": 19,
        "SearchPrevious": 20, "SearchClose": 21, "EnterCopyModeScrollExit": 26,
        "EnterCopyModeWith": 27, "CopyModeCounted": 28}
DIRECTION = {"Forward": 0, "Backward": 1}
CASE = {"Smart": 0, "Sensitive": 1, "Insensitive": 2}
MODE = {"Literal": 0, "Regex": 1}
KINDS = ["wire", "event", "key", "shortcut", "status", "case", "mode"]


def uint(output, value):
    while value >= 128:
        output.append((value & 127) | 128)
        value >>= 7
    output.append(value)


def text(output, value):
    value = value.encode()
    uint(output, len(value))
    output.extend(value)


def optional_text(output, value):
    uint(output, value is not None)
    if value is not None:
        text(output, value)


def query(output, value):
    text(output, value["text"])
    for item in [MODE[value["mode"]], CASE[value["case"]], DIRECTION[value["direction"]]]:
        uint(output, item)


def split_action(value):
    return (value, {}) if isinstance(value, str) else next(iter(value.items()))


def copy_action(output, value):
    name, fields = split_action(value)
    uint(output, COPY.index(name))
    flag = fields.get("output", fields.get("reverse", fields.get("option_off", fields.get("to", fields.get("option_absolute", False))))) if isinstance(fields, dict) else False
    uint(output, flag)
    text(output, fields.get("text", fields.get("target", "")) if isinstance(fields, dict) else "")
    uint(output, DIRECTION[fields.get("direction", "Forward")] if isinstance(fields, dict) else 0)
    request = fields if name in ("CopySelection", "CopyEndOfLine", "CopyLine") else {}
    uint(output, request.get("request_id", 0))
    uint(output, request.get("clipboard", False))
    buffer = request.get("buffer")
    uint(output, 0 if buffer is None else 2 if buffer == "Append" else 1)
    optional_text(output, buffer["Create"]["prefix"] if isinstance(buffer, dict) else None)
    optional_text(output, request.get("pipe"))
    uint(output, request.get("clear_selection", False))
    uint(output, request.get("cancel", False))
    line = fields.get("line") if name == "GotoLine" else None
    uint(output, line is not None)
    if line is not None:
        uint(output, line * 2 if line >= 0 else -line * 2 - 1)
    uint(output, {"Char": 0, "Word": 1, "Line": 2}[fields] if name == "SelectionMode" else 0)
    uint(output, fields.get("regex", False) if isinstance(fields, dict) else False)
    uint(output, fields.get("incremental", False) if isinstance(fields, dict) else False)
    for key in ["column", "row", "click_count", "rectangle"]:
        uint(output, fields[key] if name == "MouseCursor" else 0)


def normalized(case):
    output = bytearray()
    kind = case["kind"]
    uint(output, KINDS.index(kind))
    if kind == "wire":
        uint(output, case["pane"])
        uint(output, case["output"])
        name, fields = split_action(case["action"])
        uint(output, VIEW[name])
        if name in ("SearchBegin", "SearchUpdate"):
            query(output, fields)
        elif name == "CopyMode":
            copy_action(output, fields)
        elif name == "CopyModeCounted":
            copy_action(output, fields["action"])
            uint(output, fields["count"])
        elif name == "EnterCopyModeWith":
            uint(output, fields["scroll_exit"])
            uint(output, fields["hide_position"])
        return output, bytes(case["expected"])
    if kind == "event":
        uint(output, len(case["request"]))
        output.extend(case["request"])
        expected = bytearray()
        uint(expected, case["pane"])
        uint(expected, DIRECTION[case["direction"]])
        return output, expected
    if kind == "key":
        query(output, case["query"])
    if kind in ("key", "shortcut"):
        uint(output, case.get("key", 0))
        uint(output, case["character"])
        uint(output, case["modifiers"])
        uint(output, ["Press", "Repeat", "Release"].index(case["action"]))
        optional_text(output, case.get("text"))
        if kind == "key":
            expected = bytearray()
            uint(expected, case["changed"])
            query(expected, case["expected_query"])
            return output, expected
        uint(output, case["prefix"])
        uint(output, case["modal"])
        return output, bytes([case["expected"]])
    if kind == "status":
        for key in ["current", "total", "pending", "invalid"]:
            uint(output, case[key])
    elif kind == "mode":
        for key in ["mode", "position", "total", "hide", "unseen"]:
            uint(output, case[key])
        expected = bytearray()
        uint(expected, case["expected"] is not None)
        if case["expected"] is not None:
            optional_text(expected, case["expected"][0])
            text(expected, case["expected"][1])
        return output, expected
    else:
        uint(output, CASE[case["case"]])
    return output, case["expected"].encode()


def read_uint(data, offset):
    result = 0
    shift = 0
    while True:
        part = data[offset]
        offset += 1
        result |= (part & 127) << shift
        if part < 128:
            return result, offset
        shift += 7


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    parser = argparse.ArgumentParser()
    parser.add_argument("--skip-reference", action="store_true")
    parser.add_argument("--skip-build", action="store_true")
    args = parser.parse_args()
    source = (ROOT / "crates/zz-terminal/src/interaction.rs").read_text().split("pub enum CopyModeAction {", 1)[1].split("\n}\n", 1)[0]
    assert re.findall(r"^    ([A-Z][A-Za-z0-9_]*)\s*[({,]", source, re.M) == COPY
    reference = TARGET / "terminal-search-vectors.json"
    if not args.skip_reference:
        with reference.open("w") as output:
            subprocess.run(["cargo", "run", "--locked", "-p", "zz-client", "--example", "terminal_search_vectors"], cwd=ROOT, stdout=output, check=True)
    cases = json.loads(reference.read_text())["cases"]
    probe = TARGET / "terminal-search-probe"
    if not args.skip_build:
        subprocess.run(["pixi", "run", "--manifest-path", str(ROOT / "mojo/pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(ROOT / "mojo"), str(ROOT / "mojo/tests/terminal_search_probe.mojo"), "-o", str(probe)], cwd=ROOT, check=True)
    request = bytearray()
    uint(request, len(cases))
    expected = []
    for case in cases:
        encoded, result = normalized(case)
        request.extend(encoded)
        expected.append(result)
    if len(request) > 16 * 1024 * 1024:
        raise ValueError("search fixture batch exceeds its byte budget")
    started = time.monotonic()
    with tempfile.TemporaryDirectory(prefix="zz-search-") as directory:
        directory = Path(directory)
        source = directory / "request.bin"
        target = directory / "result.bin"
        source.write_bytes(request)
        subprocess.run([str(probe), str(source), str(target)], cwd=ROOT, check=True, timeout=60)
        actual = target.read_bytes()
        offset = 0
        for index, result in enumerate(expected):
            length, offset = read_uint(actual, offset)
            if actual[offset:offset + length] != result:
                raise AssertionError((index, cases[index], list(actual[offset:offset + length]), list(result)))
            offset += length
        assert offset == len(actual)
    report = {"passed": True, "source": "actual terminal action serializer and client search helpers", "cases": len(cases), "kinds": dict(Counter(case["kind"] for case in cases)), "copy_variants": len(COPY), "bytes_compared": sum(map(len, expected)), "batch_bytes": len(request), "seconds": round(time.monotonic() - started, 6)}
    (TARGET / "terminal-search-differential.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
