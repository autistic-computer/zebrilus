from .path_case import PathCase, path_respects_case


def path_depth(label: String) -> Int:
    var end = label.byte_length()
    while end > 0 and label.as_bytes()[end - 1] == 47:
        end -= 1
    var depth = 0
    for index in range(end):
        depth += Int(label.as_bytes()[index] == 47)
    return depth


def path_in_basename(
    query: String, label: String, characters: PathCase
) -> Bool:
    if query.byte_length() == 0 or "/" in query:
        return False
    var sensitive = path_respects_case(query, characters)
    var needle = List[UInt32]()
    for character in query.codepoints():
        var scalar = UInt32(Int(character))
        needle.append(scalar if sensitive else characters.lower_first(scalar))
    var end = label.byte_length()
    while end > 0 and label.as_bytes()[end - 1] == 47:
        end -= 1
    var start = 0
    for index in range(end):
        if label.as_bytes()[index] == 47:
            start = index + 1
    var basename = String(from_utf8_lossy=label.as_bytes()[start:end])
    var position = 0
    for character in basename.codepoints():
        var scalar = UInt32(Int(character))
        if (scalar if sensitive else characters.lower_first(scalar)) == needle[
            position
        ]:
            position += 1
            if position == len(needle):
                return True
    return False


def path_prior(
    query: String, label: String, changed: Bool, characters: PathCase
) -> UInt32:
    return UInt32(
        (1024 if changed else 0)
        + (256 if path_in_basename(query, label, characters) else 0)
        + max(64 - path_depth(label), 0)
    )


def path_browse_order(
    left: String, left_dir: Bool, right: String, right_dir: Bool
) -> Int:
    if left_dir != right_dir:
        return -1 if left_dir else 1
    return -1 if left < right else 1 if left > right else 0
