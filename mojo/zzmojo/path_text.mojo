from .palette_text import palette_space


@fieldwise_init
struct FilesPath(Copyable, Movable):
    var dir: String
    var typed: String
    var filter: String


def _slice(value: String, start: Int, end: Int) -> String:
    return String(from_utf8_lossy=value.as_bytes()[start:end])


def _last_slash(value: String) -> Int:
    var last = -1
    for index in range(value.byte_length()):
        if value.as_bytes()[index] == 47:
            last = index
    return last


def normalize_dir(value: String) -> String:
    var start = 1
    var anchor = String("/")
    if not value.startswith("/"):
        var cut = value.byte_length()
        for index in range(value.byte_length()):
            if value.as_bytes()[index] == 47:
                cut = index
                break
        anchor = String(_slice(value, 0, cut), "/")
        start = min(cut + 1, value.byte_length())
    var parts = List[String]()
    for index in range(start, value.byte_length() + 1):
        if index < value.byte_length() and value.as_bytes()[index] != 47:
            continue
        var part = _slice(value, start, index)
        start = index + 1
        if part == "" or part == ".":
            continue
        if part == ".." and len(parts) > 0 and parts[len(parts) - 1] != "..":
            _ = parts.pop()
        elif part != ".." or anchor != "/":
            parts.append(part^)
    for part in parts:
        anchor += part
        anchor += "/"
    return anchor^


def split_rest(rest: String, start: String) -> FilesPath:
    var cut = _last_slash(rest)
    var typed = String()
    var filter = rest.copy()
    if cut >= 0:
        typed = _slice(rest, 0, cut + 1)
        filter = _slice(rest, cut + 1, rest.byte_length())
    elif rest.startswith("~"):
        typed = String("~/")
        filter = _slice(rest, 1, rest.byte_length())
    var dir = normalize_dir(typed if typed.startswith("/") or typed.startswith("~") else String(start.rstrip("/"), "/", typed))
    return FilesPath(dir^, typed^, filter^)


def parent_dir(dir: String, root: Optional[String] = None) -> Optional[String]:
    if dir == "/":
        return None
    var up = normalize_dir(String(dir, "../"))
    if up.endswith("../") and root and root.value().startswith("/"):
        return parent_dir(normalize_dir(root.value()))
    return up^


def path_last_name(dir: String) -> Optional[String]:
    var trimmed = String(dir.rstrip("/"))
    var name = _slice(trimmed, _last_slash(trimmed) + 1, trimmed.byte_length())
    if name.byte_length() == 0 or name.startswith("~") or name == "..":
        return None
    return name^


def plain_path(root: String, rel: String, host: Optional[String] = None) -> String:
    var path = String(root.rstrip("/"), "/", rel.rstrip("/"))
    return String(host.value(), ":", path) if host else path^


def start_dir(cwd: Optional[String], home: Optional[String]) -> String:
    if not cwd or not (cwd.value().startswith("/") or cwd.value().startswith("~")):
        return String("~/")
    var dir = cwd.value().copy()
    if home:
        var base = String(home.value().rstrip("/"))
        if base.byte_length() > 0 and dir.startswith(base):
            var rest = _slice(dir, base.byte_length(), dir.byte_length())
            if rest == "" or rest.startswith("/"):
                dir = String("~", rest)
    return normalize_dir(dir)


def _ascii_alpha(value: UInt8) -> Bool:
    return value >= 65 and value <= 90 or value >= 97 and value <= 122


def _ascii_number(value: UInt8) -> Bool:
    return value >= 48 and value <= 57


def _posix_bare(value: UInt32) -> Bool:
    return value < 128 and (_ascii_alpha(UInt8(value)) or _ascii_number(UInt8(value)) or Int(value) in (95, 46, 47, 58, 64, 37, 43, 44, 61, 45))


def _mention_bare(value: UInt32) -> Bool:
    return value < 128 and (_ascii_alpha(UInt8(value)) or _ascii_number(UInt8(value)) or Int(value) in (95, 46, 47, 45))


