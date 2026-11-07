from .actions import MAX_PENDING_INPUT_BYTES
from .stream_protocol import input_text
from .input_protocol import input_paste


def pasted_commit(text: String, composed: Bool) -> Bool:
    if composed:
        return False
    if "\n" in text or "\r" in text:
        return True
    var count = 0
    for _ in text.codepoint_slices():
        count += 1
        if count > 1:
            return True
    return False


def terminal_commit(
    pane: UInt64,
    text: String,
    composed: Bool = False,
    command_output: Bool = False,
) raises -> Optional[List[UInt8]]:
    if text.byte_length() == 0:
        return None
    if text.byte_length() > MAX_PENDING_INPUT_BYTES:
        raise Error("pending terminal input exceeds queue limit")
    if pasted_commit(text, composed):
        return input_paste(pane, text, command_output)
    return input_text(pane, text)
