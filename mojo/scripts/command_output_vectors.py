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


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    TARGET.mkdir(parents=True, exist_ok=True)
    result = subprocess.run(["cargo", "run", "--quiet", "--locked", "-p", "zz-protocol", "--example", "command_output_vectors"], cwd=ROOT, capture_output=True)
    if result.returncode:
        raise SystemExit(result.stderr.decode())
    reference = json.loads(result.stdout)
    result = subprocess.run(["cargo", "run", "--quiet", "--locked", "-p", "zz-client", "--example", "terminal_mode_vectors"], cwd=ROOT, capture_output=True)
    if result.returncode:
        raise SystemExit(result.stderr.decode())
    modes = json.loads(result.stdout)
    (TARGET / "terminal-mode-vectors.json").write_text(json.dumps(modes, indent=2) + "\n")
    (TARGET / "command-output-vectors.json").write_text(json.dumps(reference, indent=2) + "\n")
    binary = TARGET / "command-output-probe"
    subprocess.run(["pixi", "run", "--manifest-path", str(PORT / "pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(PORT), "-I", str(ROOT.parent / "mjui/src"), str(PORT / "tests/command_output_probe.mojo"), "-o", str(binary)], cwd=ROOT, check=True)
    with tempfile.TemporaryDirectory(prefix="zzoutput-vectors-") as directory:
        source = Path(directory) / "source.bin"
        actual = Path(directory) / "actual.bin"
        for case in reference["cases"]:
            request = bytearray()
            for arg in case["args"]:
                if isinstance(arg, str):
                    encoded = arg.encode()
                    uint(request, len(encoded))
                    request.extend(encoded)
                else:
                    uint(request, int(arg))
            source.write_bytes(request)
            subprocess.run([str(binary), "build", str(source), str(actual)], check=True, timeout=10)
            if actual.read_bytes() != bytes(case["bytes"]):
                raise AssertionError("command-output wire differs: " + case["name"])
            source.write_bytes(bytes(case["bytes"]))
            subprocess.run([str(binary), "roundtrip", str(source), str(actual)], check=True, timeout=10)
            if actual.read_bytes() != bytes(case["bytes"]):
                raise AssertionError("command-output roundtrip differs: " + case["name"])
        for case in modes["cases"]:
            request = bytearray()
            for arg in case["args"]:
                uint(request, int(arg))
            expected = bytearray((int(case["expected"] is not None),))
            if case["expected"] is not None:
                label, detail = case["expected"]
                expected.append(int(label is not None))
                if label is not None:
                    encoded = label.encode()
                    uint(expected, len(encoded))
                    expected.extend(encoded)
                encoded = detail.encode()
                uint(expected, len(encoded))
                expected.extend(encoded)
            source.write_bytes(request)
            subprocess.run([str(binary), "mode", str(source), str(actual)], check=True, timeout=10)
            if actual.read_bytes() != expected:
                raise AssertionError("native mode indicator text differs: " + repr(case))
    report = {"passed": True, "source": reference["source"], "mode_source": modes["source"], "encoders": len(reference["cases"]), "roundtrips": len(reference["cases"]), "mode_indicators": len(modes["cases"])}
    (TARGET / "command-output-differential.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
