import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import tempfile
import time


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/port"
RUST = ROOT / "target/debug/zz_cli"
CLI = TARGET / "zz-mojo"
APP = TARGET / "zz-mojo-app"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    with tempfile.TemporaryDirectory(prefix="zz-app-launch-", dir="/tmp") as directory:
        directory = Path(directory)
        socket = directory / "s"
        config = directory / "safe.cfg"
        config.write_text("set-option -g default-command 'sleep 120'\n")
        client = directory / "prefix/bin/client"
        client.parent.mkdir(parents=True)
        shutil.copyfile(CLI, client)
        client.chmod(0o700)
        backend = directory / "prefix/share/backend/zz"
        backend.parent.mkdir(parents=True)
        backend.symlink_to(RUST)
        application = directory / "prefix/share/frontend/zz-app"
        application.parent.mkdir(parents=True)
        application.write_text(
            "#!/usr/bin/python3\nimport os, sys, subprocess, json\n"
            "assert sys.argv[3] == '--resume-fds'\n"
            "fds = tuple(map(int, sys.argv[4:7]))\n"
            "with open(os.environ['APP_NATIVE_LOG'], 'wb') as log:\n"
            "    child = subprocess.Popen([os.environ['APP_NATIVE_BINARY'], *sys.argv[1:], '--frames', '6'], pass_fds=fds, stderr=log)\n"
            "    for fd in fds: os.close(fd)\n"
            "    status = child.wait(timeout=30)\n"
            "with open(os.environ['APP_NATIVE_REPORT'], 'w') as report: json.dump({'status':status, 'pid':child.pid}, report)\n"
            "sys.exit(status)\n"
        )
        application.chmod(0o700)
        report = directory / "app.json"
        log = directory / "app.log"
        environment = os.environ.copy()
        environment.update({"ZZ_SOCKET": str(socket), "ZZ_BEND": str(TARGET / "zz-bend"), "TMUX": "", "TMUX_PANE": "", "ZZ_PANE": "", "APP_NATIVE_BINARY": str(APP), "APP_NATIVE_REPORT": str(report), "APP_NATIVE_LOG": str(log)})
        environment.pop("ZZ_DAEMON", None)

        def invoke(binary, arguments):
            result = subprocess.run([str(binary), "-S", str(socket), "-f", "/dev/null", "-f", str(config), *arguments], cwd=directory, env=environment, stdin=subprocess.DEVNULL, capture_output=True, timeout=35)
            return result.returncode, result.stdout, result.stderr

        def launched(arguments):
            report.unlink(missing_ok=True)
            result = invoke(client, arguments)
            assert result[0] == 0 and result[2] == b"", (arguments, result, log.read_text() if log.exists() else "")
            deadline = time.monotonic() + 35
            while not report.exists() and time.monotonic() < deadline:
                time.sleep(0.01)
            assert report.exists(), (arguments, log.read_text() if log.exists() else "")
            observed = json.loads(report.read_text())
            assert observed["status"] == 0, (arguments, observed, log.read_text())
            return result

        try:
            result = launched(["new-session", "-P", "-F", "#{pane_id}", "-s", "native-launch", "printf 'native startup\\n'; sleep 120"])
            pane = invoke(RUST, ["list-panes", "-t", "native-launch", "-F", "#{pane_id}"])
            assert result == pane, (result, pane)
            before = invoke(RUST, ["list-sessions", "-F", "#{session_id}:#{session_name}"])
            environment["TMUX"] = "parent-session,123,0"
            environment["TMUX_PANE"] = "%42"
            assert launched(["app"]) == (0, b"", b"")
            assert invoke(RUST, ["list-sessions", "-F", "#{session_id}:#{session_name}"]) == before
            environment["TMUX"] = ""
            environment["TMUX_PANE"] = ""
            assert launched(["attach", "native-launch"]) == (0, b"", b"")
            assert launched(["attach-session", "-r", "-f", "ignore-size", "-t", "native-launch"]) == (0, b"", b"")
            assert invoke(RUST, ["list-sessions", "-F", "#{session_id}:#{session_name}"]) == before
            assert launched([]) == (0, b"", b"")
            assert invoke(RUST, ["list-sessions", "-F", "#{session_id}:#{session_name}"]) == before
            invoke(RUST, ["kill-server"])
            assert launched([]) == (0, b"", b"")
            sessions = invoke(RUST, ["list-sessions", "-F", "#{session_name}"])
            assert sessions == (0, b"0\n", b""), sessions
            result = {"passed": True, "groups": 6, "frames_per_launch": 6, "host": "actual native Mojo application", "scope": "cold same-socket prepared new-session launch with exact -P stdout, warm app/bare/native attach/read-only launch without duplicate sessions, cold bare default startup, native window/renderer/worker shutdown, private cwd resources and inherited startup context"}
            (TARGET / "app-launch-smoke.json").write_text(json.dumps(result, indent=2) + "\n")
            print(json.dumps(result))
        finally:
            invoke(RUST, ["kill-server"])


if __name__ == "__main__":
    main()
