import base64
import argparse
import json
import os
from pathlib import Path
import runpy
import shlex
import struct
import subprocess
import tempfile
import zlib


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/port"


def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    parser = argparse.ArgumentParser()
    parser.add_argument("--skip-build", action="store_true")
    options = parser.parse_args()
    if not options.skip_build:
        subprocess.run(["bash", "mojo/scripts/ui.sh", "build-clipboard-smoke"], cwd=ROOT, check=True)
    environment = os.environ.copy()
    environment["SDL_VIDEODRIVER"] = "dummy"
    environment["SDL_AUDIODRIVER"] = "dummy"
    environment["ZZ_BEND"] = str(TARGET / "zz-bend")
    rust = ROOT / "target/debug/zz_cli"
    with tempfile.TemporaryDirectory(prefix="zzclip-", dir="/tmp") as directory:
        temporary = Path(directory)
        socket = str(temporary / "s")
        captured = temporary / "input.bin"
        captured.write_bytes(b"")
        image = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0)) + chunk(b"tEXt", b"clipboard\0" + b"x" * (1024 * 1024 + 17)) + chunk(b"IDAT", zlib.compress(b"\0\x12\x34\x56")) + chunk(b"IEND", b"")
        image_path = temporary / "image.png"
        image_path.write_bytes(image)
        recorder = temporary / "recorder.py"
        primary = base64.b64encode(b"APP_PRIMARY_MARKER")
        recorder.write_text(
            "import os, sys, tty\n"
            "tty.setraw(0)\n"
            "os.write(1, b'\\x1b[?2004hCLIPBOARD_MARKER_LINE\\r\\n')\n"
            "seen = bytearray()\n"
            "emitted = False\n"
            "with open(sys.argv[1], 'ab', buffering=0) as output:\n"
            "    while True:\n"
            "        data = os.read(0, 4096)\n"
            "        if not data: break\n"
            "        output.write(data)\n"
            "        seen.extend(data)\n"
            "        if not emitted and b'OSC52_TRIGGER' in seen:\n"
            "            emitted = True\n"
            "            os.write(1, b'\\x1b]52;p;" + primary.decode() + "\\x07')\n"
        )
        command = "python3 " + shlex.quote(str(recorder)) + " " + shlex.quote(str(captured))
        try:
            subprocess.run([str(rust), "-S", socket, "-f", "/dev/null", "new-session", "-d", "-s", "clipboard-proof", command], env=environment, check=True, capture_output=True, timeout=15)
            subprocess.run([str(TARGET / "clipboard-workspace-smoke"), socket, str(image_path), str(TARGET / "clipboard-workspace.png")], cwd=temporary, env=environment, check=True, timeout=90)
            actual = captured.read_bytes()
            expected = b"\x1b[200~PASTE_TWO\x1b[201~" + "é".encode() + b"COMPOSED\x1b[200~MULTI\x1b[201~\x1b[200~MENU_PASTE\x1b[201~IME_MENUa\x1b[200~ORDER_MENU\x1b[201~bc\x1b[200~ORDER_KEY\x1b[201~de\rfg\x1b[200~ORDER_SUFFIX\x1b[201~hOSC52_TRIGGER"
            if not actual.startswith(expected):
                raise AssertionError("ordinary paste/IME/key bytes differ: " + repr(actual))
            upload = actual[len(expected):]
            if not upload.startswith(b"\x1b[200~") or not upload.endswith(b"\x1b[201~"):
                raise AssertionError("image path was not pasted through native terminal paste")
            path = Path(upload[6:-6].decode())
            if not path.is_relative_to(temporary / "paste") or path.read_bytes() != image:
                raise AssertionError("paced image bytes differ or escaped the private paste directory")
            report = {"passed": True, "host": "isolated native SDL clipboard plus actual workspace/private daemon/PTY", "checks": ["physical copy/paste/select-all chords", "associated text consumed once", "interleaved menu paste order", "interleaved physical paste order", "raw key and text order", "consumed editor prefix preserves suffix order", "single-scalar and composed Text", "uncomposed native bracketed Paste", "server copy reaches clipboard", "clipboard query roundtrip", "stale query reply rejected", "application PRIMARY remains distinct", "image beats text offer", "multi-chunk PNG upload", "exact private PTY bytes"], "image_bytes": len(image), "pty_prefix_hex": expected.hex()}
            (TARGET / "clipboard-workspace-smoke.json").write_text(json.dumps(report, indent=2) + "\n")
            print(json.dumps(report))
        except Exception:
            print("private PTY input hex:", captured.read_bytes().hex(), flush=True)
            result = subprocess.run([str(rust), "-S", socket, "list-clients", "-F", "#{client_id}:#{client_key_table}:#{client_prefix}"], env=environment, capture_output=True, timeout=10)
            print("private client routing:", result.stdout.decode(), flush=True)
            raise
        finally:
            subprocess.run([str(rust), "-S", socket, "kill-server"], env=environment, capture_output=True, timeout=10)


if __name__ == "__main__":
    main()
