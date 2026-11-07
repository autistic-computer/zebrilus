import json
import os
from pathlib import Path
import runpy
import subprocess
import tempfile

from path_list_vectors import uint


ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "mojo"
TARGET = ROOT / "target/port"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    environment = os.environ.copy()
    environment["CARGO_TARGET_DIR"] = str(ROOT / "target/penna-native")
    command = ["cargo", "run", "--quiet", "--locked", "--offline", "-p", "zz-ui", "--example", "path_picker_vectors"]
    for setting in ("profile.dev.debug=0", 'profile.dev.package."*".opt-level=0', "profile.dev.package.ala.opt-level=0", "profile.dev.package.ala-macros.opt-level=0", "profile.dev.package.ala-platform.opt-level=0", "profile.dev.package.taffy.opt-level=0"):
        command.extend(["--config", setting])
    result = subprocess.run(command, cwd=ROOT, env=environment, check=True, stdout=subprocess.PIPE, timeout=1200)
    vectors = json.loads(result.stdout)
    (TARGET / "path-text-vectors.json").write_text(json.dumps(vectors, indent=2) + "\n")
    probe = TARGET / "path-text-probe"
    subprocess.run(["pixi", "run", "--manifest-path", str(PORT / "pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(PORT), str(PORT / "tests/path_text_probe.mojo"), "-o", str(probe)], cwd=ROOT, check=True)
    request = bytearray()
    expected = bytearray()
    uint(request, len(vectors["cases"]))
    for case in vectors["cases"]:
        uint(request, len(case["request"]))
        request.extend(case["request"])
        expected.extend(case["expected"])
    with tempfile.TemporaryDirectory(prefix="zzpath-text-") as directory:
        source = Path(directory) / "source.bin"
        actual = Path(directory) / "actual.bin"
        source.write_bytes(request)
        subprocess.run([str(probe), str(source), str(actual)], check=True, timeout=120)
        output = actual.read_bytes()
        if output != expected:
            offset = 0
            for case in vectors["cases"]:
                end = offset + len(case["expected"])
                if output[offset:end] != bytes(case["expected"]):
                    raise AssertionError("path text differs: " + json.dumps(case) + " actual=" + repr(output[offset:end]))
                offset = end
            raise AssertionError("path text output length differs")
    print(json.dumps({"passed": True, "cases": len(vectors["cases"]), "bytes": len(expected), "reference": "actual native file path helpers, FilesAction scope and client insert_text"}))


if __name__ == "__main__":
    main()
