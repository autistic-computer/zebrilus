from std.ffi import external_call, OwnedDLHandle
from std.sys.info import CompilationTarget
from .client_files import errno_text
from .editor_state import (
    MAX_EDITOR_FILE_BYTES,
    path_file_name,
    path_parent,
    same_path,
)

comptime EDITOR_READ_CHUNK_BYTES = 64 * 1024
comptime MAX_EDITOR_REAL_PATH_BYTES = 64 * 1024
comptime EDITOR_TEMP_ATTEMPTS = 128
comptime _BytePointer = Pointer[UInt8, MutUntrackedOrigin]
comptime _ErrnoPointer = Pointer[Int32, MutUntrackedOrigin]
comptime _ERRNO: StaticString = "__errno_location" if CompilationTarget.is_linux() else "__error"
comptime _O_CREAT: Int32 = 0x40 if CompilationTarget.is_linux() else 0x200
comptime _O_EXCL: Int32 = 0x80 if CompilationTarget.is_linux() else 0x800
comptime _O_CLOEXEC: Int32 = 0x80000 if CompilationTarget.is_linux() else 0x1000000
comptime _NUL_IN_PATH = "file name contained an unexpected NUL byte"


struct FileError(Copyable, Movable):
    var number: Int32
    var text: String

    def __init__(out self, number: Int32, text: String):
        self.number = number
        self.text = text

    @staticmethod
    def os(number: Int32) raises -> Self:
        return Self(number, errno_text(number) + " (os error " + String(number) + ")")

    @staticmethod
    def last() raises -> Self:
        return Self.os(_errno())


struct FileFacts(Copyable, Movable):
    var number: Int32
    var mode: UInt32
    var size: UInt64

    def __init__(out self, number: Int32, mode: UInt32 = 0, size: UInt64 = 0):
        self.number = number
        self.mode = mode
        self.size = size

    def found(self) -> Bool:
        return self.number == 0

    def directory(self) -> Bool:
        return self.number == 0 and self.mode & 0xF000 == 0x4000

    def regular(self) -> Bool:
        return self.number == 0 and self.mode & 0xF000 == 0x8000

    def symlink(self) -> Bool:
        return self.number == 0 and self.mode & 0xF000 == 0xA000


struct EditorRead(Movable):
    var ok: Bool
    var contents: String
    var message: String

    def __init__(out self, ok: Bool, var contents: String, var message: String):
        self.ok = ok
        self.contents = contents^
        self.message = message^


def _errno() -> Int32:
    return external_call[_ERRNO, _ErrnoPointer]()[]


def _has_nul(path: String) -> Bool:
    for byte in path.as_bytes():
        if byte == 0:
            return True
    return False


def _c_path(path: String) -> List[UInt8]:
    var bytes = List[UInt8](path.as_bytes())
    bytes.append(0)
    return bytes^


def file_facts(path: String, follow: Bool = True) raises -> FileFacts:
    if _has_nul(path):
        return FileFacts(22)
    var c_path = _c_path(path)
    var storage = List[UInt64](length=32, fill=0)
    var data = storage.unsafe_ptr().unsafe_bitcast[UInt8]()
    var library = OwnedDLHandle()
    var status: Int32
    var mode_offset = 28 if CompilationTarget.is_linux() else 4
    var size_offset = 40 if CompilationTarget.is_linux() else 96
    comptime if CompilationTarget.is_linux():
        var statx = library.get_function[Int32]("statx")
        status = statx(
            Int32(-100),
            c_path.unsafe_ptr(),
            Int32(0 if follow else 0x100),
            UInt32(0x7FF),
            data,
        )
    else:
        if follow:
            var stat = library.get_function[Int32]("stat")
            status = stat(c_path.unsafe_ptr(), data)
        else:
            var lstat = library.get_function[Int32]("lstat")
            status = lstat(c_path.unsafe_ptr(), data)
    _ = c_path
    if status != 0:
        return FileFacts(_errno())
    var mode: UInt32 = 0
    for index in range(2):
        mode |= UInt32(data[unsafe_offset=mode_offset + index]) << UInt32(index * 8)
    var size: UInt64 = 0
    for index in range(8):
        size |= UInt64(data[unsafe_offset=size_offset + index]) << UInt64(index * 8)
    _ = storage
    return FileFacts(0, mode, size)


