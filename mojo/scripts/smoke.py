import json
import argparse
import os
import runpy
from pathlib import Path
import subprocess
import tempfile


port = Path(__file__).resolve().parents[1]
repo = port.parent
rust = repo / "target/debug/zz_cli"
mojo = repo / "target/port/zz-mojo"


def invoke(binary, socket, *args, expected=0, environment=None):
    result = subprocess.run(
        [str(binary), "-S" if binary == rust else "--socket", str(socket), *args],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=20,
        env=environment,
    )
    if result.returncode != expected:
        raise AssertionError((args, result.returncode, result.stdout, result.stderr))
    return result


def main():
    runpy.run_path(str(repo / "scripts/port/guard.py"))["ensure"]()
    parser = argparse.ArgumentParser()
    parser.add_argument("--ui", action="store_true")
    options = parser.parse_args()
    if not rust.exists():
        raise SystemExit("build the Rust reference first: cargo build -p zz-cli")
    with tempfile.TemporaryDirectory(prefix="zzmojo-", dir="/tmp") as directory:
        socket = Path(directory) / "s"
        try:
            invoke(rust, socket, "-f", "/dev/null", "new-session", "-d", "-s", "mojo-proof", "printf 'native mojo socket proof\\n'; exec bash --norc")
            checked = 0
            cases = [
                ["list-sessions", "-F", "#{session_id}:#{session_name}"],
                ["list-windows", "-a", "-F", "#{window_id}:#{window_name}"],
                ["list-panes", "-a", "-F", "#{pane_id}:#{pane_current_command}"],
                ["display-message", "-p", "héllo world"],
                ["capture-pane", "-p", "-t", "%0"],
            ]
            for args in cases:
                reference = invoke(rust, socket, *args)
                actual = invoke(mojo, socket, *args)
                if (actual.stdout, actual.stderr) != (reference.stdout, reference.stderr):
                    raise AssertionError((args, reference.stdout, actual.stdout, reference.stderr, actual.stderr))
                checked += 1
            if not options.ui:
                for origin in ("%invalid", "%18446744073709551616", "%+"):
                    environment = os.environ.copy()
                    environment["ZZ_PANE"] = origin
                    args = ["display-message", "-p", "origin fallback"]
                    reference = invoke(rust, socket, *args, environment=environment)
                    actual = invoke(mojo, socket, *args, environment=environment)
                    if (actual.stdout, actual.stderr) != (reference.stdout, reference.stderr):
                        raise AssertionError(("invalid origin", origin, reference.stdout, actual.stdout, reference.stderr, actual.stderr))
                    checked += 1
            args = ["select-pane", "-t", "%999999"]
            reference = invoke(rust, socket, *args, expected=1)
            actual = invoke(mojo, socket, *args, expected=1)
            if (actual.stdout, actual.stderr) != (reference.stdout, reference.stderr):
                raise AssertionError(("error response", reference.stderr, actual.stderr))
            checked += 1
            invoke(mojo, socket, "rename-session", "-t", "$0", "héllo \\ next")
            args = ["list-sessions", "-F", "#{session_id}\t#{q:session_name}"]
            reference = invoke(rust, socket, *args)
            actual = invoke(mojo, socket, *args)
            if actual.stdout != reference.stdout:
                raise AssertionError(("escaped unicode discovery", reference.stdout, actual.stdout))
            checked += 1
            invoke(mojo, socket, "split-window", "-h", "-t", "%0", "exec bash --norc")
            panes = invoke(mojo, socket, "list-panes", "-a", "-F", "#{pane_id}")
            if panes.stdout != b"%0\n%1\n":
                raise AssertionError(("split command", panes.stdout))
            checked += 1
            if options.ui:
                exercise = subprocess.run(
                    [str(repo / "target/port/zz-mojo-shot"), str(socket), str(repo / "target/port/smoke-workspace.png"), "--exercise"],
                    cwd=repo.parent / "mjui",
                    timeout=60,
                )
                if exercise.returncode != 0:
                    panes = invoke(rust, socket, "list-panes", "-a", "-F", "#{pane_id}")
                    for pane in panes.stdout.decode().splitlines():
                        captured = invoke(rust, socket, "capture-pane", "-p", "-t", pane)
                        print(pane, repr(captured.stdout), flush=True)
                    raise SystemExit("native workspace exercise failed")
                checked += 4
            print(json.dumps({"status": "passed", "daemon_protocol": 118, "cases": checked}))
        finally:
            subprocess.run([str(rust), "-S", str(socket), "kill-server"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10)


if __name__ == "__main__":
    main()
