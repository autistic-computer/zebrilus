import json
import os
from pathlib import Path
import runpy
import subprocess


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/port"
BASE = runpy.run_path(str(Path(__file__).with_name("window_chooser_vectors.py")))


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    TARGET.mkdir(parents=True, exist_ok=True)
    environment = os.environ.copy()
    environment["CARGO_TARGET_DIR"] = str(ROOT / "target/penna-native")
    command = ["cargo", "build", "--locked", "--offline", "-p", "zz-ui", "--features", "scene-fixtures", "--example", "window_chooser_lifecycle_vectors"]
    for setting in BASE["SETTINGS"]:
        command.extend(["--config", setting])
    subprocess.run(command, cwd=ROOT, env=environment, check=True, timeout=600)
    texts = ["", "a\r\nb\tc\x00d\x7fe\u0085f\u009f", "λ🦀\u2028\u2029\u200b\u00ad", "\r\r\n\n", "x" * 65536]
    scalars = [chr(value) for value in range(0x110000) if not 0xD800 <= value < 0xE000]
    texts.extend("".join(scalars[start:start + 4096]) for start in range(0, len(scalars), 4096))
    corpus = {"datasets": [[BASE["item"]("dev")]], "traces": [{"initial": 0, "grouped": True, "operations": [[13, value]]} for value in texts]}
    source = subprocess.run([str(ROOT / "target/penna-native/debug/examples/window_chooser_lifecycle_vectors")], input=json.dumps(corpus).encode(), cwd=ROOT, capture_output=True, check=True, timeout=120)
    reference = json.loads(source.stdout)
    output = bytearray()
    BASE["uint"](output, len(texts))
    for value, trace in zip(texts, reference["traces"], strict=True):
        BASE["text"](output, value)
        BASE["text"](output, trace[-1]["input"])
    request = TARGET / "window-chooser-text-input.bin"
    request.write_bytes(output)
    binary = TARGET / "window-chooser-text-probe"
    subprocess.run(["pixi", "run", "--manifest-path", str(ROOT / "mojo/pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(ROOT / "mojo"), str(ROOT / "mojo/tests/window_chooser_text_probe.mojo"), "-o", str(binary)], cwd=ROOT, check=True, timeout=300)
    subprocess.run([str(binary), str(request)], cwd=ROOT, check=True, timeout=60)
    report = {"passed": True, "reference": "actual CommandPaletteView InputState.replace", "cases": len(texts), "unicode_scalars": len(scalars), "bytes": len(output)}
    (TARGET / "window-chooser-text-differential.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
