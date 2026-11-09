from std.sys.info import CompilationTarget

comptime MAX_EDITOR_FILE_BYTES = 8 * 1024 * 1024
comptime MAX_EDITOR_PATH_BYTES = 16384
comptime EDITOR_OPEN_HINT = "⌘O to open a file" if CompilationTarget.is_macos() else "Ctrl+O to open a file"
comptime EDITOR_PICKER_PLACEHOLDER = "Open a file in the editor"
comptime EDITOR_NEW_FILE_NAME = "untitled"
comptime EDITOR_RETRY_LABEL = "Try again"
comptime EDITOR_OPEN_FAILED = "Couldn’t open this file"
comptime EDITOR_SAVE_FAILED = "Couldn’t save this file"

comptime EDITOR_SET_BUFFER: UInt8 = 0
comptime EDITOR_SET_LANGUAGE: UInt8 = 1
comptime EDITOR_LOAD: UInt8 = 2
comptime EDITOR_WRITE: UInt8 = 3
comptime EDITOR_PROMPT_NEW_PATH: UInt8 = 4
comptime EDITOR_PUBLISH_PATH: UInt8 = 5
comptime EDITOR_PUBLISH_TITLE: UInt8 = 6

comptime EDITOR_RETRY_OPEN: UInt8 = 0
comptime EDITOR_RETRY_SAVE: UInt8 = 1

comptime EDITOR_SYNC_IGNORE: UInt8 = 0
comptime EDITOR_SYNC_CLEAR_PENDING: UInt8 = 1
comptime EDITOR_SYNC_RESTORE: UInt8 = 2


struct PathParts(Movable):
    var rooted: Bool
    var names: List[String]

    def __init__(out self, path: String):
        var bytes = path.as_bytes()
        var count = len(bytes)
        self.rooted = count > 0 and bytes[0] == 47
        self.names = List[String]()
        var start = 0
        for index in range(count + 1):
            if index < count and bytes[index] != 47:
                continue
            if index > start:
                var dot = index - start == 1 and bytes[start] == 46
                if not dot or start == 0:
                    self.names.append(String(unsafe_from_utf8=bytes[start:index]))
            start = index + 1

    def same(self, other: Self) -> Bool:
        if self.rooted != other.rooted or len(self.names) != len(other.names):
            return False
        for index in range(len(self.names)):
            if self.names[index] != other.names[index]:
                return False
        return True

    def file_name(self) -> Optional[String]:
        if len(self.names) == 0:
            return None
        var last = self.names[len(self.names) - 1]
        if last == ".." or last == ".":
            return None
        return last


def same_path(left: String, right: String) -> Bool:
    return PathParts(left).same(PathParts(right))


def path_file_name(path: String) -> Optional[String]:
    return PathParts(path).file_name()


def _trimmed(bytes: Span[UInt8, _], limit: Int, root: Int) -> Int:
    var end = limit
    while end > root:
        if bytes[end - 1] == 47:
            end -= 1
            continue
        var start = end
        while start > root and bytes[start - 1] != 47:
            start -= 1
        if end - start == 1 and bytes[start] == 46 and start != 0:
            end = start
            continue
        break
    return end


def path_parent(path: String) -> Optional[String]:
    var bytes = path.as_bytes()
    var root = 1 if len(bytes) > 0 and bytes[0] == 47 else 0
    var end = _trimmed(bytes, len(bytes), root)
    if end <= root:
        return None
    var start = end
    while start > root and bytes[start - 1] != 47:
        start -= 1
    return String(unsafe_from_utf8=bytes[0 : _trimmed(bytes, start, root)])


def path_extension(path: String) -> Optional[String]:
    var name = path_file_name(path)
    if not name:
        return None
    var bytes = name.value().as_bytes()
    for index in range(len(bytes) - 1, -1, -1):
        if bytes[index] == 46:
            if index == 0:
                return None
            return String(unsafe_from_utf8=bytes[index + 1 : len(bytes)])
    return None


