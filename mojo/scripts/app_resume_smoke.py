import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import tempfile
import time


ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "mojo"
TARGET = ROOT / "target/port"
RUST = ROOT / "target/debug/zz_cli"
MOJO = TARGET / "zz-mojo-resume"
PROBE = TARGET / "app-resume-probe"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    for source, output in ((PORT / "main.mojo", MOJO), (PORT / "tests/app_resume_probe.mojo", PROBE)):
        subprocess.run(["pixi", "run", "--manifest-path", str(PORT / "pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(PORT), str(source), "-o", str(output)], cwd=ROOT, check=True)
    with tempfile.TemporaryDirectory(prefix="zz-resume-", dir="/tmp") as directory:
        directory = Path(directory)
        socket = directory / "s"
        client = directory / "prefix/bin/client"
        client.parent.mkdir(parents=True)
        shutil.copyfile(MOJO, client)
        client.chmod(0o700)
        backend = directory / "prefix/share/backend/zz"
        backend.parent.mkdir(parents=True)
        backend.symlink_to(RUST)
        application = directory / "prefix/share/frontend/zz-app"
        application.parent.mkdir(parents=True)
        report = directory / "resumed"
        observer = directory / "observer.json"
        application.write_text(
            "#!/usr/bin/python3\nimport os, sys, json, signal\n"
            "fds = {}\n"
            "for key in os.listdir('/proc/self/fd'):\n"
            "    try: fds[key] = os.readlink('/proc/self/fd/' + key)\n"
            "    except FileNotFoundError: pass\n"
            "record = {'argv':sys.argv, 'fds':fds, 'blocked':sorted(signal.pthread_sigmask(signal.SIG_BLOCK, [])), 'removed':[key for key in ['TMUX','TMUX_PANE'] if key in os.environ], 'cwd':os.getcwd(), 'startup':os.environ.get('ZZ_APP_STARTUP_DIRECTORY')}\n"
            "with open(os.environ['APP_OBSERVER_REPORT'], 'w') as output: json.dump(record, output)\n"
            "error = os.open(os.environ['APP_ERROR_REPORT'], os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)\nos.dup2(error, 2)\nos.close(error)\n"
            "os.execv(os.environ['APP_RESUME_PROBE'], sys.argv)\n"
        )
        application.chmod(0o700)
        environment = os.environ.copy()
        environment.update({"ZZ_SOCKET": str(socket), "ZZ_BEND": str(TARGET / "zz-bend"), "TMUX": "", "TMUX_PANE": "", "APP_RESUME_REPORT": str(report), "APP_MESSAGE_REPORT": str(directory / "message"), "APP_VIEW_REPORT": str(directory / "view"), "APP_NORMAL_ERROR_REPORT": str(directory / "normal-error"), "APP_OBSERVER_REPORT": str(observer), "APP_ERROR_REPORT": str(directory / "app-error"), "APP_RESUME_PROBE": str(PROBE)})
        environment.pop("ZZ_DAEMON", None)

        def invoke(binary, arguments):
            result = subprocess.run([str(binary), "-S", str(socket), "-f", "/dev/null", *arguments], cwd=directory, env=environment, stdin=subprocess.DEVNULL, capture_output=True, timeout=25)
            return result.returncode, result.stdout, result.stderr

        def resumed(arguments, output=b""):
            report.unlink(missing_ok=True)
            observer.unlink(missing_ok=True)
            result = invoke(client, arguments)
            assert result == (0, output, b""), (arguments, result)
            deadline = time.monotonic() + 15
            while not report.exists() and time.monotonic() < deadline:
                time.sleep(0.01)
            assert report.exists(), (arguments, observer.read_text() if observer.exists() else "no app process", (directory / "app-error").read_text() if (directory / "app-error").exists() else "")
            server, client_id, session, count = map(int, report.read_text().split())
            assert server > 0 and client_id > 0 and count > 0
            record = json.loads(observer.read_text())
            args = record["argv"]
            assert args[1:4] == ["--socket", str(socket), "--resume-fds"], record
            assert set(record["fds"]) == {"0", "1", "2", *args[4:7]}, record
            assert record["blocked"] == [] and record["removed"] == []
            assert record["cwd"] == record["startup"] == str(directory)
            assert all(record["fds"][str(fd)] == "/dev/null" for fd in range(3)), record
            return server, session, count

        try:
            server, session, count = resumed(["new-session", "-s", "resume-proof", "sleep 120"])
            assert count == 1
            assert (directory / "normal-error").read_text() == "invalid command: command select-pane: unknown flag -Q"
            assert invoke(RUST, ["list-sessions", "-F", "#{session_id}:#{session_name}"]) == (0, f"${session}:resume-proof\n".encode(), b"")
            counter = directory / "once"
            same_server, _, count = resumed(["new-session", "-s", "chain-proof", "sleep 120", ";", "run-shell", f"printf x >> '{counter}'"])
            assert same_server == server and count == 2
            deadline = time.monotonic() + 5
            while not counter.exists() and time.monotonic() < deadline:
                time.sleep(0.01)
            assert counter.read_bytes() == b"x"
            assert invoke(RUST, ["set-option", "-s", "command-alias[50]", "resume-alias=new-session -s alias-proof 'sleep 120'"])[0] == 0
            same_server, _, count = resumed(["resume-alias"])
            assert same_server == server and count == 3
            same_server, _, count = resumed(["app"])
            assert same_server == server and count == 3
            same_server, attached_session, count = resumed(["attach", "resume-proof"])
            assert same_server == server and attached_session == session and count == 3
            assert (directory / "view").read_text() == "0\n"
            same_server, _, count = resumed(["attach-session", "-r", "-f", "ignore-size", "-t", "alias-proof"])
            assert same_server == server and count == 3
            assert (directory / "view").read_text() == "1\nignore-size"
            assert invoke(RUST, ["set-option", "-s", "command-alias[51]", "attach=display-message -p"])[0] == 0
            assert invoke(client, ["attach", "OVERRIDDEN_ATTACH"]) == invoke(RUST, ["attach", "OVERRIDDEN_ATTACH"]) == (0, b"OVERRIDDEN_ATTACH\n", b"")
            assert invoke(RUST, ["set-option", "-s", "command-alias[51]", "unrelated=display-message -p"])[0] == 0
            same_server, _, count = resumed(["new-session", "-P", "-F", "#{session_name}", "-s", "printed-proof", "sleep 120"], b"printed-proof\n")
            assert same_server == server and count == 4
            expected = invoke(RUST, ["select-pane", "-t", "%999999"])[2].decode().rstrip("\n")
            same_server, _, count = resumed(["new-session", "-s", "error-after-attach", "sleep 120", ";", "select-pane", "-t", "%999999"])
            assert same_server == server and count == 5
            assert (directory / "message").read_text() == expected
            expected = invoke(RUST, ["new-session", "-d", "-s", "resume-proof", "sleep 120"])
            assert expected[0] == 1
            actual = invoke(client, ["display-message", "-p", "BEFORE_FAILURE", ";", "new-session", "-s", "resume-proof", "sleep 120"])
            assert actual == (1, b"BEFORE_FAILURE\n", expected[2]), (actual, expected)
            body = application.read_text()
            application.write_text(body.replace("os.execv(os.environ['APP_RESUME_PROBE'], sys.argv)", "sys.exit(0)"))
            actual = invoke(client, ["new-session", "-s", "must-not-execute", "sleep 120"])
            assert actual[0] == 1 and b"closed before completing startup" in actual[2], actual
            assert b"must-not-execute" not in invoke(RUST, ["list-sessions", "-F", "#{session_name}"])[1]
            application.write_text(body)
            invoke(RUST, ["kill-server"])
            invalid = [["attach", "-Q"], ["attach", "-t"], ["attach", "x", "y"], ["attach", "--restart-daemon", "--restart-daemon"], ["attach", "-t", "x", "positional"], ["attach", "-t", "x", ";", "unknown-later"], ["attach", ";", "new-session", "-c"], ["-N", "attach"]]
            for arguments in invalid:
                observer.unlink(missing_ok=True)
                reference = invoke(RUST, arguments)
                actual = invoke(client, arguments)
                assert actual == reference, (arguments, reference, actual)
                assert not socket.exists() and not observer.exists(), (arguments, actual)
            actual = invoke(client, ["attach", "--restart-daemon"])
            assert actual[0] == 1 and b"not implemented by this client" in actual[2] and not socket.exists(), actual
            cold_config = directory / "cold.cfg"
            cold_config.write_text("new-session -d -s cold-target 'sleep 120'\n")
            _, _, count = resumed(["-f", str(cold_config), "attach-session", "-t", "cold-target"])
            assert count == 1
            invoke(RUST, ["kill-server"])
            results = []
            for binary in (RUST, client):
                results.append(invoke(binary, ["attach"]))
                invoke(RUST, ["kill-server"])
            assert results == [(1, b"", b"no sessions\n")] * 2, results
            result = {"passed": True, "groups": 8 + 3 + len(invalid) + 3, "checks": ["cold new-session same-socket prepared attachment", "exact command chain executes once", "warm expanded alias prepared metadata", "application attaches existing session", "only explicit socket/payload/reply descriptors inherited", "payload descriptor retired and socket CLOEXEC restored", "native signal/stdio/environment/startup cwd policy", "initial stdout and unattached error ordering", "attached runtime errors remain client messages", "child initialization EOF cannot report success", "ordinary interactive RPC retains display error formatting", "native attach positional/read-only/flags and warm alias ownership", "cold attach validation leaves daemon stopped", "cold configured attach and native no-sessions error"], "remaining": ["daemon restart", "remote/control/shell/foreground daemon frontends"]}
            (TARGET / "app-resume-smoke.json").write_text(json.dumps(result, indent=2) + "\n")
            print(json.dumps(result))
        finally:
            invoke(RUST, ["kill-server"])


if __name__ == "__main__":
    main()
