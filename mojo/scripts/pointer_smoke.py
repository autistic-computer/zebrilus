import json
import os
from pathlib import Path
import runpy
import shlex
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "mojo"
TARGET = ROOT / "target/port"
MJUI = ROOT.parent / "mjui"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    TARGET.mkdir(parents=True, exist_ok=True)
    binary = TARGET / "pointer-smoke"
    rust = ROOT / "target/debug/zz_cli"
    if not rust.exists():
        raise SystemExit("build the Rust reference first: cargo build -p zz-cli")
    subprocess.run(["pixi", "run", "--manifest-path", str(PORT / "pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(PORT), "-I", str(MJUI / "src"), str(PORT / "tests/pointer_smoke.mojo"), "-o", str(binary)], cwd=ROOT, check=True)
    environment = os.environ.copy()
    environment["ZZ_BEND"] = str(TARGET / "zz-bend")
    with tempfile.TemporaryDirectory(prefix="zzpointer-", dir="/tmp") as directory:
        temporary = Path(directory)
        socket = str(temporary / "s")
        capture = temporary / "received.bin"
        capture.write_bytes(b"")
        recorder = temporary / "recorder.py"
        recorder.write_text(
            "import os, sys, tty\n"
            "tty.setraw(0)\n"
            "os.write(1, b''.join(('ROW_%03d selection target\\r\\n' % i).encode() for i in range(150)) + b'POINTER_READY\\r\\n')\n"
            "tracking = False\n"
            "with open(sys.argv[1], 'ab', buffering=0) as output:\n"
            "    while True:\n"
            "        chunk = os.read(0, 4096)\n"
            "        if not chunk:\n"
            "            break\n"
            "        if not tracking and b'T' in chunk:\n"
            "            before, after = chunk.split(b'T', 1)\n"
            "            output.write(before + after)\n"
            "            os.write(1, b'\\x1b[?1003h\\x1b[?1006hTRACKING_READY\\r\\n')\n"
            "            tracking = True\n"
            "        else:\n"
            "            output.write(chunk)\n"
        )
        command = "python3 " + shlex.quote(str(recorder)) + " " + shlex.quote(str(capture))
        try:
            subprocess.run([str(rust), "-S", socket, "-f", "/dev/null", "new-session", "-d", "-s", "pointer-proof", command], cwd=ROOT, env=environment, check=True, capture_output=True, timeout=15)
            subprocess.run([str(binary), socket], cwd=ROOT, env=environment, check=True, timeout=30)
            actual = capture.read_bytes()
            expected = b"\x1b[<0;3;4M\x1b[<32;4;4M\x1b[<0;4;4m\x1b[<64;3;4M\x1b[<64;3;4M"
            if actual != expected:
                raise AssertionError(f"native PTY mouse bytes differ: {actual.hex()} != {expected.hex()}")
            report = {"passed": True, "source": "native terminal view inputs through actual daemon and PTY mouse encoder", "checks": ["wheel scrolls retained viewport without activation", "left selection and outside drag/release", "exact SGR press/motion/release/wheel bytes", "Shift+Alt selects rectangle without leaking application mouse bytes"], "pty_hex": actual.hex()}
            (TARGET / "pointer-smoke.json").write_text(json.dumps(report, indent=2) + "\n")
            print(json.dumps(report))
        finally:
            subprocess.run([str(rust), "-S", socket, "kill-server"], cwd=ROOT, env=environment, capture_output=True, timeout=10)


if __name__ == "__main__":
    main()
