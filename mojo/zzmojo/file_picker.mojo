from std.ffi import OwnedDLHandle
from std.sys.info import CompilationTarget
from std.time import perf_counter_ns
from .editor_files import FileError, file_facts
from .editor_state import path_file_name, same_path
from .path_case import PathCase
from .path_pattern import MAX_PATH_LABEL, MAX_PATH_QUERY, PathPattern
from .path_rank import PathLabels, PathRankItem, path_sort
from .path_score import PathScorer

comptime MAX_PICKER_ROWS = 500
comptime PICKER_WORKSPACE_PRIOR: UInt32 = 1024
comptime PICKER_SCAN_LIMIT = 50000
comptime PICKER_SCAN_DEPTH = 8
comptime PICKER_SCAN_BUDGET_NS: UInt64 = 5000000000
comptime PICKER_SCANNING_NOTE = "Scanning…"
comptime PICKER_LIMITED_NOTE = "Search limited. Enter a full path to open another folder."
comptime PICKER_NO_MATCH = "Nothing matches that search."
comptime PICKER_NO_FILES = "No files under this folder."
comptime PICKER_NO_FOLDERS = "No folders under this folder."
comptime PICKER_ACCEPT_TYPED: UInt8 = 0
comptime PICKER_ACCEPT_ROW: UInt8 = 1
comptime PICKER_ACCEPT_NOTHING: UInt8 = 2
comptime _DirPointer = Pointer[UInt8, MutUntrackedOrigin]
comptime _NAME_OFFSET = 19 if CompilationTarget.is_linux() else 21
comptime _KIND_OFFSET = 18 if CompilationTarget.is_linux() else 20
comptime _MAX_NAME_BYTES = 255


def entry_prior(depth: UInt64, workspace: Bool) -> UInt32:
    var shallowness: UInt32 = 0
    if depth < 64:
        shallowness = UInt32(64 - depth)
    if workspace:
        return shallowness + PICKER_WORKSPACE_PRIOR
    return shallowness


def preserved_selection(
    entries: List[String], preferred: Optional[String]
) -> Optional[Int]:
    if preferred:
        for index in range(len(entries)):
            if same_path(entries[index], preferred.value()):
                return Optional[Int](index)
    if len(entries) > 0:
        return Optional[Int](0)
    return None


def _space_width(bytes: Span[UInt8, _], index: Int) -> Int:
    var first = bytes[index]
    if (first >= 9 and first <= 13) or first == 32:
        return 1
    if index + 1 < len(bytes) and first == 0xC2:
        if bytes[index + 1] == 0x85 or bytes[index + 1] == 0xA0:
            return 2
    if index + 2 >= len(bytes):
        return 0
    var second = bytes[index + 1]
    var third = bytes[index + 2]
    if first == 0xE1 and second == 0x9A and third == 0x80:
        return 3
    if first == 0xE2 and second == 0x80:
        if third >= 0x80 and third <= 0x8A:
            return 3
        if third == 0xA8 or third == 0xA9 or third == 0xAF:
            return 3
    if first == 0xE2 and second == 0x81 and third == 0x9F:
        return 3
    if first == 0xE3 and second == 0x80 and third == 0x80:
        return 3
    return 0


def picker_trim(text: String) -> String:
    var bytes = text.as_bytes()
    var start = 0
    while start < len(bytes):
        var width = _space_width(bytes, start)
        if width == 0:
            break
        start += width
    var end = len(bytes)
    while end > start:
        var head = end - 1
        while head > start and bytes[head] & 0xC0 == 0x80:
            head -= 1
        if _space_width(bytes, head) != end - head:
            break
        end = head
    return String(unsafe_from_utf8=bytes[start:end])


def directory_query(query: String) -> String:
    var trimmed = picker_trim(query)
    var bytes = trimmed.as_bytes()
    var end = len(bytes)
    while end > 0 and (bytes[end - 1] == 47 or bytes[end - 1] == 92):
        end -= 1
    return String(unsafe_from_utf8=bytes[0:end])


def validate_selection(path: String, directories: Bool) raises -> Optional[String]:
    for byte in path.as_bytes():
        if byte == 0:
            return "Cannot open " + path + ": file name contained an unexpected NUL byte"
    var facts = file_facts(path)
    if not facts.found():
        return "Cannot open " + path + ": " + FileError.os(facts.number).text
    if directories:
        if facts.directory():
            return None
        return path + " is not a folder."
    if facts.regular():
        return None
    return path + " is not a file."


