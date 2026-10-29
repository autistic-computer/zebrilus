import base64
import fcntl
import os
import pty
import re
import struct
import sys
import termios

OSC52 = re.compile(rb"\x1b\]52;([^;\x07\x1b]*);([A-Za-z0-9+/=]*)(?:\x07|\x1b\\)")


def record(path, columns, rows, command):
    pid, master = pty.fork()
    if pid == 0:
        os.execvp(command[0], command)
    fcntl.ioctl(
        master, termios.TIOCSWINSZ, struct.pack("HHHH", rows, columns, 0, 0)
    )
    with open(path, "ab", buffering=0) as handle:
        while True:
            try:
                chunk = os.read(master, 4096)
            except OSError:
                break
            if not chunk:
                break
            handle.write(chunk)
    os.waitpid(pid, 0)


def scan(path):
    try:
        with open(path, "rb") as handle:
            data = handle.read()
    except OSError:
        return
    for _, payload in OSC52.findall(data):
        try:
            decoded = base64.b64decode(payload, validate=True)
        except Exception:
            continue
        sys.stdout.write(decoded.decode("utf-8", "replace") + "\n")


def main():
    action = sys.argv[1]
    if action == "record":
        record(sys.argv[2], int(sys.argv[3]), int(sys.argv[4]), sys.argv[5:])
    elif action == "scan":
        scan(sys.argv[2])
    else:
        sys.exit(2)


if __name__ == "__main__":
    main()