struct _SplitPath(Movable):
    var anchor: String
    var parts: List[String]

    def __init__(out self, value: String):
        var bytes = value.as_bytes()
        var drive = len(bytes) >= 2 and _ascii_alpha(bytes[0]) and bytes[1] == 58 and (len(bytes) == 2 or bytes[2] == 47 or bytes[2] == 92)
        self.anchor = String(_slice(value, 0, 1), ":/") if drive else String("/" if value.startswith("/") else "")
        self.parts = List[String]()
        self.push(_slice(value, 2 if drive else 0, value.byte_length()))

    def push(mut self, value: String):
        var windows = self.anchor.byte_length() == 3
        var first = 0
        for index in range(value.byte_length() + 1):
            if index < value.byte_length() and value.as_bytes()[index] != 47 and not (windows and value.as_bytes()[index] == 92):
                continue
            var part = _slice(value, first, index)
            first = index + 1
            if part != "" and part != ".":
                self.parts.append(part^)

    def absolute(self) -> String:
        var value = self.anchor.copy()
        for index in range(len(self.parts)):
            if index > 0:
                value += "/"
            value += self.parts[index]
        return String(".") if value == "" else value^

    def relative_to(self, base: Self) -> Optional[String]:
        var left = self.anchor.as_bytes()
        var right = base.anchor.as_bytes()
        if len(left) != len(right) or len(self.parts) < len(base.parts):
            return None
        for index in range(len(left)):
            var a = left[index] + 32 if left[index] >= 65 and left[index] <= 90 else left[index]
            var b = right[index] + 32 if right[index] >= 65 and right[index] <= 90 else right[index]
            if a != b:
                return None
        for index in range(len(base.parts)):
            if self.parts[index] != base.parts[index]:
                return None
        if len(self.parts) == len(base.parts):
            return String(".")
        var value = String()
        for index in range(len(base.parts), len(self.parts)):
            if index > len(base.parts):
                value += "/"
            value += self.parts[index]
        return value^


def _quote_shell(style: UInt8, text: String) -> String:
    var bare = True
    for character in text.codepoints():
        var scalar = UInt32(Int(character))
        if not _posix_bare(scalar) or (style == 2 and scalar == 44):
            if not (style == 2 and scalar == 92):
                bare = False
    if style == 0:
        return text.copy() if bare else String("'", text.replace("'", "'\\''"), "'")
    if style == 1:
        return text.copy() if bare and not text.startswith("%") else String("'", text.replace("\\", "\\\\").replace("'", "\\'"), "'")
    if style == 2:
        var bytes = text.as_bytes()
        var number = len(bytes) > 0 and (_ascii_number(bytes[0]) or ((bytes[0] == 46 or bytes[0] == 43) and len(bytes) > 1 and _ascii_number(bytes[1])))
        if bare and not text.startswith("@") and not number:
            return text.copy()
        var quoted = String("'")
        for character in text.codepoints():
            var code = Int(character)
            if code == 39 or code >= 0x2018 and code <= 0x201B:
                quoted += String(character)
            quoted += String(character)
        return String(quoted, "'")
    var allowed = text.byte_length() > 0 and (_ascii_alpha(text.as_bytes()[0]) or Int(text.as_bytes()[0]) in (95, 46, 47))
    for character in text.codepoints():
        allowed = allowed and _mention_bare(UInt32(Int(character)))
    if allowed:
        return text.copy()
    var longest = 0
    for index in range(text.byte_length()):
        if text.as_bytes()[index] == 39:
            var run = 0
            while index + run + 1 < text.byte_length() and text.as_bytes()[index + run + 1] == 35:
                run += 1
            longest = max(longest, run)
    var hashes = String("#") * (longest + 1)
    return String("r", hashes, "'", text, "'", hashes)


def insert_path_text(root: String, rel: String, kind: UInt8, cwd: Optional[String], style: UInt8, absolute: Bool = False) raises -> Optional[String]:
    if root.byte_length() > 4096 or rel.byte_length() > 4096 or (cwd and cwd.value().byte_length() > 4096) or style > 5 or kind > 1:
        raise Error("path insertion input exceeds its protocol limits")
    var path = _SplitPath(root)
    path.push(rel)
    var full = path.absolute()
    for character in full.codepoints():
        var scalar = Int(character)
        if scalar <= 31 or scalar >= 127 and scalar <= 159:
            return None
    var relative: Optional[String] = None
    if cwd and not absolute:
        relative = path.relative_to(_SplitPath(cwd.value()))
    var text = relative.value().copy() if relative else full.copy()
    if kind == 1:
        if not text.endswith("/"):
            text += "/"
        if not full.endswith("/"):
            full += "/"
    var out: String
    if style < 4:
        if relative and (text.startswith("-") or text.startswith("=") or text.startswith("+")):
            text = String("./", text)
        out = _quote_shell(style, text)
    elif style == 4:
        var last = UInt32(0)
        var trimmed = _slice(text, 0, text.byte_length() - 1) if text.endswith("/") else text.copy()
        for character in trimmed.codepoints():
            last = UInt32(Int(character))
        var bare = last < 128 and (_ascii_alpha(UInt8(last)) or _ascii_number(UInt8(last)) or last == 95)
        for character in text.codepoints():
            bare = bare and _mention_bare(UInt32(Int(character)))
        out = String("@", text) if bare else String("@\"", text, "\"") if '"' not in text and "#" not in text else full.copy()
    else:
        var whitespace = False
        for character in text.codepoints():
            whitespace = whitespace or palette_space(UInt32(Int(character)))
        out = String('"', text, '"') if whitespace and '"' not in text else text.copy()
    return String(out, " ")
