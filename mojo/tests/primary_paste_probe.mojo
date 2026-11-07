from std.sys import argv
from zzmojo.protocol import Decoder
from zzmojo.actions import MAX_PENDING_INPUT_BYTES
from zzmojo.primary_paste import PrimaryPasteController, PrimaryTarget, should_paste_primary


def main() raises:
    var args = argv()
    var data: List[UInt8]
    with open(args[1], "r") as source:
        data = source.read_bytes()
    var reader = Decoder(data^)
    reader.offset = 0
    var output = List[UInt8]()
    var policies = reader.uint()
    if policies != 8:
        raise Error("PRIMARY policy fixture is incomplete")
    for _ in range(Int(policies)):
        var flags = reader.uint()
        output.append(UInt8(should_paste_primary(flags & 1 != 0, flags & 2 != 0, flags & 4 != 0)))
    var traces = reader.uint()
    if traces > 10000:
        raise Error("PRIMARY trace fixture exceeds its count limit")
    var target = PrimaryTarget(0, UInt64.MAX, 0, UInt64.MAX)
    for _ in range(Int(traces)):
        var controller = PrimaryPasteController()
        var count = reader.uint()
        if count > 64:
            raise Error("PRIMARY trace exceeds its step limit")
        for _ in range(Int(count)):
            var op = reader.uint()
            if op > 8:
                raise Error("unknown PRIMARY trace operation")
            var handled: Bool
            if op == 0:
                handled = controller.release()
            else:
                var flags = op - 1
                handled = controller.press(target, flags & 1 != 0, flags & 2 != 0, flags & 4 != 0)
            output.append(UInt8(handled))
            output.append(UInt8(Bool(controller.pressed)))
    reader.finish()
    with open(args[2], "w") as destination:
        destination.write_bytes(Span(output))
    var controller = PrimaryPasteController()
    if not controller.press(target, False, True, True) or not controller.release():
        raise Error("PRIMARY shift override did not retain its matching release")
    var paste = controller.prepare(target, "a\x00世界\n")
    if not paste or paste.value().target.pane != 0 or paste.value().text != "a\x00世界\n":
        raise Error("same-batch middle release lost its captured paste")
    if controller.prepare(target, "duplicate"):
        raise Error("PRIMARY pending paste was delivered twice")
    _ = controller.press(target, False, False, False)
    var changed = target
    changed.client = 1
    if controller.accepts(changed):
        raise Error("stale PRIMARY capture would read from the native clipboard")
    if controller.prepare(changed, "stale"):
        raise Error("PRIMARY capture survived a changed source client")
    if not controller.release():
        raise Error("stale PRIMARY payload lost its release suppression")
    _ = controller.press(target, False, False, False)
    if controller.prepare(target, ""):
        raise Error("empty PRIMARY text produced a paste")
    if not controller.release():
        raise Error("empty PRIMARY text lost its captured release")
    _ = controller.press(target, False, False, False)
    var bytes = List[UInt8](length=MAX_PENDING_INPUT_BYTES, fill=97)
    var text = String(from_utf8=Span(bytes))
    var exact = controller.prepare(target, text)
    if not exact or exact.value().text.byte_length() != MAX_PENDING_INPUT_BYTES:
        raise Error("exact PRIMARY paste byte limit was rejected")
    _ = controller.press(target, False, False, False)
    text += "a"
    var refused = False
    try:
        _ = controller.prepare(target, text)
    except:
        refused = True
    if not refused or controller.pending or not controller.release():
        raise Error("oversized PRIMARY paste changed its one-shot release contract")
    print("PRIMARY policy/traces, same-batch release, source identity and 4 MiB boundary exact")
