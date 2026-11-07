from std.sys import argv
from std.testing import assert_true
from zzmojo.protocol import Decoder, append_uint, append_text
from zzmojo.tree_wire import read_text, read_uint
from zzmojo.palette_shortcuts import (
    PaletteKeystroke,
    palette_keystroke,
    palette_shortcut,
    MAX_PALETTE_SHORTCUT_BYTES,
    MAX_PALETTE_SHORTCUT_KEYS,
)


def write_key(mut bytes: List[UInt8], value: PaletteKeystroke):
    append_uint(bytes, UInt64(value.modifiers))
    append_text(bytes, value.key)
    append_uint(bytes, UInt64(Bool(value.key_char)))
    if value.key_char:
        append_text(bytes, value.key_char.value())


def main() raises:
    var args = argv()
    var data: List[UInt8]
    with open(args[1], "r") as source:
        data = source.read_bytes()
    var reader = Decoder(data^)
    reader.offset = 0
    var cases = Int(read_uint(reader, 65536))
    var bytes = List[UInt8]()
    for _ in range(cases):
        var spelling = read_text(reader, MAX_PALETTE_SHORTCUT_BYTES)
        var key = palette_keystroke(spelling)
        append_uint(bytes, UInt64(Bool(key)))
        if key:
            write_key(bytes, key.value())
        var shortcut = palette_shortcut(spelling)
        append_uint(bytes, UInt64(shortcut.kind))
        append_text(bytes, shortcut.text)
        append_uint(bytes, UInt64(len(shortcut.keys)))
        for value in shortcut.keys:
            write_key(bytes, value)
    reader.finish()
    var large = List[UInt8](
        length=MAX_PALETTE_SHORTCUT_BYTES + 1, fill=UInt8(120)
    )
    var refused = False
    try:
        _ = palette_shortcut(String(from_utf8=Span(large)))
    except:
        refused = True
    assert_true(refused)
    var words = String()
    for _ in range(MAX_PALETTE_SHORTCUT_KEYS + 1):
        words += "b "
    refused = False
    try:
        _ = palette_shortcut(words)
    except:
        refused = True
    assert_true(refused)
    with open(args[2], "w") as target:
        target.write_all(Span(bytes))