def editor_language(path: String) -> String:
    var extension = path_extension(path)
    if not extension:
        return "text"
    var lowered = List[UInt8](extension.value().as_bytes())
    for index in range(len(lowered)):
        if lowered[index] >= 65 and lowered[index] <= 90:
            lowered[index] += 32
    var name = String(unsafe_from_utf8=Span(lowered))
    if name == "rs":
        return "rust"
    if name == "md" or name == "markdown" or name == "mdx":
        return "markdown"
    if name == "json" or name == "jsonc":
        return "json"
    if name == "toml":
        return "toml"
    return "text"


def editor_title(path: Optional[String], dirty: Bool) -> String:
    var title = String("editor")
    if path:
        var name = path_file_name(path.value())
        if name and name.value().byte_length() > 0:
            title = name.value()
    if dirty:
        title += " •"
    return title


def _field_error(field: String, value: String) -> Optional[String]:
    var bytes = value.as_bytes()
    if len(bytes) > MAX_EDITOR_PATH_BYTES:
        return "editor " + field + " cannot exceed " + String(MAX_EDITOR_PATH_BYTES) + " bytes"
    for index in range(len(bytes)):
        var byte = bytes[index]
        var control = byte < 32 or byte == 127
        if byte == 0xC2 and index + 1 < len(bytes):
            control = bytes[index + 1] >= 0x80 and bytes[index + 1] <= 0x9F
        if control:
            return "editor " + field + " cannot contain control characters"
    if len(bytes) == 0 or bytes[0] != 47:
        return "editor " + field + " must be absolute"
    return None


def descriptor_path_error(path: String, cwd: String) -> Optional[String]:
    var refused = _field_error("working directory", cwd)
    if refused:
        return refused
    return _field_error("path", path)


def descriptor_sync_action(
    pending: Optional[String],
    snapshot: Optional[String],
    local: Optional[String],
) -> UInt8:
    if pending:
        if snapshot and snapshot.value() == pending.value():
            return EDITOR_SYNC_CLEAR_PENDING
        return EDITOR_SYNC_IGNORE
    if Bool(snapshot) != Bool(local):
        return EDITOR_SYNC_RESTORE
    if not snapshot or same_path(snapshot.value(), local.value()):
        return EDITOR_SYNC_IGNORE
    return EDITOR_SYNC_RESTORE


struct EditorEffect(Copyable, Movable):
    var kind: UInt8
    var text: String
    var detail: String

    def __init__(out self, kind: UInt8, text: String, detail: String = String()):
        self.kind = kind
        self.text = text
        self.detail = detail


struct EditorError(Copyable, Movable):
    var message: String
    var retry: UInt8
    var path: String

    def __init__(out self, message: String, retry: UInt8, path: String):
        self.message = message
        self.retry = retry
        self.path = path

    def title(self) -> String:
        return EDITOR_OPEN_FAILED if self.retry == EDITOR_RETRY_OPEN else EDITOR_SAVE_FAILED


