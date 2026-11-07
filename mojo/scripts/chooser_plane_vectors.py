import json
from pathlib import Path
import runpy
import subprocess
import tempfile

from terminal_codec_vectors import viewport_fingerprint


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/port"


def uint(output, value):
    while value >= 128:
        output.append((value & 127) | 128)
        value >>= 7
    output.append(value)


def data(output, value):
    uint(output, len(value))
    output.extend(value)


def fingerprint(plane):
    if plane is None:
        return "0,"
    result = [1, plane["selected"], len(plane["rows"])]

    def text(value):
        encoded = value.encode()
        result.extend((len(encoded), *encoded))

    def viewport(value):
        result.append(int(value is not None))
        if value is not None:
            result.extend(viewport_fingerprint(value).rstrip(",").split(","))

    for row in plane["rows"]:
        text(row["name"])
        text(row["text"])
        result.append(int(row["align"]))
    for field in ("sort", "view"):
        text(plane[field])
    result.append(int(plane["filter"]))
    for field in ("selection_style", "border_style", "prompt_style"):
        text(plane[field])
    result.append({"Normal": 0, "Off": 1, "Big": 2}[plane["preview_size"]])
    preview = plane["preview"]
    result.append(int(preview is not None))
    if preview is not None:
        result.append(preview["kind"])
        tiles = preview.get("tiles", [])
        result.append(len(tiles))
        for tile in tiles:
            for field in ("label", "label_style", "border_style"):
                text(tile[field])
            viewport(tile["viewport"])
        result.append(preview.get("current", 0))
        viewport(preview.get("viewport"))
        lines = preview.get("lines", [])
        result.append(len(lines))
        for line in lines:
            text(line)
        text(preview.get("status_style", ""))
        result.append(preview.get("status_width", 0))
        text(preview.get("border_style", ""))
    cursor = plane["prompt_cursor"]
    result.extend((cursor["style"], int(cursor["colour"] is not None)))
    if cursor["colour"] is not None:
        colour = cursor["colour"]
        if isinstance(colour, str):
            kind, value = colour, 0
        else:
            kind, value = next(iter(colour.items()))
        result.extend(({"Basic": 0, "Indexed": 1, "Rgb": 2, "Default": 3, "Terminal": 4, "Theme": 5}[kind], value))
    result.append(plane["prompt_column"])
    text(plane["prompt_line"])
    return ",".join(map(str, result)) + ","


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    reference = TARGET / "chooser-plane-vectors.json"
    subprocess.run(["cargo", "run", "--quiet", "--locked", "--offline", "-p", "zz-protocol", "--example", "chooser_plane_vectors", "--", str(reference)], cwd=ROOT, check=True)
    cases = json.loads(reference.read_text())
    output = bytearray()
    uint(output, len(cases))
    for case in cases:
        data(output, bytes.fromhex(case["bytes"]))
        output.append(case["status"])
        uint(output, int(case["sequence"]))
        data(output, (fingerprint(case["expected"]) if case["status"] == 1 else "").encode())
    binary = TARGET / "chooser-plane-probe"
    subprocess.run(["pixi", "run", "--manifest-path", str(ROOT / "mojo/pixi.toml"), "mojo", "build", "-I", str(ROOT / "mojo"), "-I", str(ROOT / "mojo/tests"), str(ROOT / "mojo/tests/chooser_plane_probe.mojo"), "-o", str(binary)], cwd=ROOT, check=True)
    with tempfile.TemporaryDirectory(prefix="zz-chooser-plane-") as directory:
        path = Path(directory) / "vectors.bin"
        path.write_bytes(output)
        result = subprocess.run([str(binary), str(path)], cwd=ROOT, capture_output=True, text=True)
        if result.returncode:
            print(result.stdout, end="")
            print(result.stderr, end="")
            result.check_returncode()
        assert int(result.stdout) == len(cases), result.stdout
    report = {"passed": True, "reference": "actual zz-protocol postcard decode including all five chooser previews and packed terminal metadata", "cases": len(cases), "accepted": sum(case["status"] == 1 for case in cases), "ignored": sum(case["status"] == 2 for case in cases), "rejected": sum(case["status"] == 0 and not case["refusal"] for case in cases), "bounds": sum(case["refusal"] for case in cases), "bytes": len(output)}
    (TARGET / "chooser-plane-differential.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
