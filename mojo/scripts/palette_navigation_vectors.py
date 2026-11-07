import itertools
import json
import os
from pathlib import Path
import runpy
import subprocess


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/port"
SETTINGS = ("profile.dev.debug=0", 'profile.dev.package."*".opt-level=0', "profile.dev.package.ala.opt-level=0", "profile.dev.package.ala-macros.opt-level=0", "profile.dev.package.ala-platform.opt-level=0", "profile.dev.package.taffy.opt-level=0")


def uint(output, value):
    while value >= 128:
        output.append((value & 127) | 128)
        value >>= 7
    output.append(value)


def optional(output, value):
    uint(output, int(value is not None))
    if value is not None:
        uint(output, value)


def vectors():
    row_sets = [[]]
    for kinds in itertools.product(range(4), repeat=3):
        row_sets.append([(kind, index % 2 == 1, index * 0x3F800000 if index < 2 else 0x40000000, index, index) for index, kind in enumerate(kinds)])
    seed = 0xCFA32841
    depths = [0, 0x3F800000, 0x40000000, 0x40400000, 0xBF800000, 0x7FC00000, 0x7F800000, 0xFF800000, 0x80000000]
    for length in range(1, 17):
        for _ in range(8):
            rows = []
            for index in range(length):
                seed = (seed * 1664525 + 1013904223) & 0xFFFFFFFF
                rows.append((seed % 4, bool(seed >> 7 & 1), depths[(seed >> 10) % len(depths)], (seed >> 20) % 3, index % 4))
            row_sets.append(rows)
    cases = []
    for rows in row_sets:
        for selected in dict.fromkeys((None, 0, 1, len(rows) - 1 if rows else 0, len(rows) + 1)):
            for direction in (-2, -1, 0, 1, 2):
                for current in (None, 0, 1):
                    cases.append({"rows": rows, "selected": selected, "direction": direction, "current": current})
    return cases


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    cases = vectors()
    layers = list(itertools.product(range(8), range(5)))
    environment = os.environ.copy()
    environment["CARGO_TARGET_DIR"] = str(ROOT / "target/penna-native")
    command = ["cargo", "build", "--locked", "--offline", "-p", "zz-ui", "--example", "palette_navigation_vectors"]
    for setting in SETTINGS:
        command.extend(["--config", setting])
    subprocess.run(command, cwd=ROOT, env=environment, check=True, timeout=600)
    result = subprocess.run([str(ROOT / "target/penna-native/debug/examples/palette_navigation_vectors")], input=json.dumps({"cases": cases, "layers": layers}).encode(), cwd=ROOT, capture_output=True, check=True, timeout=120)
    source = json.loads(result.stdout)
    requests = bytearray()
    wanted = bytearray()
    uint(requests, len(cases))
    for case, parsed in zip(cases, source["cases"], strict=True):
        uint(requests, len(case["rows"]))
        for row in case["rows"]:
            for value in row:
                uint(requests, int(value))
        optional(requests, case["current"])
        optional(requests, case["selected"])
        uint(requests, int(case["direction"] < 0))
        uint(requests, abs(case["direction"]))
        optional(wanted, parsed["initial"])
        optional(wanted, parsed["next"])
        for value in (*parsed["parents"], *parsed["children"]):
            optional(wanted, value)
    uint(requests, len(layers))
    for (bits, count), (results, remaining) in zip(layers, source["layers"], strict=True):
        uint(requests, bits)
        uint(requests, count)
        for result in results:
            uint(wanted, int(result))
        uint(wanted, remaining)
    uint(wanted, 1)
    request = TARGET / "palette-navigation-input.bin"
    output = TARGET / "palette-navigation-output.bin"
    binary = TARGET / "palette-navigation-probe"
    request.write_bytes(requests)
    subprocess.run(["pixi", "run", "--manifest-path", str(ROOT / "mojo/pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(ROOT / "mojo"), str(ROOT / "mojo/tests/palette_navigation_probe.mojo"), "-o", str(binary)], cwd=ROOT, check=True, timeout=300)
    subprocess.run([str(binary), str(request), str(output)], cwd=ROOT, check=True, timeout=60)
    actual = output.read_bytes()
    if actual != wanted:
        difference = next((index for index, pair in enumerate(zip(actual, wanted)) if pair[0] != pair[1]), min(len(actual), len(wanted)))
        raise AssertionError(f"palette navigation differs at byte {difference}: {len(actual)} != {len(wanted)}")
    report = {"passed": True, "navigation_cases": len(cases), "layer_cases": len(layers), "bounded_cases": 1, "bytes": len(wanted), "source": "actual UnifiedPalette navigation, initial_selection, parent_index, child_index and pop_layer", "scope": "headers, actionable rows, current-window priority, absent/out-of-range selection, signed directions, exact Float32 depth including NaN/infinities, expansion and layered back navigation"}
    (TARGET / "palette-navigation-differential.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
