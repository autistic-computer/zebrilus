import os
import pty
import subprocess
import sys
import time


def main():
    binary = os.environ["ZZ_ENVBYTES_BIN"]
    prefix = os.environ["ZZ_ENVBYTES_ARGS"].split()
    root = os.environ["ZZ_ENVBYTES_ROOT"]

    pid, master = pty.fork()
    if pid == 0:
        os.execvp(binary, [binary, *prefix, "new-session", "-s", "envbytes"])

    for name, path in (("ZZBYTES", "attach-bytes"), ("ZZPLAIN", "attach-plain")):
        answer = b""
        for _ in range(80):
            answer = subprocess.run(
                [binary, *prefix, "show-environment", "-t", "envbytes", name],
                capture_output=True,
            ).stdout
            if answer.startswith(name.encode() + b"="):
                break
            time.sleep(0.25)
        with open(os.path.join(root, path), "wb") as handle:
            handle.write(answer)

    subprocess.run(
        [binary, *prefix, "kill-session", "-t", "envbytes"], capture_output=True
    )
    try:
        os.close(master)
    except OSError:
        pass
    try:
        os.waitpid(pid, 0)
    except ChildProcessError:
        pass


if __name__ == "__main__":
    main()
    sys.exit(0)
