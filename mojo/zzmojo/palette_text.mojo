def palette_space(scalar: UInt32) -> Bool:
    return (
        scalar >= 9
        and scalar <= 13
        or scalar == 32
        or scalar == 133
        or scalar == 160
        or scalar == 5760
        or scalar >= 8192
        and scalar <= 8202
        or scalar == 8232
        or scalar == 8233
        or scalar == 8239
        or scalar == 8287
        or scalar == 12288
    )


def scalar_width(scalar: UInt32) -> Int:
    return (
        1 if scalar
        < 128 else 2 if scalar
        < 2048 else 3 if scalar
        < 65536 else 4
    )


def palette_trim(text: String, trailing: Bool = True) -> String:
    var first = 0
    var last = 0
    var byte = 0
    var started = False
    for character in text.codepoints():
        var scalar = UInt32(Int(character))
        var end = byte + scalar_width(scalar)
        if not palette_space(scalar):
            if not started:
                first = byte
                started = True
            last = end
        byte = end
    if not started:
        return String()
    return String(
        from_utf8_lossy=text.as_bytes()[first:last if trailing else byte]
    )


def palette_words(text: String) -> List[String]:
    var words = List[String]()
    var byte = 0
    var start = -1
    for character in text.codepoints():
        var scalar = UInt32(Int(character))
        if palette_space(scalar):
            if start >= 0:
                words.append(
                    String(from_utf8_lossy=text.as_bytes()[start:byte])
                )
                start = -1
        elif start < 0:
            start = byte
        byte += scalar_width(scalar)
    if start >= 0:
        words.append(String(from_utf8_lossy=text.as_bytes()[start:byte]))
    return words^


def palette_contains_space(text: String) -> Bool:
    for character in text.codepoints():
        if palette_space(UInt32(Int(character))):
            return True
    return False
