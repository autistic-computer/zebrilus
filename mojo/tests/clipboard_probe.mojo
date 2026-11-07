from std.sys import argv
from zzmojo.protocol import Decoder
from zzmojo.clipboard_protocol import (
    ClipboardEvent,
    decode_clipboard_event,
    encode_clipboard_event,
    clipboard_reply,
    decode_clipboard_reply,
)
from zzmojo.clipboard_host import clipboard_write
from zzmojo.clipboard_keys import clipboard_shortcut
from zzmojo.input_protocol import (
    KeyInput,
    decode_input_frame,
    encode_input_frame,
)
from zzmojo.terminal_commit import terminal_commit
from zzmojo.clipboard_upload import (
    paste_upload_begin,
    paste_upload_chunk,
    decode_paste_upload,
)


def main() raises:
    var args = argv()
    var bytes: List[UInt8]
    with open(args[2], "r") as file:
        bytes = file.read_bytes()
    var reader = Decoder(bytes.copy())
    reader.offset = 0
    var output = List[UInt8]()
    if args[1] == "roundtrip":
        if len(bytes) >= 10 and bytes[8] == 9:
            if bytes[9] == 24:
                var data = decode_clipboard_reply(bytes^)
                output = clipboard_reply(Span(data))
            else:
                var input = decode_input_frame(bytes^)
                output = encode_input_frame(input)
        elif len(bytes) >= 10 and (bytes[8] == 15 or bytes[8] == 16):
            var upload = decode_paste_upload(bytes^)
            if upload.kind == 15:
                output = paste_upload_begin(
                    upload.id,
                    upload.pane,
                    upload.extension,
                    upload.total,
                    upload.purpose,
                )
            else:
                output = paste_upload_chunk(upload.id, Span(upload.bytes))
        else:
            var event = decode_clipboard_event(bytes^)
            if not event:
                raise Error("unrecognized clipboard event")
            output = encode_clipboard_event(event.value())
    elif args[1] == "build":
        var kind = UInt8(reader.uint())
        if kind == 0:
            var composed = reader.uint() != 0
            var text = reader.text()
            output = terminal_commit(7, text, composed).value().copy()
        elif kind == 100:
            var id = reader.uint()
            var pane = reader.uint()
            var purpose = UInt8(reader.uint())
            var extension = reader.text()
            var total = UInt32(reader.uint())
            output = paste_upload_begin(id, pane, extension, total, purpose)
        elif kind == 24 or kind == 101:
            var id = reader.uint() if kind == 101 else UInt64(0)
            var count = Int(reader.uint())
            var data = List[UInt8]()
            for _ in range(count):
                data.append(reader.byte())
            output = paste_upload_chunk(
                id, Span(data)
            ) if kind == 101 else clipboard_reply(Span(data))
        else:
            var event = ClipboardEvent()
            event.kind = kind
            event.sequence = reader.uint()
            if kind == 6:
                event.pane = reader.uint()
                event.request_id = reader.uint()
                event.target = UInt8(reader.uint())
                event.text = reader.text()
                event.producer = UInt8(reader.uint())
            output = encode_clipboard_event(event)
    elif args[1] == "keys":
        var count = Int(reader.uint())
        for _ in range(count):
            var input = KeyInput()
            input.key = UInt8(reader.uint())
            input.codepoint = UInt32(reader.uint())
            input.modifiers = UInt8(reader.uint())
            input.action = UInt8(reader.uint())
            var prefix = reader.uint() != 0
            var modal = reader.uint() != 0
            if input.key == 16:
                input.text = String(chr(Int(input.codepoint)))
            output.append(clipboard_shortcut(input, prefix, modal))
    elif args[1] == "policy":
        var count = Int(reader.uint())
        for _ in range(count):
            var target = UInt8(reader.uint())
            var producer = UInt8(reader.uint())
            var empty = reader.uint() != 0
            var setting: Optional[String] = None
            if reader.uint() != 0:
                setting = reader.text()
            var primary = reader.uint() != 0
            var retained = reader.uint() != 0
            var result = clipboard_write(
                target, producer, empty, setting, primary
            )
            output.append(UInt8(result.clipboard))
            output.append(UInt8(result.primary))
            output.append(UInt8(result.copy_flash and retained))
    else:
        raise Error("unknown clipboard probe mode")
    with open(args[3], "w") as file:
        file.write_bytes(Span(output))