struct AcceptTarget(Copyable, Movable):
    var kind: UInt8
    var path: String
    var row: Int

    def __init__(out self, kind: UInt8, path: String = String(), row: Int = 0):
        self.kind = kind
        self.path = path
        self.row = row


struct PickerFlow(Copyable, Movable):
    var query: String
    var selected: Optional[Int]
    var scanning: Bool
    var limited: Bool
    var error: Optional[String]
    var rank_generation: UInt64
    var accept_generation: UInt64

    def __init__(out self):
        self.query = String()
        self.selected = None
        self.scanning = True
        self.limited = False
        self.error = None
        self.rank_generation = 0
        self.accept_generation = 0

    def shows_first_batch(self, listed: Bool) -> Bool:
        return not listed and picker_trim(self.query).byte_length() == 0

    def scan_finished(mut self, limited: Bool):
        self.scanning = False
        self.limited = limited

    def scan_failed(mut self, error: String):
        self.scanning = False
        self.error = error

    def query_changed(mut self, query: String, indexed: Bool) -> Bool:
        if query == self.query:
            return False
        self.query = query
        if self.scanning or indexed:
            self.error = None
        if self.accept_generation != 0xFFFFFFFFFFFFFFFF:
            self.accept_generation += 1
        return True

    def begin_rank(mut self, clear: Bool) -> UInt64:
        if clear:
            self.selected = None
        if self.rank_generation != 0xFFFFFFFFFFFFFFFF:
            self.rank_generation += 1
        return self.rank_generation

    def begin_accept(mut self) -> UInt64:
        if self.accept_generation != 0xFFFFFFFFFFFFFFFF:
            self.accept_generation += 1
        return self.accept_generation

    def accept_target(self) -> AcceptTarget:
        var typed = picker_trim(self.query)
        if typed.startswith("/"):
            return AcceptTarget(PICKER_ACCEPT_TYPED, typed)
        if self.selected:
            return AcceptTarget(PICKER_ACCEPT_ROW, String(), self.selected.value())
        return AcceptTarget(PICKER_ACCEPT_NOTHING)

    def navigate(mut self, count: Int, direction: Int) -> Optional[Int]:
        if count == 0:
            return None
        var current = 0
        if self.selected:
            current = self.selected.value()
        var selected = (current + 1) % count
        if direction < 0:
            selected = current - 1 if current > 0 else count - 1
        self.selected = Optional[Int](selected)
        return Optional[Int](selected)

    def hover(mut self, index: Int) -> Bool:
        if self.selected and self.selected.value() == index:
            return False
        self.selected = Optional[Int](index)
        return True

    def scanning_note(self, listed: Bool) -> Bool:
        return listed and self.scanning

    def empty_message(self, input_empty: Bool, directories: Bool) -> String:
        if self.scanning:
            return PICKER_SCANNING_NOTE
        if self.error:
            return self.error.value()
        if not input_empty:
            return PICKER_NO_MATCH
        return PICKER_NO_FOLDERS if directories else PICKER_NO_FILES


struct PickerEntry(Copyable, Movable):
    var relative: String
    var absolute: String
    var prior: UInt32

    def __init__(out self, relative: String, absolute: String, prior: UInt32):
        self.relative = relative
        self.absolute = absolute
        self.prior = prior


struct PickerLabels(Movable, PathLabels):
    var entries: List[PickerEntry]

    def __init__(out self):
        self.entries = List[PickerEntry]()

    def count(self) -> Int:
        return len(self.entries)

    def label(self, index: Int) -> String:
        return self.entries[index].relative

    def changed(self, index: Int) -> Bool:
        return False


struct PickerRanker(Movable):
    var characters: PathCase
    var scorer: PathScorer

    def __init__(out self) raises:
        self.characters = PathCase()
        self.scorer = PathScorer()

    def rank(mut self, query: String, source: PickerLabels) raises -> List[Int]:
        var needle = directory_query(query)
        var ranked = List[PathRankItem](capacity=source.count())
        if needle.byte_length() == 0:
            for index in range(source.count()):
                ranked.append(PathRankItem(index, 0, source.entries[index].prior))
        elif needle.byte_length() <= MAX_PATH_QUERY:
            var pattern = PathPattern(needle, self.characters)
            for index in range(source.count()):
                var window = pattern.filter(source.entries[index].relative)
                if window.accepted:
                    var matched = self.scorer.score(
                        pattern, source.entries[index].relative, window
                    )
                    ranked.append(
                        PathRankItem(index, matched.score, source.entries[index].prior)
                    )
        path_sort(source, ranked)
        var rows = List[Int](capacity=min(len(ranked), MAX_PICKER_ROWS))
        for item in ranked:
            if len(rows) == MAX_PICKER_ROWS:
                break
            rows.append(item.index)
        return rows^


