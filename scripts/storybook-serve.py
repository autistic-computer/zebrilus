#!/usr/bin/env python3
import functools
import http.server
import sys


class NoStore(http.server.SimpleHTTPRequestHandler):
    mounts = {}

    def translate_path(self, path):
        head, _, rest = path.lstrip("/").partition("/")
        if head in self.mounts:
            self.directory = self.mounts[head]
            path = "/" + rest
        return super().translate_path(path)

    def end_headers(self):
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

    def log_message(self, *args):
        pass


directory, port = sys.argv[1], int(sys.argv[2])
NoStore.mounts = dict(mount.split("=", 1) for mount in sys.argv[3:])
handler = functools.partial(NoStore, directory=directory)
http.server.ThreadingHTTPServer(("127.0.0.1", port), handler).serve_forever()
