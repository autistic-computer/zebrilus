import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/port"
RUST = ROOT / "target/debug/zz_cli"
CLI = TARGET / "zz-mojo"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    checked = 0
    with tempfile.TemporaryDirectory(prefix="zz-default-cli-", dir="/tmp") as directory:
        directory = Path(directory)
        socket = directory / "s"
        client = directory / "prefix/bin/client"
        client.parent.mkdir(parents=True)
        shutil.copyfile(CLI, client)
        client.chmod(0o700)
        backend = directory / "prefix/share/backend/zz"
        backend.parent.mkdir(parents=True)
        backend.symlink_to(RUST)
        environment = os.environ.copy()
        environment.update({"ZZ_SOCKET": str(socket), "ZZ_BEND": str(TARGET / "zz-bend"), "TMUX": "", "TMUX_PANE": "", "ZZ_PANE": ""})
        environment.pop("ZZ_DAEMON", None)

        def invoke(binary, arguments=(), payload=None):
            result = subprocess.run([str(binary), "-S", str(socket), "-f", "/dev/null", *arguments], cwd=directory, env=environment, input=payload, stdin=subprocess.DEVNULL if payload is None else None, capture_output=True, timeout=25)
            return result.returncode, result.stdout, result.stderr

        try:
            assert invoke(RUST, ["new-session", "-d", "-s", "default-proof", "sleep 120"])[0] == 0
            cases = [
                "display-message -p DEFAULT",
                'display-message -p "quoted world"',
                'NAME=overlay\ndisplay-message -p "$NAME"',
                "%if 1\ndisplay-message -p WRONG\n%else\ndisplay-message -p SOURCE_ELSE\n%endif",
                'if-shell -F 1 { display-message -p "typed block" }',
                'display-message -p "\\377\\001"',
                "display-message -p first ; display-message -p second",
                'display-message -p "~"',
                "display-message -p ~",
                "display-message -p ~root",
                "new-session -d -s default-detached 'sleep 120'",
            ]
            for text in cases:
                configured = invoke(RUST, ["set-option", "-s", "default-client-command", text])
                assert configured[0] == 0, (text, configured)
                reference = invoke(RUST)
                if text.startswith("new-session"):
                    invoke(RUST, ["kill-session", "-t", "default-detached"])
                actual = invoke(client)
                if text.startswith("new-session"):
                    invoke(RUST, ["kill-session", "-t", "default-detached"])
                assert actual == reference, (text, reference, actual)
                checked += 1
            text = "load-buffer -b default-stdin - ; save-buffer -b default-stdin -"
            assert invoke(RUST, ["set-option", "-s", "default-client-command", text])[0] == 0
            payload = b"default stdin preserved\0\xff"
            assert invoke(client, payload=payload) == invoke(RUST, payload=payload) == (0, payload, b"")
            checked += 1
            invoke(RUST, ["kill-server"])
            for line in ("set-option -s default-client-command 'display-message -p COLD_DEFAULT'\n", "set-option -s default-client-command 'if-shell -F 1 { display-message -p COLD_TYPED }'\n"):
                config = directory / "default.cfg"
                config.write_text(line)
                results = []
                for binary in (RUST, client):
                    results.append(invoke(binary, ["-f", str(config)]))
                    invoke(RUST, ["kill-server"])
                assert results[0] == results[1], (line, results)
                checked += 1
            reference = invoke(RUST, ["-N"])
            actual = invoke(client, ["-N"])
            assert actual == reference and not socket.exists(), (reference, actual)
            checked += 1
            result = {"passed": True, "native_groups": checked, "scope": "actual configured default retrieval, source DSL/typed blocks/assignments/false conditions/raw escapes/home expansion, exact output/errors, detached commands, preserved stdin, cold config startup and -N"}
            (TARGET / "cli-default-smoke.json").write_text(json.dumps(result, indent=2) + "\n")
            print(json.dumps(result))
        finally:
            invoke(RUST, ["kill-server"])


if __name__ == "__main__":
    main()
