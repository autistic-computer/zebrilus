import fcntl
import os
import pty
import select
import struct
import sys
import termios
import time

from pty_queries import Answerer

COLUMNS = 80
ROWS = 24


def main():
    sys.stdout.reconfigure(line_buffering=True)
    out_path = sys.argv[1]
    deadline = time.time() + float(sys.argv[2])
    command = sys.argv[3:]

    pid, master = pty.fork()
    if pid == 0:
        os.execvp(command[0], command)
    fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLUMNS, 0, 0))

    answerer = Answerer(master)
    drawn = bytearray()
    while time.time() < deadline:
        readable, _, _ = select.select([master], [], [], 0.2)
        if not readable:
            continue
        try:
            chunk = os.read(master, 65536)
        except OSError:
            break
        if not chunk:
            break
        drawn += chunk
        answerer.feed(chunk)
    try:
        os.close(master)
    except OSError:
        pass

    with open(out_path, "wb") as handle:
        handle.write(bytes(drawn))

    status = reap(pid, deadline)
    print("exit=%s" % status)


def reap(pid, deadline):
    while True:
        reaped, status = os.waitpid(pid, os.WNOHANG)
        if reaped == pid:
            if os.WIFEXITED(status):
                return os.WEXITSTATUS(status)
            return "signal"
        if time.time() >= deadline:
            return "stalled"
        time.sleep(0.05)


main()
