import json
from pathlib import Path
import runpy
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/port"


def integer(value):
    output = bytearray()
    while value >= 128:
        output.append((value & 127) | 128)
        value >>= 7
    output.append(value)
    return output


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    reference = subprocess.run(
        ["cargo", "run", "--locked", "-p", "zz-protocol", "--example", "client_context_vectors"],
        cwd=ROOT, capture_output=True, check=True, timeout=180,
    )
    cases = json.loads(reference.stdout)
    probe = TARGET / "client-context-probe"
    subprocess.run([
        "mojo", "build", "--fp-mode=contract=off", "-I", str(ROOT / "mojo"),
        str(ROOT / "mojo/tests/client_context_probe.mojo"), "-o", str(probe),
    ], check=True, timeout=180)
    with tempfile.TemporaryDirectory(prefix="zz-context-") as directory:
        source = Path(directory) / "context.bin"
        result = Path(directory) / "result.bin"
        for index, case in enumerate(cases):
            packet = bytearray(8)
            packet.append(int(case["cwd"] is not None))
            if case["cwd"] is not None:
                packet.extend(integer(len(case["cwd"])))
                packet.extend(case["cwd"])
            packet.extend(integer(len(case["environment"])))
            for entry in case["environment"]:
                packet.extend(integer(len(entry)))
                packet.extend(entry)
            source.write_bytes(packet)
            for mode in ("command", "interactive"):
                rejected = case[mode + "_rejected"]
                subprocess.run([
                    str(probe), mode, str(source), str(result), "reject" if rejected else "accept",
                ], check=True, timeout=15)
                if not rejected and result.read_bytes() != bytes(case[mode]):
                    raise AssertionError((index, mode, "context hello differs from Rust"))
    report = {"passed": True, "cases": len(cases) * 2, "reference": "actual Rust ClientHello and Hello::from_client", "scope": "raw cwd/environment encoding, empty and maximum paths, duplicate entries and invalid entry bounds"}
    (TARGET / "client-context-differential.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
