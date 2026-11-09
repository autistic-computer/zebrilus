from .agent_text import text_blank, text_ceil, text_slice, text_take_chars

comptime OUTPUT_PANE_MAX_BYTES = 256 * 1024
comptime OUTPUT_PANE_PROGRAM = "printf %s \"$0\" | base64 -d | less -R --tilde +G"


def base64_text(text: String) -> String:
    var table = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
    var count = text.byte_length()
    var encoded = List[UInt8](capacity=(count + 2) // 3 * 4)
    var index = 0
    while index < count:
        var first = UInt32(text.as_bytes()[index])
        var second = UInt32(text.as_bytes()[index + 1]) if index + 1 < count else UInt32(0)
        var third = UInt32(text.as_bytes()[index + 2]) if index + 2 < count else UInt32(0)
        var chunk = first << 16 | second << 8 | third
        encoded.append(table.as_bytes()[Int(chunk >> 18 & 63)])
        encoded.append(table.as_bytes()[Int(chunk >> 12 & 63)])
        encoded.append(
            table.as_bytes()[Int(chunk >> 6 & 63)] if index + 1 < count else UInt8(61)
        )
        encoded.append(
            table.as_bytes()[Int(chunk & 63)] if index + 2 < count else UInt8(61)
        )
        index += 3
    return String(from_utf8_lossy=Span(encoded))


def output_pane_args(
    pane: UInt64,
    cwd: String,
    command: String,
    exit_code: Optional[Int64],
    output: String,
) -> Optional[List[String]]:
    if text_blank(output):
        return None
    var count = output.byte_length()
    var start = text_ceil(output, max(count - OUTPUT_PANE_MAX_BYTES, 0))
    var text = String("$ ", command, "\n\n")
    if start > 0:
        text += "[earlier output left out]\n"
    text += text_slice(output, start, count)
    if not text.endswith("\n"):
        text += "\n"
    if exit_code and exit_code.value() != 0:
        text += String("\n[exit ", exit_code.value(), "]\n")
    var args = List[String]()
    args.append("-h")
    args.append("-t")
    args.append(String("%", pane))
    args.append("-c")
    args.append(cwd)
    args.append("-T")
    args.append(text_take_chars(command, 60))
    args.append("sh")
    args.append("-c")
    args.append(OUTPUT_PANE_PROGRAM)
    args.append(base64_text(text))
    return Optional[List[String]](args^)
