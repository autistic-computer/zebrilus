from .input_protocol import KeyInput


def clipboard_shortcut(
    input: KeyInput, claims_prefix: Bool = False, modal_capture: Bool = False
) -> UInt8:
    if claims_prefix or modal_capture:
        return 0
    var codepoint = input.codepoint
    if input.key != 0:
        if input.key != 16 or not input.text:
            return 0
        var count = 0
        for character in input.text.value().codepoints():
            codepoint = UInt32(character)
            count += 1
        if count != 1:
            return 0
    if input.modifiers == 3:
        if codepoint >= 65 and codepoint <= 90:
            codepoint += 32
    elif input.modifiers != 8 and input.modifiers != 9:
        return 0
    if codepoint == 99:
        return 1
    if codepoint == 118:
        return 2
    if codepoint == 97:
        return 3
    return 0
