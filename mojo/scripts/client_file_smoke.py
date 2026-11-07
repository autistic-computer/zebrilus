import json
import os
from pathlib import Path
import runpy
import subprocess
import tempfile
import time


ROOT = Path(__file__).resolve().parents[2]
RUST = ROOT / "target/debug/zz_cli"
MOJO = ROOT / "target/port/zz-mojo"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    subprocess.run(["python3", str(ROOT / "mojo/scripts/build.py")], check=True, timeout=180)
    checked = 0
    with tempfile.TemporaryDirectory(prefix="zz-file-smoke-", dir="/tmp") as directory:
        directory = Path(directory)
        socket = directory / "s"

        def invoke(binary, *args, payload=None, closed=False):
            result = subprocess.run([str(binary), "-S" if binary == RUST else "--socket", str(socket), *args], input=payload, stdin=subprocess.DEVNULL if payload is None else None, stdout=subprocess.PIPE, stderr=subprocess.PIPE, cwd=directory, preexec_fn=(lambda: os.close(0)) if closed else None, timeout=30)
            return result.returncode, result.stdout, result.stderr

        def compare(*args, payload=None, expected=0, closed=False):
            nonlocal checked
            reference = invoke(RUST, *args, payload=payload, closed=closed)
            actual = invoke(MOJO, *args, payload=payload, closed=closed)
            assert actual == reference and actual[0] == expected, (args, reference, actual)
            checked += 1
            return actual

        try:
            started = invoke(RUST, "-f", "/dev/null", "new-session", "-d", "-s", "file-proof", "sleep 120")
            assert started[0] == 0, started
            source = directory / "source 世界.bin"
            raw = b"\0\xffbinary\x80\nlast"
            source.write_bytes(raw)
            compare("load-buffer", "-b", "from-file", source.name)
            assert compare("save-buffer", "-b", "from-file", "-")[1] == raw
            for binary in (RUST, MOJO):
                target = directory / binary.name
                assert invoke(binary, "save-buffer", "-b", "from-file", str(target))[0] == 0
                assert target.read_bytes() == raw
                assert invoke(binary, "save-buffer", "-a", "-b", "from-file", str(target))[0] == 0
                assert target.read_bytes() == raw * 2
                assert invoke(binary, "save-buffer", "-b", "from-file", str(target))[0] == 0
                assert target.read_bytes() == raw
            checked += 3
            compare("load-buffer", "-b", "stdin", "-", payload=raw)
            assert compare("save-buffer", "-b", "stdin", "-")[1] == raw
            compare("load-buffer", "-b", "stdin-max", "-", payload=b"x" * 1048576)
            assert compare("save-buffer", "-b", "stdin-max", "-")[1] == b"x" * 1048576
            compare("load-buffer", "-b", "stdin-large", "-", payload=b"x" * 1048577, expected=1)
            compare("load-buffer", "-b", "missing", "missing-file", expected=1)
            compare("load-buffer", "-b", "directory", str(directory), expected=1)
            compare("save-buffer", "-b", "stdin", str(directory), expected=1)
            compare("load-buffer", "-b", "closed", "-", closed=True, expected=1)
            compare("display-message", "-p", "closed ordinary command", closed=True)
            compare("send-text", "-t", "%0", payload=b"\xff", expected=1)
            compare("load-buffer", "-b", "missing-unicode", "missing 世界", expected=1)
            compare("display-message", "-p", b"raw-\xff-argument")
            config = b"set-option -g @stdin-proof 'hello world'\n"
            compare("source-file", "-", payload=config)
            assert compare("show-options", "-gqv", "@stdin-proof")[1] == b"hello world\n"
            payload = b"chunked payload line\r\n" * 100000 + b"chunked-stream-end\r\n"
            captures = []
            for binary in (RUST, MOJO):
                result = invoke(RUST, "new-window", "-d", "-P", "-F", "#{pane_id}", "sleep 120")
                assert result[0] == 0, result
                target = result[1].strip().decode()
                created = invoke(RUST, "split-window", "-I", "-d", "-P", "-F", "#{pane_id}", "-t", target)
                assert created[0] == 0, created
                pane = created[1].strip().decode()
                sent = invoke(binary, "display-message", "-I", "-t", pane, payload=payload)
                assert sent[0] == 0, sent
                deadline = time.monotonic() + 10
                while time.monotonic() < deadline:
                    capture = invoke(RUST, "capture-pane", "-p", "-S", "-30", "-t", pane)
                    if b"chunked-stream-end" in capture[1]:
                        break
                    time.sleep(0.01)
                else:
                    raise AssertionError((binary, capture))
                captures.append(capture)
            assert captures[0] == captures[1], tuple(value[1] for value in captures)
            checked += 1
            report = {"passed": True, "groups": checked, "reference": "actual Rust CLI against a private native daemon", "scope": "client file read/write/append, raw stdout, stdin availability and limits, file and UTF-8 errors, closed stdin, source-file, and 2MiB chunked PTY input"}
            (ROOT / "target/port/client-file-smoke.json").write_text(json.dumps(report, indent=2) + "\n")
            print(json.dumps(report))
        finally:
            subprocess.run([str(RUST), "-S", str(socket), "kill-server"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10)


if __name__ == "__main__":
    main()
