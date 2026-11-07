import json
from pathlib import Path
import runpy
import subprocess
import tempfile

from palette_dialog_vectors import uint, blob


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/port"


def text(output, value):
    blob(output, value.encode())


def search(output, value):
    output.append(value is not None)
    if value is not None:
        text(output, value["query"])
        output.append(value["reverse"])


def buffer_summary(output, buffer):
    uint(output, buffer["selected"])
    output.extend((buffer["filter_no_matches"], buffer["help"]))
    text(output, buffer["prompt"])
    uint(output, len(buffer["items"]))
    for item in buffer["items"]:
        for field in ("name", "preview"):
            text(output, item[field])
        for field in ("size_bytes", "created_unix_seconds"):
            uint(output, item[field])
        for field in ("key", "text"):
            text(output, item[field])
        output.append(item["tagged"])
    search(output, buffer["search"])


def codec_summary(message):
    output = bytearray()
    event = message["Event"]
    uint(output, event["sequence"])
    payload = event["payload"]
    update = "ChooseBufferUpdate" in payload
    state = payload.get("ChooseBuffer", {}).get("state")
    output.extend((update, state is not None))
    if state is not None:
        buffer_summary(output, state)
    search(output, payload["ChooseBufferUpdate"]["search"] if update else None)
    uint(output, payload["ChooseBufferUpdate"]["selected"] if update else 0)
    return output


def summary(output, case):
    for field in ("revision", "closed", "buffer_revision"):
        uint(output, int(case[field]))
    tree = case["tree"]
    output.append(tree is not None)
    if tree is not None:
        uint(output, tree["rows"])
        uint(output, tree["selected"])
        search(output, tree["search"])
    buffer = case["buffer"]
    output.append(buffer is not None)
    if buffer is not None:
        buffer_summary(output, buffer)
    plane = case["plane"]
    output.append(plane is not None)
    if plane is not None:
        uint(output, plane["selected"])
        uint(output, plane["rows"])
        for field in ("sort", "view", "prompt_style", "prompt_line"):
            text(output, plane[field])


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    reference = TARGET / "chooser-retention-vectors.json"
    subprocess.run(["cargo", "run", "--quiet", "--locked", "--offline", "-p", "zz-client", "--example", "chooser_retention_vectors", "--", str(reference)], cwd=ROOT, check=True)
    reference_data = json.loads(reference.read_text())
    traces = reference_data["traces"]
    codec = reference_data["codec"]
    data = bytearray()
    expected = bytearray()
    uint(data, len(codec))
    for case in codec:
        blob(data, case["bytes"])
        data.append(case["status"])
        blob(data, codec_summary(case["expected"]) if case["status"] == 1 else b"")
    uint(data, len(traces))
    count = 0
    for trace in traces:
        uint(data, int(trace["initial"]))
        uint(data, len(trace["cases"]))
        for case in trace["cases"]:
            blob(data, case["bytes"])
            row = bytearray()
            summary(row, case)
            expected.extend(row * 3)
            count += 1
    binary = TARGET / "chooser-retention-probe"
    subprocess.run(["pixi", "run", "--manifest-path", str(ROOT / "mojo/pixi.toml"), "mojo", "build", "-I", str(ROOT / "mojo"), str(ROOT / "mojo/tests/chooser_retention_probe.mojo"), "-o", str(binary)], cwd=ROOT, check=True)
    with tempfile.TemporaryDirectory(prefix="zz-chooser-retention-") as directory:
        source, destination = (Path(directory) / name for name in ("input.bin", "output.bin"))
        source.write_bytes(data)
        subprocess.run([str(binary), str(source), str(destination), str(TARGET / "zz-bend")], cwd=ROOT, check=True, timeout=90)
        actual = destination.read_bytes()
    if actual != expected:
        position = next((index for index, pair in enumerate(zip(actual, expected)) if pair[0] != pair[1]), min(len(actual), len(expected)))
        raise AssertionError(f"chooser retention differs at {position}: {actual[max(0,position-16):position+32].hex()} != {expected[max(0,position-16):position+32].hex()}")
    report = {"passed": True, "reference": "actual ClientCore events and GUI-delegated chooser revision advance", "cases": count, "traces": len(traces), "bytes": len(actual), "buffer_codec": {"cases": len(codec), "accepted": sum(case["status"] == 1 for case in codec), "rejected": sum(case["status"] == 0 and not case["refusal"] for case in codec), "bounds": sum(case["refusal"] for case in codec)}, "initial_revisions": [0, 18446744073709551614, 18446744073709551615], "shared_buffer_rows": 65536, "shared_plane_text_bytes": 1048576, "snapshot_updates": 10000, "paths": ["pure reducer", "InteractiveConnection.consume+Bend envelope gate", "AppModel.refresh"]}
    (TARGET / "chooser-retention-differential.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
