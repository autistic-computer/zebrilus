from std.sys import argv
from zzmojo.protocol import Decoder, append_uint, append_text
from zzmojo.input_protocol import KeyInput, _read_lines
from zzmojo.terminal_view import PointerCell, search_result_text
from zzmojo.terminal_search_protocol import (
    SearchQuery, CopyAction, search_request, copy_request, copy_mode_request,
    append_search_query, decode_search_ui,
)
from zzmojo.terminal_search import search_key_action, search_shortcut, search_case_label
from zzmojo.terminal_mode_indicator import mode_indicator
from zzmojo.terminal_model import TerminalMode


def query(mut reader: Decoder) raises -> SearchQuery:
    var text = reader.text()
    var mode = UInt8(reader.uint())
    var case_policy = UInt8(reader.uint())
    var direction = UInt8(reader.uint())
    return SearchQuery(text, mode, case_policy, direction)


def optional_text(mut reader: Decoder) raises -> Optional[String]:
    if reader.uint() != 0:
        return reader.text()
    return None


def copy_action(mut reader: Decoder) raises -> CopyAction:
    var result = CopyAction(UInt8(reader.uint()))
    result.flag = reader.uint() != 0
    result.text = reader.text()
    result.direction = UInt8(reader.uint())
    result.copy_request.request_id = reader.uint()
    result.copy_request.clipboard = reader.uint() != 0
    result.copy_request.buffer = UInt8(reader.uint())
    result.copy_request.prefix = optional_text(reader)
    result.copy_request.pipe = optional_text(reader)
    result.copy_request.clear_selection = reader.uint() != 0
    result.copy_request.cancel = reader.uint() != 0
    if reader.uint() != 0:
        result.line = _read_lines(reader)
    result.selection = UInt8(reader.uint())
    result.regex = reader.uint() != 0
    result.incremental = reader.uint() != 0
    result.pointer.column = Int(reader.uint())
    result.pointer.row = Int(reader.uint())
    result.pointer.click_count = Int(reader.uint())
    result.pointer.rectangle = reader.uint() != 0
    return result^


def key(mut reader: Decoder) raises -> KeyInput:
    var result = KeyInput()
    result.key = UInt8(reader.uint())
    result.codepoint = UInt32(reader.uint())
    result.modifiers = UInt8(reader.uint())
    result.action = UInt8(reader.uint())
    result.text = optional_text(reader)
    return result^


def run(mut reader: Decoder, kind: UInt64) raises -> List[UInt8]:
    if kind == 0:
        var pane = reader.uint()
        var output = reader.uint() != 0
        var action = UInt8(reader.uint())
        if action == 17 or action == 18:
            return search_request(pane, action, query(reader), output)
        if action == 16 or action == 28:
            var copy = copy_action(reader)
            var count: Optional[UInt32] = None
            if action == 28:
                count = UInt32(reader.uint())
            return copy_request(pane, copy, count, output)
        if action == 26:
            return copy_mode_request(pane, True, command_output=output)
        if action == 27:
            var scroll = reader.uint() != 0
            var hide = reader.uint() != 0
            return copy_mode_request(pane, scroll, hide, output)
        return search_request(pane, action, command_output=output)
    var result = List[UInt8]()
    if kind == 1:
        var event = decode_search_ui(reader.bytes())
        if not event:
            raise Error("search UI fixture did not decode")
        append_uint(result, event.value().pane)
        append_uint(result, UInt64(event.value().direction))
    elif kind == 2:
        var initial = query(reader)
        var input = key(reader)
        append_uint(result, UInt64(search_key_action(initial, input)))
        append_search_query(result, initial)
    elif kind == 3:
        var input = key(reader)
        var prefix = reader.uint() != 0
        var modal = reader.uint() != 0
        append_uint(result, UInt64(search_shortcut(input, prefix, modal)))
    elif kind == 4:
        var current = Int(reader.uint())
        var total = Int(reader.uint())
        var pending = reader.uint() != 0
        var invalid = reader.uint() != 0
        var text = search_result_text(current, total, pending, invalid)
        result.extend(text.as_bytes())
    elif kind == 5:
        var text = search_case_label(UInt8(reader.uint()))
        result.extend(text.as_bytes())
    elif kind == 6:
        var mode = UInt8(reader.uint())
        var position = UInt32(reader.uint())
        var total = UInt32(reader.uint())
        var hide = reader.uint() != 0
        var unseen = UInt32(reader.uint())
        var indicator = mode_indicator(TerminalMode(mode, position, total, hide), unseen)
        append_uint(result, UInt64(Bool(indicator)))
        if indicator:
            append_uint(result, UInt64(Bool(indicator.value().label)))
            if indicator.value().label:
                append_text(result, indicator.value().label.value())
            append_text(result, indicator.value().detail)
    else:
        raise Error("unknown terminal search fixture")
    return result^


def main() raises:
    var args = argv()
    var data: List[UInt8]
    with open(args[1], "r") as source:
        data = source.read_bytes()
    if len(data) > 16 * 1024 * 1024:
        raise Error("search fixture batch exceeds its byte budget")
    var reader = Decoder(data^)
    reader.offset = 0
    var count = reader.uint()
    if count > 100_000:
        raise Error("search fixture count exceeds its limit")
    var result = List[UInt8]()
    for _ in range(Int(count)):
        var kind = reader.uint()
        var item = run(reader, kind)
        append_uint(result, UInt64(len(item)))
        result.extend(Span(item))
        if len(result) > 16 * 1024 * 1024:
            raise Error("search fixture result exceeds its byte budget")
    reader.finish()
    with open(args[2], "w") as target:
        target.write_bytes(Span(result))
