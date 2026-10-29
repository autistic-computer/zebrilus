import os
import signal
import subprocess
import sys
import time

SESSION = "=w"


def environment():
    clean = dict(os.environ)
    for name in ("TMUX", "TMUX_PANE", "ZZ_SOCKET", "ZZ_SESSION", "ZZ_PANE"):
        clean.pop(name, None)
    return clean


def control(prefix):
    return subprocess.Popen(
        [*prefix, "-C", "attach-session", "-t", SESSION],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        env=environment(),
    )


def markers(work):
    return ",".join(sorted(name for name in os.listdir(work) if name.startswith("m")))


def clear(work):
    for name in list(os.listdir(work)):
        if name.startswith("m"):
            os.remove(os.path.join(work, name))


def reap(client, deadline=20.0):
    limit = time.time() + deadline
    while time.time() < limit:
        if client.poll() is not None:
            code = client.returncode
            return "exit%d" % code if code >= 0 else "signal%d" % -code
        time.sleep(0.05)
    client.kill()
    client.wait()
    return "stalled"


def marker(work, number):
    return os.path.join(work, "m%d" % number)


def main():
    sys.stdout.reconfigure(line_buffering=True)
    work = sys.argv[1]
    prefix = sys.argv[2:]
    os.makedirs(work, exist_ok=True)

    clear(work)
    client = control(prefix)
    client.stdin.write("run-shell 'sleep 2; touch %s'\n" % marker(work, 1))
    client.stdin.flush()
    time.sleep(0.6)
    client.stdin.write("run-shell 'touch %s'\n" % marker(work, 2))
    client.stdin.write("run-shell 'touch %s'\n" % marker(work, 3))
    client.stdin.flush()
    time.sleep(0.3)
    client.send_signal(signal.SIGKILL)
    status = reap(client)
    time.sleep(3.0)
    print("hard-direct status=%s ran=[%s]" % (status, markers(work)))

    queue = os.path.join(work, "queue.conf")
    with open(queue, "w") as handle:
        handle.write("run-shell 'sleep 2; touch %s'\n" % marker(work, 1))
        handle.write("run-shell 'touch %s'\n" % marker(work, 2))
        handle.write("run-shell 'touch %s'\n" % marker(work, 3))
    clear(work)
    client = control(prefix)
    client.stdin.write("source-file %s\n" % queue)
    client.stdin.flush()
    time.sleep(0.8)
    client.send_signal(signal.SIGKILL)
    status = reap(client)
    replacement = control(prefix)
    time.sleep(3.0)
    print("hard-source status=%s ran=[%s]" % (status, markers(work)))
    replacement.stdin.close()
    reap(replacement)
    seen = [
        line
        for line in replacement.stdout.read().splitlines()
        if not line.startswith(("%begin", "%end", "%session-changed", "%exit"))
    ]
    print("replacement extra-lines=%d" % len(seen))

    clear(work)
    client = control(prefix)
    client.stdin.write("source-file %s\n" % queue)
    client.stdin.flush()
    time.sleep(4.0)
    print("live-source ran=[%s]" % markers(work))
    client.stdin.close()
    reap(client)


main()
