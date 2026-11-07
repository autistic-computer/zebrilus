from .input_protocol import KeyInput, input_key_name


def palette_open_shortcut(
    input: KeyInput,
    apple: Bool = False,
    overlay: Bool = False,
    capture: Bool = False,
) raises -> Bool:
    if overlay or capture or input.action == 2:
        return False
    var bare = input.copy()
    bare.modifiers = 0
    bare.text = None
    var base = input_key_name(bare)
    if base.byte_length() == 0 and input.text:
        base = input.text.value().copy()
    if apple:
        return (input.modifiers == 8 or input.modifiers == 9) and (
            base == "k" or base == "p"
        )
    return input.modifiers == 3 and (
        base == "k" or base == "p" or base == "K" or base == "P"
    )
