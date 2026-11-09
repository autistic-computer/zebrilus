from .palette_text import palette_space, palette_trim, palette_words, scalar_width


def text_slice(text: String, begin: Int, end: Int) -> String:
    var count = text.byte_length()
    var first = min(max(begin, 0), count)
    var last = min(max(end, first), count)
    return String(from_utf8_lossy=text.as_bytes()[first:last])


def text_find(text: String, needle: String, start: Int = 0) -> Int:
    var count = text.byte_length()
    var width = needle.byte_length()
    if width == 0:
        return start if start <= count else -1
    var index = max(start, 0)
    while index + width <= count:
        var matched = True
        for offset in range(width):
            if text.as_bytes()[index + offset] != needle.as_bytes()[offset]:
                matched = False
                break
        if matched:
            return index
        index += 1
    return -1


def text_rfind(text: String, needle: String) -> Int:
    var width = needle.byte_length()
    var index = text.byte_length() - width
    while index >= 0:
        var matched = True
        for offset in range(width):
            if text.as_bytes()[index + offset] != needle.as_bytes()[offset]:
                matched = False
                break
        if matched:
            return index
        index -= 1
    return -1


def text_boundary(text: String, index: Int) -> Bool:
    var count = text.byte_length()
    if index <= 0 or index >= count:
        return index == 0 or index == count
    var byte = text.as_bytes()[index]
    return byte < 128 or byte >= 192


def text_floor(text: String, index: Int) -> Int:
    var at = min(max(index, 0), text.byte_length())
    while not text_boundary(text, at):
        at -= 1
    return at


def text_ceil(text: String, index: Int) -> Int:
    var at = min(max(index, 0), text.byte_length())
    while not text_boundary(text, at):
        at += 1
    return at


def text_trim(text: String) -> String:
    return palette_trim(text)


def text_trim_start(text: String) -> String:
    return palette_trim(text, False)


def text_trim_end(text: String) -> String:
    var last = 0
    var byte = 0
    for character in text.codepoints():
        var scalar = UInt32(Int(character))
        byte += scalar_width(scalar)
        if not palette_space(scalar):
            last = byte
    return text_slice(text, 0, last)


def text_blank(text: String) -> Bool:
    for character in text.codepoints():
        if not palette_space(UInt32(Int(character))):
            return False
    return True


def text_lines(text: String) -> List[String]:
    var lines = List[String]()
    var count = text.byte_length()
    var start = 0
    for index in range(count):
        if text.as_bytes()[index] == 10:
            var end = index
            if end > start and text.as_bytes()[end - 1] == 13:
                end -= 1
            lines.append(text_slice(text, start, end))
            start = index + 1
    if start < count:
        lines.append(text_slice(text, start, count))
    return lines^


def text_split(text: String, separator: String) -> List[String]:
    var parts = List[String]()
    var start = 0
    while True:
        var at = text_find(text, separator, start)
        if at < 0:
            break
        parts.append(text_slice(text, start, at))
        start = at + separator.byte_length()
    parts.append(text_slice(text, start, text.byte_length()))
    return parts^


def text_join(parts: List[String], separator: String) -> String:
    var joined = String()
    for index in range(len(parts)):
        if index > 0:
            joined += separator
        joined += parts[index]
    return joined^


def text_words(text: String) -> List[String]:
    return palette_words(text)


def text_contains(text: String, needle: String) -> Bool:
    return text_find(text, needle) >= 0


def text_first_scalar(text: String) -> Int:
    for character in text.codepoints():
        return Int(character)
    return -1


def text_chars(text: String) -> Int:
    var count = 0
    for byte in text.as_bytes():
        if byte < 128 or byte >= 192:
            count += 1
    return count


def text_take_chars(text: String, limit: Int) -> String:
    var byte = 0
    var taken = 0
    for character in text.codepoints():
        if taken == limit:
            break
        byte += scalar_width(UInt32(Int(character)))
        taken += 1
    return text_slice(text, 0, byte)


def text_file_name(path: String) -> String:
    var end = path.byte_length()
    while end > 1 and path.as_bytes()[end - 1] == 47:
        end -= 1
    var start = end
    while start > 0 and path.as_bytes()[start - 1] != 47:
        start -= 1
    var name = text_slice(path, start, end)
    if name == "" or name == ".." or name == "/":
        return String()
    return name^
