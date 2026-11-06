import argparse
import json
from pathlib import Path
import runpy
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "mojo"
TARGET = ROOT / "target/port"


def uint(output, value):
    while value >= 128:
        output.append((value & 127) | 128)
        value >>= 7
    output.append(value)


def blob(output, value):
    uint(output, len(value))
    output.extend(value)


def chunks(data):
    offset = 0
    while offset < len(data):
        length = 0
        shift = 0
        while data[offset] & 128:
            length |= (data[offset] & 127) << shift
            offset += 1
            shift += 7
        length |= data[offset] << shift
        offset += 1
        yield data[offset:offset + length]
        offset += length


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    parser = argparse.ArgumentParser()
    parser.add_argument("--skip-build", action="store_true")
    parser.add_argument("--skip-reference", action="store_true")
    options = parser.parse_args()
    TARGET.mkdir(parents=True, exist_ok=True)
    reference = TARGET / "terminal-projection-vectors.json"
    if not options.skip_reference:
        result = subprocess.run(["cargo", "run", "--quiet", "--locked", "-p", "zz-client", "--example", "terminal_projection_vectors"], cwd=ROOT, capture_output=True)
        if result.returncode:
            raise SystemExit(result.stderr.decode())
        current = json.loads(result.stdout)
        reference.write_text(json.dumps(current, indent=2) + "\n")
    else:
        current = json.loads(reference.read_text())
    probe = TARGET / "terminal-projection-probe"
    if not options.skip_build:
        subprocess.run(["pixi", "run", "--manifest-path", str(PORT / "pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(PORT), str(PORT / "tests/terminal_projection_probe.mojo"), "-o", str(probe)], cwd=ROOT, check=True)
    counts = {}
    with tempfile.TemporaryDirectory(prefix="zz-terminal-projection-") as directory:
        temporary = Path(directory)
        source = temporary / "source.bin"
        actual = temporary / "actual.bin"
        for category in ("projections", "local", "sources", "kitty", "recolors", "placements"):
            records = current[category]
            request = bytearray()
            expected = bytearray()
            uint(request, len(records))
            for record in records:
                blob(request, record["request"])
                blob(expected, record["expected"])
            source.write_bytes(request)
            subprocess.run([str(probe), category, str(source), str(actual)], cwd=ROOT, check=True, timeout=60)
            result = actual.read_bytes()
            if result != expected:
                for record, value in zip(records, chunks(result)):
                    if bytes(record["expected"]) != value:
                        raise AssertionError("native terminal projection differs: " + record["name"])
                raise AssertionError("native terminal projection record count differs: " + category)
            counts[category] = len(records)
    report = {"passed": True, "reference": current["reference"], "comparison": "exact row/source identities, option boundaries, anchor flags and recolored packed styles", **counts}
    (TARGET / "terminal-projection-differential.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
