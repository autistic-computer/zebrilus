from mjui.input import (
    Clipboard,
    KeyEvent,
)
from mjui.ui import Ui
from .view import WorkspaceView, TERMINAL_KEY
from .state import AppModel
from .keyboard_controller import KeyboardController
from .clipboard_host import deliver_clipboard
from .clipboard_keys import clipboard_shortcut
from .clipboard_upload import (
    paste_upload_begin,
    paste_upload_chunk,
    MAX_PASTE_UPLOAD_BYTES,
    MAX_PASTE_UPLOAD_CHUNK_BYTES,
    MAX_PASTE_UPLOADS,
)
from .input_protocol import mjui_key_input, input_copy_selection, input_view
from .actions import MAX_PENDING_INPUT_BYTES


@fieldwise_init
struct ClipboardKeyResult(TrivialRegisterPassable):
    var handled: Bool
    var unmark: Bool


@fieldwise_init
struct ClipboardPlan(Copyable, Movable):
    var action: UInt8
    var pane: UInt64
    var epoch: UInt64
    var output: Bool
    var output_id: Optional[UInt64]
    var server: UInt64
    var client: UInt64
    var composed: Bool


@fieldwise_init
struct _Upload(Copyable, Movable):
    var pane: UInt64
    var epoch: UInt64
    var bytes: List[UInt8]
    var offset: Int
    var next_due: Float64


def clipboard_modal(model: AppModel) -> Bool:
    if not model.session or not model.window:
        return False
    for session in model.snapshot.sessions:
        if session.id == model.session.value():
            for window in session.windows:
                if window.id == model.window.value():
                    return Bool(window.modal)
    return False


