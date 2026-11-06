#!/usr/bin/env python3
import argparse
from array import array
import json
from pathlib import Path
import runpy
import selectors
import subprocess
import sys
import tempfile
import time


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/port"
LIMIT = 64 * 1024 * 1024
TAGS = ["UpsertSession", "RemoveSession", "SessionName", "SessionActiveWindow", "SessionViewers",
        "UpsertWindow", "RemoveWindow", "WindowName", "WindowActivePane", "WindowLayout",
        "WindowFlags", "WindowPresentation", "UpsertPane", "RemovePane", "PaneTitle",
        "PaneState", "PanePresentation", "PaneStatus", "PaneSummary"]


def identifier(output, value):
    output.extend((value["high"], value["low"]))


def optional_id(output, value):
    output.append(int(value["$"] == "Some"))
    if value["$"] == "Some":
        identifier(output, value["value"])


def node_id(output, value):
    output.extend((value["key"]["high"], value["key"]["low"]))


def pane_body(output, value):
    identifier(output, value["id"])
    output.extend(value[field] for field in ("title", "state", "presentation", "status", "summary"))


def window(output, entry):
    value = entry["value"]
    node_id(output, entry)
    output.extend((entry["key"]["major"], value["name"]))
    identifier(output, value["active_pane"])
    output.extend(value[field] for field in ("layout", "flags", "presentation"))
    output.append(len(value["panes"]))
    for entry in value["panes"]:
        node_id(output, entry)
        pane_body(output, entry["value"])


def session(output, entry):
    value = entry["value"]
    node_id(output, entry)
    output.append(value["name"])
    identifier(output, value["active_window"])
    output.extend((value["viewers"], len(value["windows"])))
    for entry in value["windows"]:
        window(output, entry)


def snapshot(output, value):
    identifier(output, value["generation"])
    optional_id(output, value["focused"])
    output.append(len(value["sessions"]))
    for entry in value["sessions"]:
        session(output, entry)


def operation(output, value):
    tag = TAGS.index(value["$"])
    output.append(tag)
    if tag == 0:
        session(output, value["session"])
        return
    if tag == 1:
        identifier(output, value["id"])
        return
    identifier(output, value["session"])
    if tag in (2, 4):
        output.append(value["name" if tag == 2 else "viewers"])
        return
    if tag == 5:
        window(output, value["window"])
        return
    identifier(output, value["window"])
    if tag in (3, 6):
        return
    if tag in (7, 9, 11):
        output.append(value[{7: "name", 9: "layout", 11: "presentation"}[tag]])
        return
    if tag == 10:
        output.extend((value["index"], value["flags"]))
        return
    if tag == 12:
        pane_body(output, value["pane"])
        return
    identifier(output, value["pane"])
    if tag in (8, 13):
        return
    output.append(value[{14: "title", 15: "state", 16: "presentation", 17: "status", 18: "summary"}[tag]])


def update(output, kind, value):
    if kind == "delta":
        identifier(output, value["base"])
        identifier(output, value["version"])
    elif kind == "view":
        optional_id(output, value["focused"])
    ops = value if kind == "overlay" else value["ops"]
    output.append(len(ops))
    for item in ops:
        operation(output, item)


def outcome(output, value):
    failure = value["error"]
    if failure["$"] == "None":
        status = 0
        output.append(status)
    else:
        failure = failure["value"]
        status = ["", "BaseMismatch", "MissingSession", "MissingWindow", "MissingPane"].index(failure["$"])
        output.append(status)
        if status == 1:
            identifier(output, failure["expected"])
            identifier(output, failure["received"])
        else:
            identifier(output, failure["id"])
    snapshot(output, value["snapshot"])
    return status


def packed(words):
    if sys.byteorder == "little":
        return words.tobytes()
    value = array("I", words)
    value.byteswap()
    return value.tobytes()


def worker():
    return subprocess.Popen([str(TARGET / "zz-bend-tree"), "serve"], stdin=subprocess.PIPE,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE)


def command(kind, paths):
    return ("tree " + kind + " " + " ".join(map(str, paths)) + "\n").encode()


def request(process, kind, paths, timeout=30):
    start = time.perf_counter()
    process.stdin.write(command(kind, paths))
    process.stdin.flush()
    with selectors.DefaultSelector() as selector:
        selector.register(process.stdout, selectors.EVENT_READ)
        if not selector.select(timeout):
            process.kill()
            raise RuntimeError("tree worker timed out")
    response = process.stdout.readline()
    if not response:
        raise RuntimeError(process.stderr.read().decode())
    return int(response), time.perf_counter() - start


def close(process):
    process.stdin.close()
    status = process.wait(timeout=10)
    if status != 0:
        raise RuntimeError(process.stderr.read().decode())


def peak(pid):
    for line in Path(f"/proc/{pid}/status").read_text().splitlines():
        if line.startswith("VmHWM:"):
            return int(line.split()[1]) * 1024
    raise RuntimeError("no worker memory high-water mark")


def malformed(paths, kind, current, patch):
    paths[0].write_bytes(current)
    paths[1].write_bytes(patch)
    paths[2].write_bytes(b"unchanged")
    result = subprocess.run([str(TARGET / "zz-bend-tree"), "serve"],
                            input=command(kind, paths), capture_output=True, timeout=10)
    assert result.returncode == 65, result.stderr.decode()
    assert paths[2].read_bytes() == b"unchanged"