def picker_walk_enters(relative: String, top: Bool) -> Bool:
    var found = path_file_name(relative)
    var name = String()
    if found:
        name = found.value()
    if name == "node_modules" or name == "target" or name == "__pycache__" or name == "venv":
        return False
    if relative == "go/pkg/mod" or relative.endswith("/go/pkg/mod"):
        return False
    if not top:
        return True
    return not (
        name == "Applications"
        or name == "Library"
        or name == "Movies"
        or name == "Music"
        or name == "Pictures"
        or name == "Public"
        or name == "Templates"
        or name == "Videos"
        or name == "snap"
    )


def _joined(directory: String, name: String) -> String:
    if directory.byte_length() == 0 or directory.endswith("/"):
        return directory + name
    return directory + "/" + name


struct _Pending(Copyable, Movable):
    var absolute: String
    var relative: String
    var depth: Int

    def __init__(out self, absolute: String, relative: String, depth: Int):
        self.absolute = absolute
        self.relative = relative
        self.depth = depth


struct PickerScan(Movable):
    var directories: Bool
    var pending: List[_Pending]
    var head: Int
    var current: _Pending
    var handle: Optional[_DirPointer]
    var visited: Int
    var started: UInt64
    var limited: Bool
    var finished: Bool
    var error: Optional[String]
    var library: OwnedDLHandle

    def __init__(out self, root: String, directories: Bool) raises:
        self.directories = directories
        self.pending = List[_Pending]()
        self.head = 0
        self.current = _Pending(root, String(), 0)
        self.handle = None
        self.visited = 0
        self.started = UInt64(perf_counter_ns())
        self.limited = False
        self.finished = False
        self.error = None
        self.library = OwnedDLHandle()
        var failed = self._open(root)
        if failed != 0:
            self.finished = True
            self.error = "Cannot read " + root + ": " + FileError.os(failed).text

    def __deinit__(deinit self):
        if self.handle:
            _ = self.library.call["closedir", Int32](self.handle.value())

    def _open(mut self, path: String) raises -> Int32:
        for byte in path.as_bytes():
            if byte == 0:
                return 22
        var c_path = List[UInt8](path.as_bytes())
        c_path.append(0)
        self.handle = self.library.call["opendir", Optional[_DirPointer]](
            c_path.unsafe_ptr()
        )
        _ = c_path
        if self.handle:
            return 0
        return FileError.last().number

    def _close(mut self):
        if self.handle:
            _ = self.library.call["closedir", Int32](self.handle.value())
            self.handle = None

    def _spent(self) -> Bool:
        return (
            self.visited >= PICKER_SCAN_LIMIT
            or UInt64(perf_counter_ns()) - self.started >= PICKER_SCAN_BUDGET_NS
        )

    def _finish(mut self, limited: Bool):
        self._close()
        self.limited = limited
        self.finished = True

    def advance(
        mut self, slice_ns: UInt64, mut found: List[PickerEntry], entries: Int = 4096
    ) raises -> Bool:
        if self.finished:
            return True
        var deadline = UInt64(perf_counter_ns()) + slice_ns
        var steps = 0
        while True:
            if steps >= entries:
                return False
            if steps % 32 == 31 and UInt64(perf_counter_ns()) >= deadline:
                return False
            steps += 1
            if not self.handle:
                if self.head >= len(self.pending):
                    self._finish(False)
                    return True
                if self._spent():
                    self._finish(True)
                    return True
                self.current = self.pending[self.head].copy()
                self.head += 1
                var opening = self.current.absolute
                if self._open(opening) != 0:
                    continue
            if self._spent():
                self._finish(True)
                return True
            var entry = self.library.call["readdir", Optional[_DirPointer]](
                self.handle.value()
            )
            if not entry:
                self._close()
                continue
            var record = entry.value()
            var name_bytes = List[UInt8]()
            for index in range(_MAX_NAME_BYTES):
                var byte = record[unsafe_offset=_NAME_OFFSET + index]
                if byte == 0:
                    break
                name_bytes.append(byte)
            if len(name_bytes) == 0 or name_bytes[0] == 46:
                continue
            self.visited += 1
            var name: String
            try:
                name = String(from_utf8=Span(name_bytes))
            except:
                continue
            var absolute = _joined(self.current.absolute, name)
            var relative = _joined(self.current.relative, name)
            var kind = record[unsafe_offset=_KIND_OFFSET]
            var directory = kind == 4
            var regular = kind == 8
            if kind == 0:
                var facts = file_facts(absolute, False)
                directory = facts.directory()
                regular = facts.regular()
            if relative.byte_length() > MAX_PATH_LABEL:
                continue
            if not directory:
                if regular and not self.directories:
                    found.append(PickerEntry(relative, absolute, 0))
                continue
            if not picker_walk_enters(relative, False):
                continue
            if self.directories:
                found.append(
                    PickerEntry(
                        relative,
                        absolute,
                        entry_prior(
                            UInt64(self.current.depth + 1),
                            file_facts(_joined(absolute, ".git")).found(),
                        ),
                    )
                )
            if self.current.depth + 1 < PICKER_SCAN_DEPTH and picker_walk_enters(
                relative, self.current.depth == 0
            ):
                self.pending.append(_Pending(absolute, relative, self.current.depth + 1))


