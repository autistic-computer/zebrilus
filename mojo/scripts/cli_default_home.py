import json
from pathlib import Path
import pwd
import runpy
import subprocess


ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/port"


def uint(output, value):
    while value >= 128:
        output.append((value & 127) | 128)
        value >>= 7
    output.append(value)


def text(output, value):
    data = value.encode()
    uint(output, len(data))
    output.extend(data)


def main():
    runpy.run_path(str(ROOT / "scripts/port/guard.py"))["ensure"]()
    users = [None, "", "root", "demfabris", "zz-default-missing-user", "nobody", "a\0b", "世界", "x" * 1022]
    users.extend(user.pw_name for user in pwd.getpwall() if len(user.pw_name.encode()) > 15 and user.pw_name not in users)
    subprocess.run(["cargo", "build", "--locked", "-p", "zz-mux", "--example", "cli_default_vectors"], cwd=ROOT, check=True, timeout=300)
    result = subprocess.run([str(ROOT / "target/debug/examples/cli_default_vectors")], input=json.dumps({"cases": [], "users": users}).encode(), cwd=ROOT, capture_output=True, check=True, timeout=30)
    source = json.loads(result.stdout)
    if source["abi"] != {"record": 48, "directory": 32, "pointer": 8}:
        raise AssertionError("native home adapter requires its verified Linux 64-bit passwd layout")
    requests = bytearray()
    wanted = bytearray()
    uint(requests, len(users))
    for user, actual in zip(users, source["users"], strict=True):
        uint(requests, int(user is not None))
        if user is not None:
            text(requests, user)
        uint(wanted, int(actual["home"] is not None))
        if actual["home"] is not None:
            text(wanted, actual["home"])
    request = TARGET / "cli-default-home-input.bin"
    output = TARGET / "cli-default-home-output.bin"
    binary = TARGET / "cli-default-home-probe"
    request.write_bytes(requests)
    subprocess.run(["pixi", "run", "--manifest-path", str(ROOT / "mojo/pixi.toml"), "mojo", "build", "--fp-mode=contract=off", "-I", str(ROOT / "mojo"), str(ROOT / "mojo/tests/cli_default_home_probe.mojo"), "-o", str(binary)], cwd=ROOT, check=True, timeout=300)
    subprocess.run([str(binary), str(request), str(output)], cwd=ROOT, check=True, timeout=30)
    if output.read_bytes() != wanted:
        raise AssertionError("native home adapter differs from actual source user_home")
    report = {"passed": True, "cases": len(users), "abi": source["abi"], "source": "actual Rust passwd user_home and native C layout", "scope": "current uid, named/empty/missing users, embedded NUL, Unicode and long names; no environment mutation"}
    (TARGET / "cli-default-home.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
