import json
import os
from pathlib import Path
import runpy
import socket as sockets
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "mojo"
TARGET = ROOT / "target/port"
RUST = ROOT / "target/debug/zz_cli"
MOJO = TARGET / "zz-mojo-cli-globals"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    TARGET.mkdir(parents=True, exist_ok=True)
    subprocess.run(["pixi", "run", "--manifest-path", str(PORT / "pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(PORT), str(PORT / "main.mojo"), "-o", str(MOJO)], cwd=ROOT, check=True)
    checked = 0
    rejected = 0
    with tempfile.TemporaryDirectory(prefix="zz-cli-route-", dir="/tmp") as directory:
        directory = Path(directory)
        socket = directory / "s"
        environment = os.environ.copy()
        environment.update({"ZZ_BEND": str(TARGET / "zz-bend"), "ZZ_SOCKET": str(socket), "TMUX": "", "TMUX_PANE": ""})

        def invoke(binary, arguments, env=None):
            result = subprocess.run([str(binary), *arguments], cwd=directory, env=environment if env is None else env, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=20)
            return result.returncode, result.stdout, result.stderr

        def compare(arguments, env=None):
            nonlocal checked
            reference = invoke(RUST, arguments, env)
            actual = invoke(MOJO, arguments, env)
            assert actual == reference, (arguments, reference, actual)
            checked += 1
            return actual

        spawned = [socket]
        try:
            assert invoke(RUST, ["-f", "/dev/null", "new-session", "-d", "-s", "global-proof", "sleep 120"])[0] == 0
            for args in (["-h"], ["-V"], ["--version"], ["--help"], ["help"], ["protocol-version"], ["-S", str(socket), "protocol-version"], ["protocol-version", "extra"], ["--socket"], ["--socket="], ["--host"], ["--host="], ["--host=-x", "list-sessions"], ["-X"], ["--unknown"], ["-D"], ["-D", "list-sessions"], ["-c"], ["-c", "x", "list-sessions"], ["-L", "name", "-V"], ["--host=-x", "-h"]):
                compare(args)
            for prefix in ([], ["-S", str(socket)], ["-S", "s"], ["-S" + str(socket)], ["--socket=" + str(socket)], ["-S", str(directory / "missing"), "--socket", str(socket)], ["-2u"], ["-qvN"], ["-f", str(directory / "missing-config")], ["-T", "RGB,clipboard,utf8", "-T", "RGB@", "-2"]):
                compare([*prefix, "list-sessions", "-F", "#{session_name}"])
            compare(["-u", "display-message", "-p", "#{client_utf8}:#{client_termfeatures}"])
            compare(["display-message", "-p", "世界"])
            compare(["display-message", "-p", "--", "--verbose"])
            compare([b"set-buffer", b"-b", b"raw-argv", b"raw\xff\x01"])
            compare(["save-buffer", "-b", "raw-argv", "-"])
            inherited = environment.copy()
            inherited.pop("ZZ_SOCKET")
            inherited["TMUX"] = "foreign-server,123,0"
            compare(["list-sessions"], inherited)
            compare(["-S", str(socket), "list-sessions", "-F", "#{session_name}"], inherited)
            compare(["--version"], inherited)
            compare(["protocol-version"], inherited)
            compare(["--help"], inherited)
            for path in (directory / "missing", directory / "regular", directory / "stale", directory / "failed (errno 99)"):
                if path.name == "regular":
                    path.write_text("private fixture")
                if path.name == "stale":
                    stale = sockets.socket(sockets.AF_UNIX)
                    stale.bind(str(path))
                    stale.close()
                compare(["-S", str(path), "list-sessions"])
            compare(["-S", "", "list-sessions"])
            empty = environment.copy()
            empty["ZZ_SOCKET"] = ""
            compare(["list-sessions"], empty)
            label_root = directory / "labels"
            label_root.mkdir()
            labelled = environment.copy()
            labelled["TMUX_TMPDIR"] = str(label_root)
            label_socket = label_root / f"tmux-{os.getuid()}" / "work"
            spawned.append(label_socket)
            assert invoke(RUST, ["-L", "work", "-f", "/dev/null", "new-session", "-d", "-s", "label-proof", "sleep 120"], labelled)[0] == 0
            compare(["-Lwork", "list-sessions", "-F", "#{session_name}"], labelled)
            runtime = directory / "runtime"
            runtime.mkdir()
            defaults = environment.copy()
            defaults.pop("ZZ_SOCKET")
            defaults["XDG_RUNTIME_DIR"] = str(runtime)
            default_socket = runtime / "zz/default.sock"
            spawned.append(default_socket)
            assert invoke(RUST, ["-f", "/dev/null", "new-session", "-d", "-s", "default-proof", "sleep 120"], defaults)[0] == 0
            compare(["list-sessions", "-F", "#{session_name}"], defaults)
            before = invoke(RUST, ["list-sessions", "-F", "#{session_name}"])
            for args in ([], ["-C"], ["-c", "touch unsupported-shell-must-not-run"], ["--host", "remote", "list-sessions"], ["app"], ["daemon"], ["display-message", "--help"], ["--verbose", "list-sessions"]):
                actual = invoke(MOJO, args)
                assert actual[0] == 1 and b"not implemented by this client" in actual[2], (args, actual)
                rejected += 1
            assert not (directory / "unsupported-shell-must-not-run").exists()
            assert invoke(RUST, ["list-sessions", "-F", "#{session_name}"]) == before
            reference = invoke(RUST, ["--kill-server"])
            assert invoke(RUST, ["-f", "/dev/null", "new-session", "-d", "-s", "kill-proof", "sleep 120"])[0] == 0
            actual = invoke(MOJO, ["--kill-server"])
            assert actual == reference, (reference, actual)
            checked += 1
            compare(["--kill-server"])
            report = {"passed": True, "native_differentials": checked, "explicit_unsupported": rejected, "scope": "global flags, help/version, raw argv, Unix socket selection/defaults and security, features/UTF8 and existing-daemon modern Exec", "remaining": ["default-client-command startup", "interactive resume and desktop launch", "remote and control mode", "shell startup", "per-command help", "verbose diagnostics", "non-UTF-8 socket paths", "full Unix path-length boundary"]}
            (TARGET / "cli-globals-smoke.json").write_text(json.dumps(report, indent=2) + "\n")
            print(json.dumps(report))
        finally:
            for path in spawned:
                invoke(RUST, ["-S", str(path), "kill-server"])


if __name__ == "__main__":
    main()
