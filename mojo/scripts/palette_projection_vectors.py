import argparse
import bisect
import json
from pathlib import Path
import runpy
import subprocess
import tempfile

from palette_places_vectors import uint, text


ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "mojo"
TARGET = ROOT / "target/port"
ORACLE = ROOT / "target/debug/examples/palette_projection_vectors"


def catalog_source(catalog):
    data = bytearray()
    uint(data, len(catalog))
    for id, name, commands in catalog:
        text(data, id)
        text(data, name)
        uint(data, len(commands))
        for command in commands:
            text(data, command)
    return '''comptime PALETTE_PROGRAM_DATA = "''' + data.hex() + '''"


@inline(.never)
def program_bytes() -> List[UInt8]:
    var data = StringSlice(PALETTE_PROGRAM_DATA).as_bytes()
    var output = List[UInt8](capacity=len(data) // 2)
    for offset in range(0, len(data), 2):
        var value: UInt8 = 0
        for index in range(2):
            var digit = data[offset + index]
            value = (value << 4) | (digit - 48 if digit <= 57 else digit - 87)
        output.append(value)
    return output^
'''


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    parser = argparse.ArgumentParser()
    parser.add_argument("--generate", action="store_true")
    options = parser.parse_args()
    subprocess.run(["cargo", "build", "--offline", "-j", "1", "-p", "zz-client", "--example", "palette_projection_vectors"], cwd=ROOT, check=True)
    source = json.loads(subprocess.check_output([str(ORACLE)]))
    generated = catalog_source(source["catalog"])
    destination = PORT / "zzmojo/palette_program_data.mojo"
    if options.generate:
        destination.write_text(generated)
    assert destination.read_text() == generated, "palette program catalog changed; regenerate with --generate"
    binary = TARGET / "palette-projection-probe"
    subprocess.run(["pixi", "run", "--manifest-path", str(PORT / "pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(PORT), str(PORT / "tests/palette_projection_probe.mojo"), "-o", str(binary)], cwd=ROOT, check=True)
    packet = bytearray()
    uint(packet, len(source["cases"]))
    expected = bytearray()
    ends = []
    for case in source["cases"]:
        uint(packet, len(case["request"]))
        packet.extend(case["request"])
        expected.extend(case["expected"])
        ends.append(len(expected))
    with tempfile.TemporaryDirectory(prefix="zz-palette-projection-", dir="/tmp") as directory:
        input_path = Path(directory) / "in"
        output_path = Path(directory) / "out"
        input_path.write_bytes(packet)
        subprocess.run([str(binary), str(input_path), str(output_path)], check=True)
        actual = output_path.read_bytes()
        if actual != expected:
            offset = next((index for index in range(min(len(actual), len(expected))) if actual[index] != expected[index]), min(len(actual), len(expected)))
            index = bisect.bisect_right(ends, offset)
            raise AssertionError((offset, index, source["cases"][index]["name"], actual[max(0, offset - 20):offset + 80].hex(), expected[max(0, offset - 20):offset + 80].hex()))
    subprocess.run(["pixi", "run", "--manifest-path", str(PORT / "pixi.toml"), "mojo", "run", "--fp-mode=contract=off", "-I", str(PORT), str(PORT / "tests/palette_projection_bounds_probe.mojo")], cwd=ROOT, check=True)
    report = {"passed": True, "cases": len(source["cases"]), "bytes": len(expected), "programs": len(source["catalog"]), "explicit_refusals": 4, "reference": "actual zz-client navigation/program/window_summary helpers", "scope": "actual protocol snapshots, numeric session labels and path ties, pane labels across all kinds, layout/map order and pane-order/base-index saturation, detected programs with exact 128-byte normalization/path/extension/first-match rules"}
    (TARGET / "palette-projection-vectors.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