def profile(paths, minimum, maximum):
    values = []
    patch = array("I", [1, 1, 0xffffffff, 0xffffffff])
    for count in (1000, 16000, 65535, 262144, 1048576, maximum):
        if count < minimum or count > maximum or any(row["sessions"] == count for row in values):
            continue
        current = array("I", [0, 17, 0, count])
        for index in range(count, 0, -1):
            current.extend((0xffff0000, index, 0, 0, 0, 1, 0))
        paths[0].write_bytes(packed(current))
        paths[1].write_bytes(packed(patch))
        process = worker()
        status, elapsed = request(process, "overlay", paths, timeout=300)
        assert status == 0, status
        expected = array("I", [0, 0, 17, 0, count])
        for index in range(1, count + 1):
            expected.extend((0xffff0000, index, 0, 0, 0, 1, 0))
        assert paths[2].read_bytes() == packed(expected)
        high_water = peak(process.pid)
        close(process)
        values.append({"sessions": count, "normalized_input_bytes": len(current) * 4,
                       "seconds": round(elapsed, 6), "worker_peak_bytes": high_water})
        print(json.dumps({"profile": values[-1]}), flush=True)
    return values


def main():
    runpy.run_path(str(Path(__file__).with_name("guard.py")))["ensure"]()
    parser = argparse.ArgumentParser()
    parser.add_argument("--profile", action="store_true")
    parser.add_argument("--minimum-sessions", type=int, default=1)
    parser.add_argument("--maximum-sessions", type=int, default=65535)
    args = parser.parse_args()
    maximum = (LIMIT - 64 - 16 - 16) // 28
    if args.maximum_sessions < 1 or args.maximum_sessions > maximum:
        raise SystemExit(f"maximum sessions must be between 1 and {maximum}")
    if args.minimum_sessions < 1 or args.minimum_sessions > args.maximum_sessions:
        raise SystemExit("minimum sessions must be between 1 and maximum sessions")
    oracle = json.loads((TARGET / "tree-reducer-oracle.json").read_text())
    compared = 0
    accepted = 0
    with tempfile.TemporaryDirectory(prefix="zz-bend-tree-") as directory:
        paths = [Path(directory) / name for name in ("current.bin", "update.bin", "output.bin")]
        process = worker()
        for row in oracle["vectors"]:
            current = array("I")
            snapshot(current, row["before"])
            patch = array("I")
            update(patch, row["kind"], row["update"])
            expected = array("I")
            status = outcome(expected, row["outcome"])
            paths[0].write_bytes(packed(current))
            paths[1].write_bytes(packed(patch))
            actual, _ = request(process, row["kind"], paths)
            assert actual == status, row["name"]
            assert paths[2].read_bytes() == packed(expected), row["name"]
            accepted += int(status == 0)
            compared += len(expected)
        close(process)
        valid = oracle["vectors"][0]
        current = array("I")
        snapshot(current, valid["before"])
        patch = array("I")
        update(patch, valid["kind"], valid["update"])
        malformed(paths, valid["kind"], packed(current)[:-1], packed(patch))
        malformed(paths, valid["kind"], packed(current) + packed(array("I", [42])), packed(patch))
        malformed(paths, "overlay", packed(current), packed(array("I", [0xffffffff])))
        malformed(paths, "overlay", packed(array("I", [0, 0, 0, 0xffffffff])), packed(array("I", [0])))
        malformed(paths, "overlay", packed(current), packed(array("I", [1, 19, 0, 0])))
        malformed(paths, "overlay", packed(array("I", [0, 0, 2, 0])), packed(array("I", [0])))
        pane = [0, 3, 0, 3, 0, 0, 0, 0, 0]
        repeated = array("I", [0, 0, 0, 1, 0, 1, 0, 0, 0, 0, 1,
                               0, 2, 0, 0, 0, 0, 0, 0, 0, 2] + pane + pane)
        malformed(paths, "overlay", packed(repeated), packed(array("I", [0])))
        paths[0].write_bytes(packed(current))
        with paths[1].open("wb") as output:
            output.truncate(LIMIT)
        paths[2].write_bytes(b"unchanged")
        oversized = subprocess.run([str(TARGET / "zz-bend-tree"), "serve"],
                                   input=command("overlay", paths), capture_output=True, timeout=10)
        assert oversized.returncode == 65
        assert paths[2].read_bytes() == b"unchanged"
        profiles = profile(paths, args.minimum_sessions, args.maximum_sessions) if args.profile else []
    report = {"reference": oracle["reference"], "cases": len(oracle["vectors"]),
              "accepted": accepted, "rejected": len(oracle["vectors"]) - accepted,
              "normalized_output_words_compared": compared, "atomic_malformed_cases": 8,
              "maximum_chunk_bytes": 65536, "combined_input_budget_bytes": LIMIT - 64,
              "profile": profiles, "result": "all native tree and error-ID word comparisons passed"}
    if profiles:
        report["profile_payload_arena"] = [{"group": "session-name", "bytes": [0]},
                                           {"group": "session-viewers", "bytes": [0]}]
    destination = TARGET / ("tree-native-profile.json" if args.profile else "tree-native.json")
    destination.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
