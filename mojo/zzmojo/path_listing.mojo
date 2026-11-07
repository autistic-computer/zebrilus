from std.collections import Set
from std.memory import ArcPointer
from std.sys import size_of
from .path_protocol import PathEntry, PathMark, PathRoot, PathMessage, PATH_BEGIN, PATH_CHUNK, PATH_GIT


comptime PATH_RETAINED_BYTES: Int = 16 * 1024 * 1024


@fieldwise_init
struct PathChunk(Movable):
    var entries: List[PathEntry]


def _text_cost(value: String) -> Int:
    return 2 * value.byte_length() + 32


def _root_cost(value: PathRoot) -> Int:
    return size_of[PathRoot]() + _text_cost(value.root) + _text_cost(value.display_root) + (_text_cost(value.cwd.value()) if value.cwd else 0)


struct PathMarks(Movable):
    var exact: Dict[String, UInt8]
    var directories: List[PathMark]

    def __init__(out self):
        self.exact = Dict[String, UInt8]()
        self.directories = List[PathMark]()

    def additional(self, marks: Span[PathMark, _]) -> Int:
        var seen = Set[String]()
        var bytes = 0
        for mark in marks:
            if mark.rel.endswith("/"):
                bytes += 2 * size_of[PathMark]() + _text_cost(mark.rel)
            elif mark.rel not in self.exact and mark.rel not in seen:
                seen.add(mark.rel)
                bytes += 128 + _text_cost(mark.rel)
        return bytes

    def extend(mut self, marks: Span[PathMark, _]):
        for mark in marks:
            if mark.rel.endswith("/"):
                self.directories.append(mark.copy())
            else:
                self.exact[mark.rel] = mark.kind

    def get(self, rel: String) -> Optional[UInt8]:
        if rel in self.exact:
            return self.exact.get(rel, 0)
        for mark in self.directories:
            if rel.startswith(mark.rel) or rel == mark.rel.rstrip("/"):
                return mark.kind
        return None


struct PathListing(Movable):
    var host: String
    var dir: String
    var request: Optional[UInt64]
    var root: Optional[PathRoot]
    var chunks: List[ArcPointer[PathChunk]]
    var marks: PathMarks
    var error: Optional[String]
    var done: Bool
    var truncated: Bool
    var retained_bytes: Int

    def __init__(out self, host: String = "", dir: String = "", request: Optional[UInt64] = None):
        self.retained_bytes = size_of[Self]() + _text_cost(host) + _text_cost(dir)
        self.host = host.copy()
        self.dir = dir.copy()
        self.request = request
        self.root = None
        self.chunks = List[ArcPointer[PathChunk]]()
        self.marks = PathMarks()
        self.error = None
        self.done = False
        self.truncated = False

    def covers(self, host: String, dir: String) -> Bool:
        return self.host == host and dir.startswith(self.dir) and (dir == self.dir or (self.done and not self.truncated and not self.error))

    def is_empty(self) -> Bool:
        for chunk in self.chunks:
            if len(chunk[].entries) > 0:
                return False
        return True

    def additional(self, message: PathMessage) -> Int:
        if message.kind == PATH_BEGIN:
            if message.error:
                return _text_cost(message.error.value()) - (_text_cost(self.error.value()) if self.error else 0)
            if message.root:
                return _root_cost(message.root.value()) - (_root_cost(self.root.value()) if self.root else 0)
        elif message.kind == PATH_CHUNK and len(message.entries) > 0:
            var bytes = 128 + 2 * size_of[PathEntry]() * len(message.entries)
            for entry in message.entries:
                bytes += _text_cost(entry.rel)
            return bytes
        elif message.kind == PATH_GIT:
            return self.marks.additional(Span(message.marks))
        return 0

    def apply(mut self, var message: PathMessage, budget: Int = PATH_RETAINED_BYTES) raises -> Bool:
        var additional = self.additional(message)
        if additional > budget - self.retained_bytes:
            raise Error("path listing exceeds retained byte budget")
        var changed: Bool
        if message.kind == PATH_BEGIN:
            if message.error:
                self.error = message.error.take()
                self.done = True
            elif message.root:
                self.root = message.root.take()
            else:
                raise Error("path listing begin has no result")
            changed = True
        elif message.kind == PATH_CHUNK:
            var added = len(message.entries) > 0
            if added:
                var entries = List[PathEntry]()
                swap(entries, message.entries)
                self.chunks.append(ArcPointer(PathChunk(entries^)))
            changed = added or (message.done and not self.done)
            self.done = self.done or message.done
            self.truncated = self.truncated or message.truncated
        elif message.kind == PATH_GIT:
            self.marks.extend(Span(message.marks))
            changed = True
        else:
            raise Error("message does not update a path listing")
        self.retained_bytes += additional
        return changed

    def fail_budget(mut self):
        var host = self.host.copy()
        var dir = self.dir.copy()
        var request = self.request
        self = Self(host^, dir^, request)
        self.error = String("Path listing exceeds the 16 MiB retained limit")
        self.retained_bytes += _text_cost(self.error.value())
        self.done = True


def path_mark_letter(kind: UInt8) raises -> String:
    if kind > 3:
        raise Error("unknown path git mark")
    return String("MAUC"[byte=Int(kind)])
