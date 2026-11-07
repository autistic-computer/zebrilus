import json
import os
from pathlib import Path
import runpy
import signal
import subprocess
import tempfile
import time


ROOT = Path(__file__).resolve().parents[2]
RUST = ROOT / "target/debug/zz_cli"
MOJO = ROOT / "target/port/zz-mojo-exec"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    subprocess.run(["mojo", "build", "--fp-mode=contract=off", "-I", str(ROOT / "mojo"), str(ROOT / "mojo/main.mojo"), "-o", str(MOJO)], check=True, timeout=180)
    checked = 0
    with tempfile.TemporaryDirectory(prefix="zz-exec-smoke-", dir="/tmp") as directory:
        directory = Path(directory)
        socket = directory / "s"

        def command(binary, *args):
            return [str(binary), "-S" if binary == RUST else "--socket", str(socket), *args]

        def invoke(binary, args, payload=None, environment=None, cwd=None):
            completed = subprocess.run(command(binary, *args), input=payload, stdin=subprocess.DEVNULL if payload is None else None, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=environment, cwd=cwd or directory, timeout=30)
            return completed.returncode, completed.stdout, completed.stderr

        def compare(args, payload=None):
            nonlocal checked
            reference = invoke(RUST, args, payload)
            actual = invoke(MOJO, args, payload)
            assert actual == reference, (args, reference, actual)
            checked += 1
            return actual

        try:
            assert invoke(RUST, ["-f", "/dev/null", "new-session", "-d", "-s", "exec-proof", "sleep 120"])[0] == 0
            assert invoke(RUST, ["set-buffer", "-b", "raw", "raw\x01buffer"])[0] == 0
            for args in [
                ["display-message", "-p", "first", ";", "display-message", "-p", "second"],
                ["display-message", "-p", "first;", "display-message", "-p", "second"],
                ["display-message", "-p", "literal\\;", ";", "display-message", "-p", "second"],
                ["display-message", "-p", "first", ";", "select-pane", "-t", "%999999", ";", "display-message", "-p", "after-error"],
                ["if-shell", "-F", "1", "display-message -p nested"],
                ["run-shell", "printf 'released-output'; printf 'warning' >&2"],
                ["save-buffer", "-b", "raw", "-", ";", "display-message", "-p", "after-raw"],
                ["display-message", "-p", "header", ";", "save-buffer", "-b", "raw", "-"],
                ["save-buffer", "-b", "raw", "-", ";", "save-buffer", "-b", "raw", "-"],
                ["definitely-unknown-command"],
                ["new-pane", "--definitely-invalid"],
            ]:
                compare(args)
            compare(["source-file", "-"], b"display-message -p first\ndisplay-message -p second\n")
            compare(["source-file", "-"], b"display-message -p first\nunknown-command\ndisplay-message -p skipped\n")
            compare(["load-buffer", "-b", "stdin-one", "-", ";", "load-buffer", "-b", "stdin-two", "-"], b"shared stdin\0\xff")
            compare(["save-buffer", "-b", "stdin-one", "-"])
            captures = []
            payload = b"direct split stream\r\n" * 100000 + b"DIRECT-SPLIT-END\r\n"
            for binary in (RUST, MOJO):
                created = invoke(RUST, ["new-window", "-d", "-P", "-F", "#{pane_id}", "sleep 120"])
                assert created[0] == 0, created
                target = created[1].strip().decode()
                sent = invoke(binary, ["split-window", "-I", "-d", "-P", "-F", "#{pane_id}", "-t", target], payload)
                assert sent[0] == 0, sent
                pane = sent[1].strip().decode()
                deadline = time.monotonic() + 10
                while time.monotonic() < deadline:
                    captured = invoke(RUST, ["capture-pane", "-p", "-S", "-30", "-t", pane])
                    if b"DIRECT-SPLIT-END" in captured[1]:
                        captures.append(captured)
                        break
                    time.sleep(0.01)
                else:
                    raise AssertionError((binary, captured))
            assert captures[0] == captures[1], captures
            checked += 1
            physical = directory / "physical"
            logical = directory / "logical"
            physical.mkdir()
            logical.symlink_to(physical, target_is_directory=True)
            environment = os.environ.copy()
            environment["PWD"] = str(logical)
            environment["SSH_AUTH_SOCK"] = str(directory / "agent.sock")
            for binary, name in ((RUST, "rust-context"), (MOJO, "mojo-context")):
                created = invoke(binary, ["new-session", "-d", "-s", name, "sleep 120"], environment=environment, cwd=physical)
                assert created[0] == 0, created
                assert invoke(RUST, ["display-message", "-p", "-t", name, "#{session_path}"])[1] == os.fsencode(logical) + b"\n"
                assert invoke(RUST, ["show-environment", "-t", name, "SSH_AUTH_SOCK"])[1] == b"SSH_AUTH_SOCK=" + os.fsencode(directory / "agent.sock") + b"\n"
            checked += 2
            for args, closed in [
                (["display-message", "-p", "sealed stdout"], (1,)),
                (["save-buffer", "-b", "raw", "-"], (1,)),
                (["select-pane", "-t", "%999999"], (2,)),
                (["display-message", "-p", "sealed standard streams"], (0, 1, 2)),
                (["load-buffer", "-b", "closed-input", "-"], (0, 1, 2)),
                (["display-message", "-p", "header", ";", "save-buffer", "-b", "raw", "-"], (1, 2)),
            ]:
                results = []
                for binary in (RUST, MOJO):
                    def close_standard_streams():
                        for fd in closed:
                            os.close(fd)
                    result = subprocess.run(command(binary, *args), stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE, preexec_fn=close_standard_streams, timeout=20)
                    results.append((result.returncode, result.stdout, result.stderr))
                assert results[0] == results[1], (args, closed, results)
                checked += 1
            for binary in (RUST, MOJO):
                process = subprocess.Popen(command(binary, "wait-for", "blocked-exec"), stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
                try:
                    deadline = time.monotonic() + 10
                    while time.monotonic() < deadline:
                        if process.poll() is not None:
                            raise AssertionError(process.communicate())
                        channel = Path(f"/proc/{process.pid}/wchan").read_text().strip()
                        if channel.startswith("unix_stream_"):
                            break
                        time.sleep(0.01)
                    else:
                        raise AssertionError((binary, "exec reader did not block", channel))
                    process.send_signal(signal.SIGTERM)
                    assert process.wait(timeout=5) == 0
                finally:
                    if process.poll() is None:
                        process.kill()
                    process.communicate()
            checked += 1
            for binary in (RUST, MOJO):
                killed = invoke(binary, ["kill-server"])
                assert killed == (0, b"", b""), (binary, killed)
                if binary == RUST:
                    restarted = invoke(RUST, ["-f", "/dev/null", "new-session", "-d", "-s", "restart-proof", "sleep 120"])
                    assert restarted[0] == 0, restarted
            checked += 1
            report = {"passed": True, "groups": checked, "reference": "actual Rust CLI against a private native daemon", "scope": "command chains/separators, streamed output/errors, raw stdout ownership, stdin reuse, direct split streaming, native errors, logical cwd/environment, and blocked execution termination"}
            (ROOT / "target/port/exec-smoke.json").write_text(json.dumps(report, indent=2) + "\n")
            print(json.dumps(report))
        finally:
            subprocess.run(command(RUST, "kill-server"), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10)


if __name__ == "__main__":
    main()