def real_path(path: String) raises -> Optional[String]:
    if _has_nul(path):
        return None
    var library = OwnedDLHandle()
    var realpath = library.get_function[Optional[_BytePointer]]("realpath")
    var c_path = _c_path(path)
    var pointer = realpath(c_path.unsafe_ptr(), Optional[_BytePointer](None))
    _ = c_path
    if not pointer:
        return None
    var length = external_call["strnlen", Int](pointer.value(), MAX_EDITOR_REAL_PATH_BYTES + 1)
    var resolved: Optional[String] = None
    if length <= MAX_EDITOR_REAL_PATH_BYTES:
        var bytes = List[UInt8](Span(unsafe_ptr=pointer.value(), length=length))
        try:
            resolved = String(from_utf8=Span(bytes))
        except:
            resolved = None
    _ = external_call["free", NoneType](pointer.value())
    return resolved^


def _open(path: String, flags: Int32, mode: Int32 = 0) -> Int32:
    var c_path = _c_path(path)
    var fd = external_call["open", Int32, num_fixed_args=2](c_path.unsafe_ptr(), flags, mode)
    _ = c_path
    return fd


def _too_large(path: String) -> String:
    return path + " is larger than the 8 MiB editor limit"


def read_editor_file(path: String) raises -> EditorRead:
    if _has_nul(path):
        return EditorRead(False, String(), "Could not open " + path + ": " + _NUL_IN_PATH)
    var facts = file_facts(path)
    if not facts.found():
        return EditorRead(
            False, String(), "Could not open " + path + ": " + FileError.os(facts.number).text
        )
    if facts.size > UInt64(MAX_EDITOR_FILE_BYTES):
        return EditorRead(False, String(), _too_large(path))
    var fd = _open(path, _O_CLOEXEC)
    if fd < 0:
        return EditorRead(False, String(), "Could not open " + path + ": " + FileError.last().text)
    var library = OwnedDLHandle()
    var read = library.get_function[Int]("read")
    var chunk = List[UInt8](length=EDITOR_READ_CHUNK_BYTES, fill=0)
    var data = List[UInt8](capacity=Int(facts.size))
    var failed: Int32 = 0
    while len(data) <= MAX_EDITOR_FILE_BYTES:
        var size = min(len(chunk), MAX_EDITOR_FILE_BYTES + 1 - len(data))
        var count = read(fd, chunk.unsafe_ptr(), size)
        if count < 0:
            failed = _errno()
            if failed == 4:
                failed = 0
                continue
            break
        if count == 0:
            break
        data.extend(Span(chunk)[:count])
    _ = chunk
    _ = external_call["close", Int32](fd)
    if failed != 0:
        return EditorRead(False, String(), "Could not read " + path + ": " + FileError.os(failed).text)
    if len(data) > MAX_EDITOR_FILE_BYTES:
        return EditorRead(False, String(), _too_large(path))
    for byte in data:
        if byte == 0:
            return EditorRead(
                False, String(), path + " contains NUL bytes and appears to be binary"
            )
    try:
        return EditorRead(True, String(from_utf8=Span(data)), String())
    except:
        return EditorRead(False, String(), path + " is not valid UTF-8")


def _mkdir(path: String) raises -> Int32:
    var library = OwnedDLHandle()
    var mkdir = library.get_function[Int32]("mkdir")
    var c_path = _c_path(path)
    var status = mkdir(c_path.unsafe_ptr(), UInt32(0o777))
    _ = c_path
    return _errno() if status != 0 else Int32(0)


def create_directories(path: String) raises -> Optional[FileError]:
    if path.byte_length() == 0:
        return None
    var failed = _mkdir(path)
    if failed == 0:
        return None
    if failed != 2:
        if file_facts(path).directory():
            return None
        return FileError.os(failed)
    var parent = path_parent(path)
    if not parent:
        return FileError(0, "failed to create whole tree")
    var above = create_directories(parent.value())
    if above:
        return above^
    failed = _mkdir(path)
    if failed == 0 or file_facts(path).directory():
        return None
    return FileError.os(failed)


