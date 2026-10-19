import os
import random
import select
import sys
import tty

LETTERS = b"abcdefghijklmnopqrstuvwxyz"
NOISE = b"0123456789#$%&*+=-:;<>"


def token(key):
    return bytes(LETTERS[(key - 97 + 5 * i) % 26] for i in range(6))


tty.setraw(0)
cols, rows = os.get_terminal_size(0)
rng = random.Random(1)
if sys.argv[1:] == ["color"]:
    pool = b"".join(b"\x1b[38;5;%03dm%c" % (rng.randrange(256), rng.choice(NOISE)) for _ in range(1 << 15))
    cell = len(pool) // (1 << 15)
else:
    pool = bytes(rng.choice(NOISE) for _ in range(1 << 16))
    cell = 1
last = 97
offset = 0


def status():
    return b"\x1b[%d;1H" % rows + token(last) + b"\x1b[K"


os.write(1, b"\x1b[2J" + status())
while True:
    ready, _, _ = select.select([0], [], [], 0)
    if ready:
        data = os.read(0, 256)
        if not data:
            break
        last = data[-1]
        os.write(1, status())
    frame = bytearray(b"\x1b[H")
    for row in range(rows - 1):
        start = (offset + row * 97) % (len(pool) // cell - cols) * cell
        frame += pool[start:start + cols * cell]
        frame += b"\r\n"
    offset = (offset + 131) % len(pool)
    frame += b"\x1b[0m" + status()
    os.write(1, frame)
