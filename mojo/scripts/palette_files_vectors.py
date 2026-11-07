import json
from pathlib import Path
import runpy
import subprocess
import tempfile

from path_list_vectors import uint, text


ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "mojo"
TARGET = ROOT / "target/port"


def optional(output, value):
    output.append(value is not None)
    if value is not None:
        text(output, value)


def expected(case):
    output = bytearray()
    before = case["before"]
    for field in ["open", "absolute", "folder", "copy"]:
        optional(output, before[field])
    text(output, before["empty"])
    output.append(before["inserts"])
    uint(output, len(before["hints"]))
    for key, label in before["hints"]:
        text(output, key)
        text(output, label)
    uint(output, case["effect"][0])
    text(output, case["effect"][1])
    uint(output, case["after"]["selected"])
    optional(output, case["after"]["pending"])
    text(output, case["clipboard"])
    uint(output, len(case["calls"]))
    for call in case["calls"]:
        uint(output, call[0])
        uint(output, call[1])
        text(output, call[2])
        text(output, call[3])
        optional(output, call[4])
        text(output, call[5])
        output.extend([call[6], call[7]])
    return output


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    source = subprocess.run(["cargo", "run", "--quiet", "--locked", "--offline", "-j", "1", "-p", "zz-ui", "--no-default-features", "--features", "path-rank-fixtures", "--example", "palette_files_vectors"], cwd=ROOT, check=True, stdout=subprocess.PIPE, timeout=300)
    values = json.loads(source.stdout)
    payload = bytearray()
    uint(payload, len(values["cases"]))
    records = []
    for case in values["cases"]:
        data = case["input"]
        for field in data["path"]:
            text(payload, field)
        uint(payload, len(data["rows"]))
        for rel, name, directory, symlink in data["rows"]:
            text(payload, rel)
            text(payload, name)
            payload.extend([directory, symlink])
        uint(payload, data["selected"])
        payload.append(data["root"] is not None)
        if data["root"] is not None:
            text(payload, data["root"][0])
            text(payload, data["root"][1])
        payload.extend([data["here"], data["insert"]])
        uint(payload, data["key"])
        records.append(expected(case))
    binary = TARGET / "palette-files-vectors-probe"
    subprocess.run(["pixi", "run", "--manifest-path", str(PORT / "pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(PORT), str(PORT / "tests/palette_files_vectors_probe.mojo"), "-o", str(binary)], cwd=ROOT, check=True)
    with tempfile.TemporaryDirectory(prefix="zz-files-vectors-", dir="/tmp") as directory:
        path = Path(directory)
        (path / "input").write_bytes(payload)
        subprocess.run([str(binary), str(path / "input"), str(path / "output")], check=True, timeout=60)
        actual = (path / "output").read_bytes()
        offset = 0
        for index, wanted in enumerate(records):
            if actual[offset:offset + len(wanted)] != wanted:
                raise AssertionError("file action differs " + json.dumps(values["cases"][index]) + " actual=" + repr(actual[offset:offset + len(wanted)]))
            offset += len(wanted)
        if offset != len(actual):
            raise AssertionError("file action output length differs")
    report = {"passed": True, "cases": len(records), "bytes": offset, "source": values["source"], "scope": "all native action keys, empty/out-of-range selection, folder-vs-file choice, root metadata, remote copy, insertion, parent edges, clipboard effects and hints"}
    (TARGET / "palette-files-vectors.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
