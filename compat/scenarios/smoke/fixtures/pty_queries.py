import os

WINDOW_PIXELS_QUERY = b"\x1b[14t"
WINDOW_PIXELS_ANSWER = b"\x1b[4;0;0t"


class Answerer:
    def __init__(self, master):
        self.master = master
        self.tail = b""

    def feed(self, data):
        seen = self.tail + data
        for _ in range(seen.count(WINDOW_PIXELS_QUERY)):
            os.write(self.master, WINDOW_PIXELS_ANSWER)
        self.tail = seen[-(len(WINDOW_PIXELS_QUERY) - 1):]
