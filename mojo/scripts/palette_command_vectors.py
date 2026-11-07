import argparse
import json
import os
from pathlib import Path
import runpy
import subprocess


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/port"
DATA = ROOT / "mojo/zzmojo/palette_command_data.mojo"
SETTINGS = ("profile.dev.debug=0", 'profile.dev.package."*".opt-level=0', "profile.dev.package.ala.opt-level=0", "profile.dev.package.ala-macros.opt-level=0", "profile.dev.package.ala-platform.opt-level=0", "profile.dev.package.taffy.opt-level=0")


def uint(output, value):
    while value >= 128:
        output.append((value & 127) | 128)
        value >>= 7
    output.append(value)


def text(output, value):
    data = value.encode()
    uint(output, len(data))
    output.extend(data)


def catalog_source(catalog):
    data = bytearray()
    uint(data, len(catalog))
    for command in catalog:
        for value in (command["name"], " ".join(command["aliases"]), command["description"], command["usage"]):
            text(data, value)
        uint(data, 0 if command["target"] is None else command["target"] + 1)
        uint(data, int(command["needs_arguments"]))
        uint(data, sum(int(value) << index for index, value in enumerate(command["allowed"])))
    if len(data) > 128 * 1024 or len(catalog) > 512:
        raise AssertionError("palette command catalog exceeds its limit")
    return f'''comptime COMMANDS = "{data.hex()}"


@inline(.never)
def command_bytes() -> List[UInt8]:
    var data = StringSlice(COMMANDS).as_bytes()
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
    TARGET.mkdir(parents=True, exist_ok=True)
    environment = os.environ.copy()
    environment["CARGO_TARGET_DIR"] = str(ROOT / "target/penna-native")
    command = ["cargo", "run", "--locked", "--offline", "-p", "zz-ui", "--example", "palette_command_vectors"]
    for setting in SETTINGS:
        command.extend(["--config", setting])
    reference = TARGET / "palette-command-source.json"
    with reference.open("w") as destination:
        subprocess.run(command, cwd=ROOT, env=environment, stdout=destination, check=True, timeout=600)
    source = json.loads(reference.read_text())
    generated = catalog_source(source["catalog"])
    if options.generate:
        DATA.write_text(generated)
        print(json.dumps({"generated": str(DATA), "commands": len(source["catalog"])}))
        return
    if DATA.read_text() != generated:
        raise AssertionError("palette command data is stale; run --generate")
    requests = bytearray()
    wanted = bytearray()
    uint(requests, len(source["cases"]))
    rows = 0
    for case in source["cases"]:
        text(requests, case["query"])
        uint(requests, int(case["browser"]))
        uint(requests, int(case["editor"]))
        uint(wanted, len(case["rows"]))
        rows += len(case["rows"])
        for row in case["rows"]:
            if row["index"] is None:
                raise AssertionError("native command row has no catalog entry")
            uint(wanted, row["index"])
            text(wanted, row["label"])
            text(wanted, row["description"])
            uint(wanted, len(row["hits"]))
            for start, end in row["hits"]:
                uint(wanted, start)
                uint(wanted, end)
    request = TARGET / "palette-command-input.bin"
    output = TARGET / "palette-command-output.bin"
    binary = TARGET / "palette-commands-probe"
    request.write_bytes(requests)
    subprocess.run(["pixi", "run", "--manifest-path", str(ROOT / "mojo/pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(ROOT / "mojo"), str(ROOT / "mojo/tests/palette_commands_probe.mojo"), "-o", str(binary)], cwd=ROOT, check=True, timeout=300)
    subprocess.run([str(binary), str(request), str(output)], cwd=ROOT, check=True, timeout=60)
    actual = output.read_bytes()
    if actual != wanted:
        difference = next((index for index, pair in enumerate(zip(actual, wanted)) if pair[0] != pair[1]), min(len(actual), len(wanted)))
        raise AssertionError(f"palette command output differs at byte {difference}: {len(actual)} != {len(wanted)}")
    report = {"passed": True, "commands": len(source["catalog"]), "cases": len(source["cases"]), "rows": rows, "bytes": len(wanted), "reference": "actual Rust command_entries and PaneKindAvailability with source command metadata", "scope": "native stable command ranking, alias fallback, exact UTF-8 highlights, labels/descriptions, internal/browser filtering and all four browser/editor availability combinations"}
    (TARGET / "palette-command-differential.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