def _write_all(fd: Int32, contents: Span[UInt8, _]) raises -> Optional[FileError]:
    var library = OwnedDLHandle()
    var write = library.get_function[Int]("write")
    var offset = 0
    while offset < len(contents):
        var count = write(fd, contents.unsafe_ptr().unsafe_offset(offset), len(contents) - offset)
        if count < 0:
            var failed = _errno()
            if failed == 4:
                continue
            return FileError.os(failed)
        if count == 0:
            return FileError(0, "failed to write whole buffer")
        offset += count
    return None


def _fill(fd: Int32, path: String, contents: Span[UInt8, _]) raises -> Optional[FileError]:
    var library = OwnedDLHandle()
    var facts = file_facts(path)
    if facts.found():
        var fchmod = library.get_function[Int32]("fchmod")
        if fchmod(fd, UInt32(facts.mode & 0o7777)) != 0:
            return FileError.last()
    var failure = _write_all(fd, contents)
    if failure:
        return failure^
    comptime if CompilationTarget.is_macos():
        if external_call["fcntl", Int32, num_fixed_args=2](fd, Int32(51), Int32(0)) != 0:
            return FileError.last()
    else:
        var fsync = library.get_function[Int32]("fsync")
        if fsync(fd) != 0:
            return FileError.last()
    return None


def _unlink(path: String):
    var c_path = _c_path(path)
    _ = external_call["unlink", Int32](c_path.unsafe_ptr())
    _ = c_path


def _replace_file(path: String, contents: Span[UInt8, _]) raises -> Optional[FileError]:
    var parent = path_parent(path)
    if not parent:
        return FileError(0, "configuration path has no parent directory")
    var directory = parent.value()
    var failure = create_directories(directory)
    if failure:
        return failure^
    var name = path_file_name(path)
    var stem = String(".") + (name.value() if name else String("config")) + ".tmp-"
    if directory.byte_length() != 0 and not directory.endswith("/"):
        stem = directory + "/" + stem
    else:
        stem = directory + stem
    stem += String(external_call["getpid", UInt32]()) + "-"
    var temporary = String()
    var fd: Int32 = -1
    for nonce in range(EDITOR_TEMP_ATTEMPTS):
        temporary = stem + String(nonce)
        fd = _open(temporary, Int32(1) | _O_CREAT | _O_EXCL | _O_CLOEXEC, Int32(0o666))
        if fd >= 0:
            break
        var failed = _errno()
        if failed != 17:
            return FileError.os(failed)
    if fd < 0:
        return FileError(0, "could not allocate a unique temporary configuration file")
    failure = _fill(fd, path, contents)
    _ = external_call["close", Int32](fd)
    if failure:
        _unlink(temporary)
        return failure^
    var library = OwnedDLHandle()
    var rename = library.get_function[Int32]("rename")
    var source = _c_path(temporary)
    var target = _c_path(path)
    var status = rename(source.unsafe_ptr(), target.unsafe_ptr())
    _ = source
    _ = target
    if status != 0:
        var renamed = FileError.last()
        _unlink(temporary)
        return renamed^
    return None


def atomic_write(path: String, contents: Span[UInt8, _]) raises -> Optional[FileError]:
    if _has_nul(path):
        return FileError(0, _NUL_IN_PATH)
    var target = path
    if file_facts(path, False).symlink():
        var resolved = real_path(path)
        if resolved:
            target = resolved.value()
    var failure = _replace_file(target, contents)
    if failure and not same_path(target, path):
        var number = failure.value().number
        if number == 13 or number == 1 or number == 30:
            return _replace_file(path, contents)
    return failure^


def save_editor_file(path: String, contents: String) raises -> Optional[String]:
    var failure = atomic_write(path, contents.as_bytes())
    if failure:
        return "Could not save " + path + ": " + failure.value().text
    return None
