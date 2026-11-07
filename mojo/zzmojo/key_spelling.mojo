from .palette_text import palette_trim


def canonical_protocol_key(value: String) raises -> String:
    var rest = palette_trim(value)
    if value == " " or rest == "Space":
        return " "
    var bytes = rest.as_bytes()
    if len(bytes) == 4 and (bytes[0] == 110 or bytes[0] == 78) and (bytes[1] == 111 or bytes[1] == 79) and (bytes[2] == 110 or bytes[2] == 78) and (bytes[3] == 101 or bytes[3] == 69):
        return "None"
    var control = False
    var alt = False
    while True:
        var skip = 0
        if rest.startswith("Ctrl-"):
            control = True
            skip = 5
        elif rest.startswith("C-"):
            control = True
            skip = 2
        elif rest.startswith("Alt-"):
            alt = True
            skip = 4
        elif rest.startswith("M-"):
            alt = True
            skip = 2
        if skip == 0:
            break
        var tail = String(from_utf8=rest.as_bytes()[skip:])
        rest = tail^
    if not control and not alt:
        return rest^
    if rest == "Space" or (rest.byte_length() == 0 and value.endswith(" ")):
        rest = " "
    return String("C-" if control else "", "M-" if alt else "", rest)
