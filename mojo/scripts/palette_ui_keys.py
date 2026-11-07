import json
from pathlib import Path
import runpy
import subprocess
import tempfile

from path_list_vectors import uint, text

ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "mojo"
TARGET = ROOT / "target/port"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    source = subprocess.run(["cargo", "run", "--quiet", "--locked", "--offline", "-j", "1", "-p", "zz-ui", "--no-default-features", "--features", "path-rank-fixtures", "--example", "palette_files_vectors", "--", "--ui-keys"], cwd=ROOT, check=True, stdout=subprocess.PIPE, timeout=300)
    values = json.loads(source.stdout)
    payload = bytearray()
    expected = bytearray()
    uint(payload, len(values["cases"]))
    for spelling, result in values["cases"]:
        text(payload, spelling)
        expected.append(result is not None)
        if result is not None:
            text(expected, result[0])
            uint(expected, result[1])
            expected.append(result[2] is not None)
            if result[2] is not None:
                text(expected, result[2])
    binary = TARGET / "palette-ui-keys-probe"
    subprocess.run(["pixi", "run", "--manifest-path", str(PORT / "pixi.toml"), "mojo", "build", "-I", str(PORT), str(PORT / "tests/palette_ui_keys_probe.mojo"), "-o", str(binary)], cwd=ROOT, check=True)
    with tempfile.TemporaryDirectory(prefix="zz-palette-ui-keys-", dir="/tmp") as directory:
        path = Path(directory)
        (path / "input").write_bytes(payload)
        subprocess.run([str(binary), str(path / "input"), str(path / "output")], check=True, timeout=30)
        actual = (path / "output").read_bytes()
        if actual != expected:
            raise AssertionError("palette UI key parser differs from the native parser")
    report = {"passed": True, "cases": len(values["cases"]), "bytes": len(expected), "source": values["source"]}
    (TARGET / "palette-ui-keys.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
