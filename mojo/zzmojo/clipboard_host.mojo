from mjui.input import Clipboard
from .clipboard_protocol import (
    ClipboardState,
    ClipboardResponse,
    MAX_CLIPBOARD_REPLY_BYTES,
    clipboard_reply,
)


@fieldwise_init
struct ClipboardWrite(TrivialRegisterPassable):
    var clipboard: Bool
    var primary: Bool
    var copy_flash: Bool


def clipboard_write(
    target: UInt8,
    producer: UInt8,
    empty: Bool,
    set_clipboard: Optional[String] = None,
    primary_supported: Bool = True,
) raises -> ClipboardWrite:
    if target > 1 or producer > 1:
        raise Error("unknown clipboard target or producer")
    if empty:
        return ClipboardWrite(False, False, False)
    var server = producer == 0
    var enabled = not set_clipboard or set_clipboard.value() != "off"
    var clipboard = target == 0 or (server and enabled)
    return ClipboardWrite(
        clipboard, target == 1 and primary_supported, server and clipboard
    )


@fieldwise_init
struct ClipboardDelivery(Movable):
    var copied_panes: List[UInt64]
    var reply: Optional[ClipboardResponse]

    def take_reply(mut self) -> Optional[ClipboardResponse]:
        var result: Optional[ClipboardResponse] = None
        swap(result, self.reply)
        return result^


def deliver_clipboard[
    Host: Clipboard
](
    mut host: Host,
    state: ClipboardState,
    set_clipboard: Optional[String] = None,
) raises -> ClipboardDelivery:
    var copied = List[UInt64]()
    for event in state.writes:
        var write = clipboard_write(
            event.target,
            event.producer,
            event.text.byte_length() == 0,
            set_clipboard,
            host.supports_primary(),
        )
        if write.clipboard:
            host.set_clipboard(event.text)
        if write.primary:
            host.set_primary_selection(event.text)
        if write.copy_flash:
            copied.append(event.pane)
    var reply: Optional[ClipboardResponse] = None
    if state.query:
        var text = host.get_clipboard_bounded(MAX_CLIPBOARD_REPLY_BYTES)
        if text.byte_length() > MAX_CLIPBOARD_REPLY_BYTES:
            raise Error("client clipboard exceeds paste buffer limit")
        reply = ClipboardResponse(
            state.query_server,
            state.query_client,
            clipboard_reply(text.as_bytes()),
        )
    return ClipboardDelivery(copied^, reply^)