struct EditorModel(Movable):
    var cwd: String
    var path: Optional[String]
    var snapshot_path: Optional[String]
    var pending_path: Optional[String]
    var saved_contents: String
    var dirty: Bool
    var title: String
    var error: Optional[EditorError]
    var picker_open: Bool

    def __init__(out self, path: Optional[String], cwd: String):
        self.cwd = cwd
        self.path = None
        self.snapshot_path = path.copy()
        self.pending_path = None
        self.saved_contents = String()
        self.dirty = False
        self.title = String()
        self.error = None
        self.picker_open = False

    @staticmethod
    def open(path: Optional[String], cwd: String, mut effects: List[EditorEffect]) -> Self:
        var model = Self(path, cwd)
        model._restore(path, cwd, effects)
        return model^

    def scratch_hint(self, buffer_empty: Bool) -> Bool:
        return not self.path and buffer_empty and not self.error and not self.picker_open

    def synchronize(mut self, path: Optional[String], cwd: String) -> List[EditorEffect]:
        var effects = List[EditorEffect]()
        self.cwd = cwd
        self.snapshot_path = path.copy()
        var action = descriptor_sync_action(self.pending_path, path, self.path)
        if action == EDITOR_SYNC_CLEAR_PENDING:
            self.pending_path = None
        elif action == EDITOR_SYNC_RESTORE:
            self._restore(path, cwd, effects)
        return effects^

    def _restore(mut self, path: Optional[String], cwd: String, mut effects: List[EditorEffect]):
        self.cwd = cwd
        if not path:
            self.path = None
            self.saved_contents = String()
            self.dirty = False
            self.error = None
            effects.append(EditorEffect(EDITOR_SET_BUFFER, String(), "text"))
            self._publish_title(effects)
            return
        self.path = path.copy()
        effects.append(EditorEffect(EDITOR_SET_LANGUAGE, editor_language(path.value())))
        self._publish_title(effects)
        effects.append(EditorEffect(EDITOR_LOAD, path.value()))

    def buffer_changed(mut self, contents: String) -> List[EditorEffect]:
        var effects = List[EditorEffect]()
        var dirty = contents != self.saved_contents
        if self.dirty != dirty:
            self.dirty = dirty
            self._publish_title(effects)
        return effects^

    def open_picker(mut self) -> Bool:
        if self.picker_open:
            return False
        self.error = None
        self.picker_open = True
        return True

    def picker_closed(mut self, selected: Optional[String]) -> List[EditorEffect]:
        var effects = List[EditorEffect]()
        self.picker_open = False
        if selected:
            effects.append(EditorEffect(EDITOR_LOAD, selected.value()))
        return effects^

    def load_failed(mut self, path: String, message: String):
        self.error = EditorError(message, EDITOR_RETRY_OPEN, path)

    def finish_load(mut self, path: String, contents: String) -> List[EditorEffect]:
        var effects = List[EditorEffect]()
        if descriptor_path_error(path, self.cwd):
            self.error = EditorError(
                path + " cannot be stored as an editor path", EDITOR_RETRY_OPEN, path
            )
            return effects^
        effects.append(EditorEffect(EDITOR_SET_BUFFER, contents, editor_language(path)))
        self.path = path
        self.saved_contents = contents
        self.dirty = False
        self.error = None
        self._publish_path(path, effects)
        self._publish_title(effects)
        return effects^

    def save(mut self, contents: String) -> List[EditorEffect]:
        if self.path:
            var current = self.path.value()
            return self.begin_save(current, contents)
        var effects = List[EditorEffect]()
        effects.append(EditorEffect(EDITOR_PROMPT_NEW_PATH, self.cwd, EDITOR_NEW_FILE_NAME))
        return effects^

    def begin_save(mut self, path: String, contents: String) -> List[EditorEffect]:
        var effects = List[EditorEffect]()
        var refused = descriptor_path_error(path, self.cwd)
        if refused:
            self.error = EditorError(refused.value(), EDITOR_RETRY_SAVE, path)
            return effects^
        self.error = None
        effects.append(EditorEffect(EDITOR_WRITE, path, contents))
        return effects^

    def save_failed(mut self, path: String, message: String):
        self.error = EditorError(message, EDITOR_RETRY_SAVE, path)

    def finish_save(mut self, path: String, saved: String, contents: String) -> List[EditorEffect]:
        var effects = List[EditorEffect]()
        var refused = descriptor_path_error(path, self.cwd)
        if refused:
            self.error = EditorError(refused.value(), EDITOR_RETRY_SAVE, path)
            return effects^
        self.path = path
        self.saved_contents = saved
        self.dirty = contents != self.saved_contents
        self.error = None
        self._publish_path(path, effects)
        self._publish_title(effects)
        return effects^

    def retry(mut self, kind: UInt8, path: String, contents: String) -> List[EditorEffect]:
        if kind == EDITOR_RETRY_SAVE:
            return self.begin_save(path, contents)
        var effects = List[EditorEffect]()
        effects.append(EditorEffect(EDITOR_LOAD, path))
        return effects^

    def _publish_path(mut self, path: String, mut effects: List[EditorEffect]):
        if self.snapshot_path and self.snapshot_path.value() == path:
            self.pending_path = None
            return
        self.pending_path = path
        effects.append(EditorEffect(EDITOR_PUBLISH_PATH, path))

    def _publish_title(mut self, mut effects: List[EditorEffect]):
        var title = editor_title(self.path, self.dirty)
        if title == self.title:
            return
        self.title = title
        effects.append(EditorEffect(EDITOR_PUBLISH_TITLE, title))
