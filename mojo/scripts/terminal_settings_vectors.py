import json
import os
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


def text(value):
    encoded = value.encode()
    return integer(len(encoded)) + encoded


def strings(values):
    return integer(len(values)) + b"".join(text(value) for value in values)


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    TARGET.mkdir(parents=True, exist_ok=True)
    environment = os.environ.copy()
    environment["CARGO_TARGET_DIR"] = str(ROOT / "target/penna-native")
    command = ["cargo", "run", "--locked", "-p", "zz-ui", "--example", "terminal_font_vectors"]
    for setting in ("profile.dev.debug=0", 'profile.dev.package."*".opt-level=0', "profile.dev.package.ala.opt-level=0", "profile.dev.package.ala-macros.opt-level=0", "profile.dev.package.ala-platform.opt-level=0", "profile.dev.package.taffy.opt-level=0"):
        command.extend(["--config", setting])
    reference = subprocess.run(command, cwd=ROOT, env=environment, stdout=subprocess.PIPE, check=True, timeout=1800)
    cases = json.loads(reference.stdout)
    probe = TARGET / "terminal-settings-probe"
    subprocess.run(["mojo", "build", "--fp-mode=contract=off", "-I", str(ROOT / "mojo"), str(ROOT / "mojo/tests/terminal_settings_probe.mojo"), "-o", str(probe)], check=True, timeout=180)
    with tempfile.TemporaryDirectory(prefix="zz-font-settings-") as directory:
        source = Path(directory) / "context.bin"
        result = Path(directory) / "request.bin"
        for case in cases:
            context = case["context"]
            packet = bytearray(8)
            for families in context["families"]:
                packet.extend(strings(families))
            packet.extend(integer(context["weight"]))
            packet.extend(integer(len(context["features"])))
            for feature in context["features"]:
                packet.extend(feature["tag"])
                packet.extend(integer(feature["value"]))
            source.write_bytes(packet)
            expected = case["expected"]
            output = text(expected["family"]) + strings(expected["fallbacks"]) + integer(len(expected["features"]))
            for tag, value in expected["features"]:
                output += text(tag) + integer(value)
            output += integer(int(expected["weight"])) + bytes([expected["italic"]])
            subprocess.run([str(probe), str(source), str(result), str(int(case["bold"])), str(int(case["italic"]))], check=True, timeout=15)
            assert result.read_bytes() == output, case
    report = {"passed": True, "cases": len(cases), "reference": "actual Rust terminal_font_for_style", "scope": "four styles, configured and empty stacks, ordered fallback/features, duplicate features and weight clamps"}
    (TARGET / "terminal-settings-differential.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
