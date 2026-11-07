import bisect
import json
from pathlib import Path
import runpy
import subprocess
import tempfile

from palette_model_vectors import cases_for, encode
from palette_places_vectors import fixtures


ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "mojo"
TARGET = ROOT / "target/port"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    subprocess.run(["cargo", "build", "--offline", "-j", "1", "-p", "zz-client", "--example", "palette_verb_vectors"], cwd=ROOT, check=True)
    subprocess.run(["cargo", "build", "--offline", "-j", "1", "-p", "zz-ui", "--example", "palette_places_vectors"], cwd=ROOT, check=True)
    fleets, _, scopes = fixtures()
    fleets[1][1][0][4][0][3][0][5][1][3] = "running"
    fleets[4][1][0][3] = None
    cases = cases_for(scopes)
    statuses = [(1, 1, 2), (2, 1, 1)]
    reference = json.loads(subprocess.check_output([str(ROOT / "target/debug/examples/palette_places_vectors")], input=json.dumps({"fleets": fleets, "cases": cases, "statuses": statuses}).encode()))
    binary = TARGET / "palette-component-model-probe"
    subprocess.run(["pixi", "run", "--manifest-path", str(PORT / "pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(PORT), "-I", str(PORT / "tests"), str(PORT / "tests/palette_model_probe.mojo"), "-o", str(binary)], cwd=ROOT, check=True)
    with tempfile.TemporaryDirectory(prefix="zz-palette-chrome-", dir="/tmp") as directory:
        source = Path(directory) / "in"
        target = Path(directory) / "out"
        chrome = Path(directory) / "chrome"
        source.write_bytes(encode(fleets, cases, statuses))
        subprocess.run([str(binary), str(source), str(target), str(chrome)], check=True)
        if target.read_bytes() != bytes(reference["bytes"]):
            raise AssertionError("existing palette model serialization changed")
        expected = bytes(reference["chrome"])
        actual = chrome.read_bytes()
        if actual != expected:
            offset = next((index for index in range(min(len(actual), len(expected))) if actual[index] != expected[index]), min(len(actual), len(expected)))
            index = bisect.bisect_right(reference["chrome_ends"], offset)
            raise AssertionError((offset, index, cases[index], actual[max(0, offset - 24):offset + 160], expected[max(0, offset - 24):offset + 160]))
    report = {"passed": True, "states": len(cases), "bytes": len(expected), "model_bytes": len(reference["bytes"]), "source": "source palette preview policy calling native verbs.compile/go_to_line/catalog_command_spec for every selected row and no selection", "scope": "draft GUI/path/command/alias previews, selected target fills, readable target quoting, leading colon command usages and missing previews; existing model serialization preserved"}
    (TARGET / "palette-component-model.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