struct ClipboardController(Movable):
    var uploads: Dict[UInt64, _Upload]
    var server: UInt64
    var client: UInt64
    var next_upload: UInt64

    def __init__(out self):
        self.uploads = Dict[UInt64, _Upload]()
        self.server = 0
        self.client = 0
        self.next_upload = 1

    def observe(mut self, model: AppModel):
        if (
            self.server != model.source_server
            or self.client != model.source_client
        ):
            self.uploads.clear()
            self.server = model.source_server
            self.client = model.source_client

    def deliver[
        Host: Clipboard
    ](mut self, mut view: WorkspaceView, mut ui: Ui, mut host: Host) raises:
        var setting: Optional[String] = None
        if 5 in view.model.routing.options:
            setting = view.model.routing.options[5].value.copy()
        var delivery = deliver_clipboard(
            host, view.backend.clipboard.take(), setting
        )
        view.backend.reply_clipboard(delivery.take_reply())
        if view.backend.pending():
            view.next_poll = 0
            ui.request_wake(ui.time)

    def plan(
        mut self,
        view: WorkspaceView,
        ui: Ui,
        mut keyboard: KeyboardController,
        key: KeyEvent,
        bytes: Span[UInt8, _],
        composed: Bool = False,
    ) raises -> Optional[ClipboardPlan]:
        self.observe(view.model)
        var pane = view.input_pane()
        if (
            key.release
            or key.consumed
            or not view.active
            or ui.focus != TERMINAL_KEY
            or not pane
            or not view.model.session
        ):
            return None
        var input = mjui_key_input(
            key.key, key.ch, key.unshifted, key.mods, key.action()
        )
        if input.key == 16:
            return None
        var output = view.output_input and view.output.owns(pane.value())
        var prefix = not output and view.model.routing.claims_prefix_input(
            input
        )
        var action = clipboard_shortcut(
            input, prefix, clipboard_modal(view.model)
        )
        if action == 0:
            return None
        keyboard.claim_text(key, bytes)
        return ClipboardPlan(
            action,
            pane.value(),
            view.focus_epoch,
            output,
            view.output.id if output else Optional[UInt64](None),
            view.model.source_server,
            view.model.source_client,
            composed,
        )

    def act[
        Host: Clipboard
    ](
        mut self,
        mut view: WorkspaceView,
        mut ui: Ui,
        host: Host,
        action: UInt8,
        composed: Bool = False,
        plan: Optional[ClipboardPlan] = None,
    ) raises -> ClipboardKeyResult:
        if plan and (
            plan.value().server != view.model.source_server
            or plan.value().client != view.model.source_client
            or (
                plan.value().output and plan.value().output_id != view.output.id
            )
        ):
            return ClipboardKeyResult(True, False)
        var pane = Optional[UInt64](
            plan.value().pane
        ) if plan else view.input_pane()
        if not pane:
            return ClipboardKeyResult(False, False)
        var output = plan.value().output if plan else (
            view.output_input and view.output.owns(pane.value())
        )
        var epoch = plan.value().epoch if plan else view.focus_epoch
        var unmark = False
        if action == 2:
            if not output and host.has_clipboard_image():
                var image = host.get_clipboard_image(MAX_PASTE_UPLOAD_BYTES + 1)
                if len(image) == 0 or len(image) > MAX_PASTE_UPLOAD_BYTES:
                    raise Error("Paste a PNG image of 6 MiB or less.")
                if len(self.uploads) >= MAX_PASTE_UPLOADS:
                    raise Error("too many image pastes in flight (limit 2)")
                if self.next_upload == UInt64.MAX:
                    raise Error("paste upload identifiers exhausted")
                var id = self.next_upload
                self.next_upload += 1
                var frame = paste_upload_begin(
                    id, pane.value(), "png", UInt32(len(image))
                )
                view.backend.input(
                    pane.value(),
                    frame,
                    epoch,
                    True,
                    (self.server, self.client),
                )
                self.uploads[id] = _Upload(
                    pane.value(), epoch, image^, 0, ui.time
                )
            else:
                var text = host.get_clipboard_bounded(MAX_PENDING_INPUT_BYTES)
                if text.byte_length() > 0:
                    if not output:
                        _ = view.cancel_local_scroll(pane.value())
                    view.backend.commit(
                        pane.value(), text, epoch, composed, output
                    )
                    view.marked = None
                    unmark = True
        else:
            if not output:
                view.flush_local_scroll(pane.value())
            var frame = input_copy_selection(
                pane.value(), 1, 0, output
            ) if action == 1 else input_view(
                pane.value(), 12, command_output=output
            )
            view.backend.input(pane.value(), frame, epoch, output)
        view.next_poll = 0
        ui.request_wake(ui.time)
        return ClipboardKeyResult(True, unmark)

    def tick(mut self, mut view: WorkspaceView, mut ui: Ui) raises:
        self.observe(view.model)
        var retired = List[UInt64]()
        var next_due = Float64.MAX
        var ids = List[UInt64]()
        for entry in self.uploads.items():
            ids.append(entry.key)
        for id in ids:
            ref upload = self.uploads[id]
            if ui.time >= upload.next_due:
                var end = min(
                    upload.offset + MAX_PASTE_UPLOAD_CHUNK_BYTES,
                    len(upload.bytes),
                )
                if (
                    end - upload.offset + 32
                    <= MAX_PENDING_INPUT_BYTES
                    - view.backend.queue.pending_bytes
                ):
                    var frame = paste_upload_chunk(
                        id, Span(upload.bytes)[upload.offset : end]
                    )
                    view.backend.input(
                        upload.pane,
                        frame,
                        upload.epoch,
                        True,
                        (self.server, self.client),
                    )
                    upload.offset = end
                    view.next_poll = 0
                    ui.request_wake(ui.time)
                upload.next_due = ui.time + 0.01
            if upload.offset == len(upload.bytes):
                retired.append(id)
            else:
                next_due = min(next_due, upload.next_due)
        for id in retired:
            _ = self.uploads.pop(id)
        if next_due != Float64.MAX:
            ui.request_wake(next_due)