struct FilePicker(Movable):
    var directories: Bool
    var root: String
    var flow: PickerFlow
    var scan: PickerScan
    var index: PickerLabels
    var indexed: Bool
    var rows: List[PickerEntry]
    var ranker: PickerRanker
    var input_empty: Bool

    def __init__(out self, root: String, directories: Bool) raises:
        self.directories = directories
        self.root = root
        self.flow = PickerFlow()
        self.scan = PickerScan(root, directories)
        self.index = PickerLabels()
        self.indexed = False
        self.rows = List[PickerEntry]()
        self.ranker = PickerRanker()
        self.input_empty = True
        self._settle()

    def _settle(mut self):
        if not self.scan.finished or not self.flow.scanning:
            return
        if self.scan.error:
            self.flow.scan_failed(self.scan.error.value())
        else:
            self.flow.scan_finished(self.scan.limited)

    def scanning(self) -> Bool:
        return self.flow.scanning

    def advance(mut self, slice_ns: UInt64, entries: Int = 4096) raises -> Bool:
        if not self.flow.scanning:
            return False
        var found = List[PickerEntry]()
        _ = self.scan.advance(slice_ns, found, entries)
        var listed = not self.directories and self.scan.finished and not self.scan.error
        if len(found) > 0 or (listed and not self.indexed):
            for entry in found:
                self.index.entries.append(entry.copy())
            self.indexed = True
            self._search(False)
        self._settle()
        return True

    def selected_path(self) -> Optional[String]:
        if self.flow.selected and self.flow.selected.value() < len(self.rows):
            return Optional[String](self.rows[self.flow.selected.value()].absolute)
        return None

    def set_query(mut self, query: String) raises:
        self.input_empty = query.byte_length() == 0
        if self.flow.query_changed(query, self.indexed):
            self._search(True)

    def _search(mut self, clear: Bool) raises:
        if not self.indexed:
            return
        var preferred = self.selected_path()
        _ = self.flow.begin_rank(clear)
        var ranked = self.ranker.rank(self.flow.query, self.index)
        var rows = List[PickerEntry](capacity=len(ranked))
        var paths = List[String](capacity=len(ranked))
        for index in ranked:
            rows.append(self.index.entries[index].copy())
            paths.append(self.index.entries[index].absolute)
        self.rows = rows^
        self.flow.selected = preserved_selection(paths, preferred)

    def navigate(mut self, direction: Int) -> Optional[Int]:
        return self.flow.navigate(len(self.rows), direction)

    def hover(mut self, index: Int) -> Bool:
        if index < 0 or index >= len(self.rows):
            return False
        return self.flow.hover(index)

    def accept_path(mut self, path: String) raises -> Optional[String]:
        _ = self.flow.begin_accept()
        var refused = validate_selection(path, self.directories)
        if refused:
            self.flow.error = refused
            return None
        return Optional[String](path)

    def accept(mut self, index: Int) raises -> Optional[String]:
        if index < 0 or index >= len(self.rows):
            return None
        var path = self.rows[index].absolute
        return self.accept_path(path)

    def accept_selected(mut self) raises -> Optional[String]:
        var target = self.flow.accept_target()
        if target.kind == PICKER_ACCEPT_TYPED:
            return self.accept_path(target.path)
        if target.kind == PICKER_ACCEPT_ROW:
            return self.accept(target.row)
        return None

    def empty_message(self) -> String:
        return self.flow.empty_message(self.input_empty, self.directories)
