import hashlib
import json
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile


ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "mojo"
TARGET = ROOT / "target/port"


def uint(output, value):
    value = int(value)
    while value >= 128:
        output.append((value & 127) | 128)
        value >>= 7
    output.append(value)


def argument(output, value):
    if isinstance(value, str):
        encoded = value.encode()
        uint(output, len(encoded))
        output.extend(encoded)
    elif isinstance(value, list):
        uint(output, len(value))
        output.extend(value)
    else:
        uint(output, value)


def source_method(source, name):
    marker = "fn " + name + "("
    start = source.index(marker)
    start = source.rfind("\n", 0, start) + 1
    body = source.index("{", start)
    depth = 1
    end = body + 1
    while depth:
        if source[end] == "{":
            depth += 1
        elif source[end] == "}":
            depth -= 1
        end += 1
    return source[start:end]


def policy_vectors(temporary):
    source = (ROOT / "crates/zz/src/mux/client.rs").read_text()
    methods = "\n".join(source_method(source, name) for name in ("set_clipboard_writes", "write_clipboard"))
    program = (PORT / "scripts/clipboard_policy.rs").read_text().replace('    include!("clipboard_policy_source.rs");', methods)
    (temporary / "clipboard_policy.rs").write_text(program)
    binary = temporary / "clipboard-policy"
    subprocess.run(["rustc", "--edition=2024", "-D", "warnings", str(temporary / "clipboard_policy.rs"), "-o", str(binary)], check=True)
    result = subprocess.run([str(binary)], check=True, capture_output=True, text=True)
    options = [None, "external", "on", "off", "", "other"]
    records = []
    for line in result.stdout.splitlines():
        option, producer, target, empty, retained, initial, clipboard, primary, generation = map(int, line.split())
        records.append({"args": [target, producer, empty, options[option], sys.platform.startswith(("linux", "freebsd")), retained], "expected": [int(clipboard > 0), int(primary > 0), int(generation != initial)]})
    return {"source": "verbatim current MuxClient::set_clipboard_writes/write_clipboard with isolated host and retained-viewport adapters", "source_sha256": hashlib.sha256(methods.encode()).hexdigest(), "cases": records}


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    TARGET.mkdir(parents=True, exist_ok=True)
    vectors = []
    for package, example in [("zz-protocol", "clipboard_vectors"), ("zz-client", "clipboard_keys_vectors")]:
        result = subprocess.run(["cargo", "run", "--quiet", "--locked", "-p", package, "--example", example], cwd=ROOT, capture_output=True)
        if result.returncode:
            raise SystemExit(result.stderr.decode())
        vectors.append(json.loads(result.stdout))
    (TARGET / "clipboard-wire-vectors.json").write_text(json.dumps(vectors[0], indent=2) + "\n")
    (TARGET / "clipboard-key-vectors.json").write_text(json.dumps(vectors[1], indent=2) + "\n")
    binary = TARGET / "clipboard-probe"
    subprocess.run(["pixi", "run", "--manifest-path", str(PORT / "pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(PORT), "-I", str(ROOT.parent / "mjui/src"), str(PORT / "tests/clipboard_probe.mojo"), "-o", str(binary)], cwd=ROOT, check=True)
    with tempfile.TemporaryDirectory(prefix="zzclipboard-vectors-") as directory:
        temporary = Path(directory)
        policies = policy_vectors(temporary)
        source = temporary / "source.bin"
        actual = temporary / "actual.bin"
        accepted = 0
        rejected = 0
        encoders = 0
        for case in vectors[0]["cases"]:
            if "args" in case:
                request = bytearray()
                for value in case["args"]:
                    argument(request, value)
                source.write_bytes(request)
                subprocess.run([str(binary), "build", str(source), str(actual)], check=True)
                if actual.read_bytes() != bytes(case["bytes"]):
                    raise AssertionError("clipboard encoder differs: " + case["name"])
                encoders += 1
            source.write_bytes(bytes(case["bytes"]))
            result = subprocess.run([str(binary), "roundtrip", str(source), str(actual)], capture_output=True)
            if (result.returncode == 0) != case["accepted"]:
                raise AssertionError("clipboard acceptance differs: " + case["name"] + result.stderr.decode())
            if case["accepted"]:
                accepted += 1
                if actual.read_bytes() != bytes(case["canonical"]):
                    raise AssertionError("clipboard roundtrip differs: " + case["name"])
            else:
                rejected += 1
        for mode, reference in [("keys", vectors[1]), ("policy", policies)]:
            request = bytearray()
            uint(request, len(reference["cases"]))
            expected = bytearray()
            for case in reference["cases"]:
                for index, value in enumerate(case["args"]):
                    if mode == "policy" and index == 3:
                        uint(request, value is not None)
                        if value is not None:
                            argument(request, value)
                    else:
                        argument(request, value)
                if isinstance(case["expected"], list):
                    expected.extend(case["expected"])
                else:
                    expected.append(case["expected"])
            source.write_bytes(request)
            subprocess.run([str(binary), mode, str(source), str(actual)], check=True)
            if actual.read_bytes() != expected:
                different = next(index for index, values in enumerate(zip(actual.read_bytes(), expected)) if values[0] != values[1])
                raise AssertionError("clipboard " + mode + " differs at byte " + str(different))
        for name, data in [("clipboard-wire-vectors", vectors[0]), ("clipboard-key-vectors", vectors[1]), ("clipboard-policy-vectors", policies)]:
            (TARGET / (name + ".json")).write_text(json.dumps(data, indent=2) + "\n")
    report = {"passed": True, "encoders": encoders, "accepted": accepted, "rejected": rejected, "shortcuts": len(vectors[1]["cases"]), "host_policies": len(policies["cases"]), "policy_source_sha256": policies["source_sha256"]}
    (TARGET / "clipboard-differential.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
