import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]
PORT = ROOT / "mojo"
TARGET = ROOT / "target/port"
RUST = ROOT / "target/debug/zz_cli"
MOJO = TARGET / "zz-mojo-bootstrap"


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    subprocess.run(["pixi", "run", "--manifest-path", str(PORT / "pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(PORT), str(PORT / "main.mojo"), "-o", str(MOJO)], cwd=ROOT, check=True)
    subprocess.run(["python3", str(PORT / "scripts/resources.py"), str(MOJO), "--backend-only"], check=True)
    checked = 0
    refused = 0
    with tempfile.TemporaryDirectory(prefix="zz-cold-", dir="/tmp") as directory:
        directory = Path(directory)
        socket = directory / "s"
        config = directory / "cfg"
        config.write_text("set-option -g @cold-marker COLD_CONFIG\n")
        label_socket = None
        trace = directory / "daemon-started"
        wrapper = directory / "backend"
        wrapper.write_text("#!/usr/bin/python3\nimport os, sys\nassert 'daemon' in sys.argv\nwith open(os.environ['BOOTSTRAP_TRACE'], 'ab') as output: output.write(b'daemon\\n')\nos.execv(os.environ['BOOTSTRAP_NATIVE'], sys.argv)\n")
        wrapper.chmod(0o700)
        environment = os.environ.copy()
        environment.update({"ZZ_BEND": str(TARGET / "zz-bend"), "ZZ_DAEMON": str(wrapper), "BOOTSTRAP_TRACE": str(trace), "BOOTSTRAP_NATIVE": str(RUST), "ZZ_SOCKET": str(socket), "ZZ_PANE": "", "TMUX": "", "TMUX_PANE": ""})

        def invoke(binary, arguments, env=None, data=None):
            result = subprocess.run([str(binary), "-S", str(socket), "-f", str(config), *arguments], cwd=directory, env=environment if env is None else env, input=data, stdin=subprocess.DEVNULL if data is None else None, capture_output=True, timeout=25)
            return result.returncode, result.stdout, result.stderr

        def stop():
            invoke(RUST, ["kill-server"])

        def cold(arguments, data=None, expected_start=False, no_daemon=False):
            nonlocal checked
            results = []
            for binary in (RUST, MOJO):
                stop()
                if binary == MOJO:
                    trace.unlink(missing_ok=True)
                results.append(invoke(binary, arguments, data=data))
                if binary == MOJO and no_daemon:
                    assert not trace.exists(), (arguments, results[-1])
                if expected_start:
                    marker = invoke(RUST, ["show-options", "-gqv", "@cold-marker"])
                    assert marker == (0, b"COLD_CONFIG\n", b""), (arguments, binary, marker)
            assert results[0] == results[1], (arguments, results)
            checked += 1
            stop()

        try:
            cold(["new-session", "-d", "-s", "cold-proof", "sleep 120"], expected_start=True)
            cold(["new", "-d", "-s", "cold-alias", "sleep 120"], expected_start=True)
            cold(["list-commands", "-F", "#{command_name}"])
            cold(["lsk", "-T", "prefix"])
            cold(["start-server"])
            for arguments in (["list-sessions"], ["-N", "new-session", "-d"], ["new-session", "-Q"], ["new-session", "-c"], ["new-session", "-d", ";", "does-not-exist"], ["new-session", "-d", ";", "list-sessions", "extra"], ["kill-s"], ["unknown-command"]):
                cold(arguments, no_daemon=True)
            labels = directory / "labels"
            labels.mkdir()
            labelled = environment.copy()
            labelled["TMUX_TMPDIR"] = str(labels)
            label_socket = labels / f"tmux-{os.getuid()}" / "work"
            for binary in (RUST, MOJO):
                result = invoke(binary, ["-L", "work", "new-session", "-d", "-s", "label-cold", "sleep 120"], labelled)
                assert result == (0, b"", b""), result
                marker = invoke(RUST, ["-L", "work", "show-options", "-gqv", "@cold-marker"], labelled)
                assert marker == (0, b"COLD_CONFIG\n", b""), marker
                assert invoke(RUST, ["-S", str(label_socket), "kill-server"], labelled) == (0, b"", b"")
            checked += 1
            for name in ("new-session", "list-commands"):
                results = [invoke(binary, ["-L", "missing/nested", name, *(["-d"] if name == "new-session" else [])], labelled) for binary in (RUST, MOJO)]
                assert results[0] == results[1] and b"error creating" in results[0][2], (name, results)
                assert results[0][0] == (0 if name == "new-session" else 1), results
                checked += 1
            payload = b"cold stdin once\0\xff"
            cold(["new-session", "-d", "-s", "cold-stdin", "sleep 120", ";", "load-buffer", "-b", "cold", "-", ";", "save-buffer", "-b", "cold", "-"], data=payload, expected_start=True)
            assert invoke(MOJO, ["new-session", "-d", "-s", "warm-proof", "sleep 120"])[0] == 0
            assert invoke(RUST, ["set-option", "-s", "command-alias[50]", "warmcustom=display-message -p WARM_DYNAMIC_ALIAS"])[0] == 0
            assert invoke(MOJO, ["warmcustom"]) == invoke(RUST, ["warmcustom"]) == (0, b"WARM_DYNAMIC_ALIAS\n", b"")
            checked += 1
            broken = environment.copy()
            broken["ZZ_DAEMON"] = ""
            assert invoke(MOJO, ["display-message", "-p", "warm skips backend"], broken) == (0, b"warm skips backend\n", b"")
            checked += 1
            stop()
            for override, fragment in (("", b"ZZ_DAEMON requires"), ("zz_cli", b"PATH lookup is disabled"), (str(directory / "missing"), b"could not launch daemon backend")):
                broken = environment.copy()
                broken["ZZ_DAEMON"] = override
                result = invoke(MOJO, ["new-session", "-d"], broken)
                assert result[0] == 1 and fragment in result[2], result
                assert not socket.exists(), result
                refused += 1
            invalid = directory / "invalid"
            invalid.write_text("touch SHOULD_NOT_FALL_BACK_TO_SHELL\n")
            invalid.chmod(0o700)
            broken["ZZ_DAEMON"] = str(invalid)
            result = invoke(MOJO, ["new-session", "-d"], broken)
            assert result[0] == 1 and b"Exec format error" in result[2], result
            assert not (directory / "SHOULD_NOT_FALL_BACK_TO_SHELL").exists()
            refused += 1
            isolated = directory / "prefix/bin/client"
            isolated.parent.mkdir(parents=True)
            shutil.copyfile(MOJO, isolated)
            isolated.chmod(0o700)
            packaged = environment.copy()
            packaged.pop("ZZ_DAEMON")
            result = invoke(isolated, ["new-session", "-d"], packaged)
            assert result[0] == 1 and b"daemon backend is missing" in result[2], result
            refused += 1
            subprocess.run(["python3", str(PORT / "scripts/resources.py"), str(isolated), "--backend-only"], check=True)
            assert invoke(isolated, ["new-session", "-d", "-s", "packaged", "sleep 120"], packaged)[0] == 0
            assert invoke(RUST, ["list-sessions", "-F", "#{session_name}"]) == (0, b"packaged\n", b"")
            checked += 1
            report = {"passed": True, "native_groups": checked, "explicit_backend_failures": refused, "scope": "cold native start policy, whole-chain static rejection, -N, config/stdin preservation, warm dynamic aliases, explicit override and isolated bundled resource", "remaining": ["daemon restart", "foreground daemon entry", "remote/control/shell frontends"]}
            (TARGET / "cli-bootstrap-smoke.json").write_text(json.dumps(report, indent=2) + "\n")
            print(json.dumps(report))
        finally:
            stop()
            if label_socket is not None:
                invoke(RUST, ["-S", str(label_socket), "kill-server"])


if __name__ == "__main__":
    main()
