import json
from pathlib import Path
import runpy
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/port"


def uint(output, value):
    while value >= 128:
        output.append((value & 127) | 128)
        value >>= 7
    output.append(value)


def text(output, value):
    encoded = value.encode()
    uint(output, len(encoded))
    output.extend(encoded)


def optional(output, value, write):
    output.append(value is not None)
    if value is not None:
        write(output, value)


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    reference = TARGET / "chooser-runtime-vectors.json"
    subprocess.run(["cargo", "run", "--quiet", "--locked", "--offline", "-p", "zz-client", "--example", "chooser_runtime_vectors", "--", str(reference)], cwd=ROOT, check=True)
    traces = json.loads(reference.read_text())
    output = bytearray()
    uint(output, len(traces))
    steps = 0
    for trace in traces:
        uint(output, len(trace))
        for step in trace:
            request = step["request"]
            kind = request[0]
            output.append(kind)
            if kind == 0:
                uint(output, request[1])
                uint(output, request[2])
                output.append(request[3])
            elif kind in (1, 2):
                text(output, request[1])
            uint(output, step["revision"])
            optional(output, step["selected"], uint)
            output.append(step["search"])
            optional(output, step["marked"], text)
            optional(output, step["utf16"], uint)
            uint(output, step["keys"])
            if kind == 0:
                output.extend(step["effect"])
            elif kind == 2:
                optional(output, step["effect"], text)
            elif kind == 3:
                output.append(step["effect"])
            steps += 1
    binary = TARGET / "chooser-runtime-probe"
    subprocess.run(["pixi", "run", "--manifest-path", str(ROOT / "mojo/pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(ROOT / "mojo"), str(ROOT / "mojo/tests/chooser_runtime_probe.mojo"), "-o", str(binary)], cwd=ROOT, check=True)
    with tempfile.TemporaryDirectory(prefix="zz-chooser-runtime-") as directory:
        path = Path(directory) / "vectors.bin"
        path.write_bytes(output)
        result = subprocess.run([str(binary), str(path)], cwd=ROOT, capture_output=True, check=True, text=True)
        assert list(map(int, result.stdout.split())) == [len(traces), steps], result.stdout
    report = {"passed": True, "reference": "actual GUI-delegated ChooserInput state", "traces": len(traces), "states": steps, "key_queries": steps * 32, "bytes": len(output), "scope": "source revision/selection/search/composition and full UTF16 state; explicit 64 KiB atomic refusals; scroll geometry checked separately"}
    (TARGET / "chooser-runtime-differential.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
