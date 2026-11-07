from .app_resume import ResumeStartup
from std.atomic import Atomic
from std.memory import OwnedPointer
from mjui.sys.dispatch import (
    DispatchRef,
    dispatch_async,
    dispatch_global_queue,
    dispatch_semaphore,
    dispatch_signal,
    dispatch_wait,
    dispatch_release,
)
from .state import AppModel
from .stream_transport import (
    InteractiveConnection as Connection,
    connect_interactive,
)
from .actions import Action, ActionQueue
from .geometry import GridSize
from .stream_protocol import resize_terminal, client_terminal_size, input_text
from .input_protocol import input_paste
from .terminal_commit import pasted_commit
from .actions import MAX_PENDING_INPUT_BYTES
from .clipboard_protocol import (
    ClipboardState,
    ClipboardResponse,
    MAX_CLIPBOARD_REPLY_BYTES,
)
from .search_ui_commands import SearchUiCommands
from .path_protocol import decode_path_message
from .path_service import PathSource, PathCommand
from .path_mailbox import PathInbox
from .exec_protocol import Invocation
from .palette_protocol import (
    palette_command_request,
    read_palette_command,
    palette_cancel_prefix,
)
from .prompt_protocol import PromptAction, prompt_request
from .chooser_protocol import ChooseAction, choose_request

comptime KEY = 1
comptime TEXT = 2
comptime CREATE_SESSION = 3
comptime SPLIT_RIGHT = 4
comptime SPLIT_DOWN = 5
comptime RECONNECT = 6
comptime RESIZE_TERMINAL = 7
comptime RESIZE_WINDOW = 8
comptime NATIVE_INPUT = 9
comptime NATIVE_PASSIVE = 10
comptime HISTORY_PREFETCH = 11
comptime PASTE = 12
comptime OUTPUT_TEXT = 13
comptime OUTPUT_PASTE = 14
comptime PATH_CONTROL = 15
comptime PALETTE_COMMAND = 16
comptime PALETTE_OPEN = 17


struct Job(Movable):
    var path: String
    var startup: Optional[ResumeStartup]
    var connection: Optional[Connection]
    var model: AppModel
    var actions: List[Action]
    var epoch: UInt64
    var done: Atomic[Int64]
    var semaphore: DispatchRef
    var clipboard_reply: Optional[ClipboardResponse]
    var path_epoch: UInt64

    def __init__(
        out self, path: String, var startup: Optional[ResumeStartup] = None
    ):
        self.path = path
        self.startup = startup^
        self.connection = None
        self.model = AppModel()
        self.actions = List[Action]()
        self.epoch = 0
        self.done = Atomic[Int64](1)
        self.semaphore = dispatch_semaphore()
        self.clipboard_reply = None
        self.path_epoch = 0


def run_job(context: MutOpaquePointer[MutUntrackedOrigin]) abi("C"):
    var pointer = context.unsafe_bitcast[Job]()
    ref job = pointer[]
    try:
        if not job.connection:
            job.path_epoch += 1
            var session = (
                "$"
                + String(
                    job.model.session.value()
                ) if job.model.session else String()
            )
            if job.startup:
                var startup = job.startup.take()
                job.connection = startup^.connect(
                    native_paths=True, path_epoch=job.path_epoch
                )
            else:
                job.connection = connect_interactive(
                    job.path,
                    session,
                    native_search=True,
                    native_paths=True,
                    path_epoch=job.path_epoch,
                )
        if job.clipboard_reply:
            ref reply = job.clipboard_reply.value()
            if (
                job.connection.value().server_id == reply.server
                and job.connection.value().client_id == reply.client
            ):
                job.connection.value().transport.write(Span(reply.frame))
        job.clipboard_reply = None
        job.model.refresh(job.connection.value())
        job.model.sync_focus(job.connection.value())
        var desired_session = job.model.session
        var desired_window = job.model.window
        var desired_pane = job.model.pane
        var restore_selection = False
        var command_error: Optional[String] = None
        for action in job.actions:
            if action.source:
                var source = action.source.value()
                if (
                    source[0] != job.connection.value().server_id
                    or source[1] != job.connection.value().client_id
                ):
                    continue
            if (
                action.output_owner
                and job.connection.value().model.events.output.id
                != action.output_owner
            ):
                continue
            if action.kind == PALETTE_OPEN:
                if not action.source:
                    continue
                var request = job.connection.value().transport.next_request
                if request == UInt64.MAX:
                    raise Error("request identifiers exhausted")
                job.connection.value().transport.next_request += 1
                var frame = palette_cancel_prefix(request)
                job.connection.value().transport.write(Span(frame))
                if job.connection.value().model.dialogs.prompt:
                    frame = prompt_request(PromptAction(2))
                    job.connection.value().transport.write(Span(frame))
                if job.connection.value().model.dialogs.chooser:
                    frame = choose_request(ChooseAction(17))
                    job.connection.value().transport.write(Span(frame))
                continue
            if action.kind == PALETTE_COMMAND:
                if not action.source:
                    continue
                var command = read_palette_command(action.frame.copy())
                var response = job.connection.value().command_invocation(
                    command.command
                )
                if response.exit_code != 0:
                    command_error = (
                        response.error if response.error.byte_length()
                        > 0 else String("command failed")
                    )
                restore_selection = (
                    restore_selection or action.selection_epoch != job.epoch
                )
                continue
            if action.kind == PATH_CONTROL:
                if not action.source:
                    continue
                var source = action.source.value()
                var captured = PathSource(
                    source[0], source[1], action.selection_epoch
                )
                var message = decode_path_message(action.frame.copy())
                if not message:
                    raise Error("path action has no decoded request")
                if job.connection.value().model.paths.send(
                    captured, message.value()
                ):
                    job.connection.value().transport.write(Span(action.frame))
                continue
            if action.kind == HISTORY_PREFETCH:
                if action.pane:
                    job.connection.value().request_history(
                        action.pane.value(), UInt32(Int(action.text))
                    )
                continue
            if action.kind == RESIZE_TERMINAL or action.kind == RESIZE_WINDOW:
                if action.geometry:
                    var size = action.geometry.value()
                    var frame = client_terminal_size(
                        size.columns, size.rows, size.generation
                    )
                    if action.kind == RESIZE_TERMINAL and action.pane:
                        frame = resize_terminal(
                            action.pane.value(),
                            size.columns,
                            size.rows,
                            size.cell_width,
                            size.cell_height,
                            size.generation,
                        )
                    job.connection.value().transport.write(Span(frame))
                continue
            restore_selection = (
                restore_selection or action.selection_epoch != job.epoch
            )
            if action.kind == NATIVE_PASSIVE:
                job.connection.value().transport.write(Span(action.frame))
                continue
            if action.kind == OUTPUT_TEXT or action.kind == OUTPUT_PASTE:
                if action.pane:
                    var frame: List[UInt8]
                    if action.kind == OUTPUT_PASTE:
                        frame = input_paste(
                            action.pane.value(), action.text, True
                        )
                    else:
                        frame = input_text(action.pane.value(), action.text)
                    job.connection.value().transport.write(Span(frame))
                continue
            if action.kind == RECONNECT:
                job.path_epoch += 1
                var session = (
                    "$"
                    + String(
                        job.model.session.value()
                    ) if job.model.session else String()
                )
                job.connection = connect_interactive(
                    job.path,
                    session,
                    native_search=True,
                    native_paths=True,
                    path_epoch=job.path_epoch,
                )
            elif action.kind == CREATE_SESSION:
                job.model.create_session(job.connection.value())
            elif action.kind == SPLIT_RIGHT or action.kind == SPLIT_DOWN:
                job.model.pane = action.pane
                job.model.focus_dirty = True
                job.model.sync_focus(job.connection.value())
                job.model.split(
                    job.connection.value(),
                    horizontal=action.kind == SPLIT_RIGHT,
                )
            elif (
                action.kind == KEY
                or action.kind == TEXT
                or action.kind == PASTE
                or action.kind == NATIVE_INPUT
            ):
                job.model.pane = action.pane
                job.model.focus_dirty = True
                job.model.sync_focus(job.connection.value())
                if action.kind == NATIVE_INPUT:
                    job.connection.value().transport.write(Span(action.frame))
                elif action.kind == PASTE and action.pane:
                    var frame = input_paste(action.pane.value(), action.text)
                    job.connection.value().transport.write(Span(frame))
                else:
                    job.model.send_key(
                        job.connection.value(),
                        action.text,
                        literal=action.kind == TEXT,
                    )
        if restore_selection:
            job.model.session = desired_session
            job.model.window = desired_window
            job.model.pane = desired_pane
            job.model.focus_dirty = True
        job.model.sync_focus(job.connection.value())
        job.model.refresh(job.connection.value())
        if command_error:
            job.model.error = command_error.value()
    except error:
        job.model.error = String(error)
        job.connection = None
    job.actions.clear()
    job.clipboard_reply = None
    job.done.store(1)
    dispatch_signal(job.semaphore)


struct AsyncBackend(Movable):
    var job: OwnedPointer[Job]
    var queue: ActionQueue
    var busy: Bool
    var online: Bool
    var clipboard: ClipboardState
    var clipboard_reply: Optional[ClipboardResponse]
    var source: Optional[Tuple[UInt64, UInt64]]
    var search_commands: SearchUiCommands
    var paths: PathInbox
    var path_source: Optional[PathSource]

    def __init__(
        out self, path: String, var startup: Optional[ResumeStartup] = None
    ):
        self.job = OwnedPointer(Job(path, startup^))
        self.queue = ActionQueue()
        self.busy = False
        self.online = False
        self.clipboard = ClipboardState()
        self.clipboard_reply = None
        self.source = None
        self.search_commands = SearchUiCommands()
        self.paths = PathInbox()
        self.path_source = None

    def __deinit__(deinit self):
        if self.busy:
            dispatch_wait(self.job[].semaphore)
        dispatch_release(self.job[].semaphore)

    def enqueue(
        mut self,
        kind: Int,
        pane: Optional[UInt64] = None,
        text: String = "",
        selection_epoch: UInt64 = 0,
    ) raises:
        self.queue.append(kind, text, pane, selection_epoch)

    def ready_fd(self) -> Int32:
        if self.busy or not self.job[].connection:
            return -1
        if self.job[].connection.value().model.server_stopped:
            return -1
        return self.job[].connection.value().transport.fd

    def pending(self) -> Bool:
        return len(self.queue) > 0 or Bool(self.clipboard_reply)

    def path(mut self, command: PathCommand) raises -> Bool:
        if not self.path_source or not self.path_source.value().matches(
            command.source
        ):
            return False
        self.queue.append(
            PATH_CONTROL,
            "",
            None,
            command.source.epoch,
            frame=command.frame,
            source=(command.source.server, command.source.client),
        )
        return True

    def palette_open(
        mut self, epoch: UInt64, source: Tuple[UInt64, UInt64]
    ) raises:
        self.queue.append(PALETTE_OPEN, "", None, epoch, source=source)

    def palette_command(
        mut self,
        command: Invocation,
        epoch: UInt64,
        source: Tuple[UInt64, UInt64],
    ) raises:
        self.queue.append(
            PALETTE_COMMAND,
            "",
            None,
            epoch,
            frame=palette_command_request(0, command),
            source=source,
        )

    def reply_clipboard(
        mut self, var reply: Optional[ClipboardResponse]
    ) raises:
        if not reply:
            return
        if len(reply.value().frame) > MAX_CLIPBOARD_REPLY_BYTES + 20:
            raise Error("pending clipboard reply exceeds byte limit")
        self.clipboard_reply = reply^

    def history_prefetch(
        mut self, pane: UInt64, target: UInt32, epoch: UInt64
    ) raises:
        self.queue.append(HISTORY_PREFETCH, String(target), pane, epoch)

    def next_history_wake(self, time: Float64) -> Float64:
        if self.busy or not self.job[].connection:
            return Float64.MAX
        var wait = self.job[].connection.value().history_wait_ns()
        return (
            time
            + Float64(wait.value()) / 1_000_000_000 if wait else Float64.MAX
        )

    def resize(
        mut self, pane: Optional[UInt64], size: GridSize, epoch: UInt64
    ) raises:
        self.queue.append(
            RESIZE_TERMINAL if pane else RESIZE_WINDOW, "", pane, epoch, size
        )

    def input(
        mut self,
        pane: UInt64,
        frame: List[UInt8],
        epoch: UInt64,
        passive: Bool = False,
        source: Optional[Tuple[UInt64, UInt64]] = None,
        output_owner: Optional[UInt64] = None,
    ) raises:
        self.queue.append(
            NATIVE_PASSIVE if passive else NATIVE_INPUT,
            "",
            pane,
            epoch,
            frame=frame,
            source=source if source else self.source,
            output_owner=output_owner,
        )

    def paste(mut self, pane: UInt64, text: String, epoch: UInt64, source: Tuple[UInt64, UInt64]) raises:
        if text.byte_length() > MAX_PENDING_INPUT_BYTES:
            raise Error("pending terminal paste exceeds queue limit")
        if text.byte_length() > 0:
            self.queue.append(PASTE, text, pane, epoch, source=source)

    def commit(
        mut self,
        pane: UInt64,
        text: String,
        epoch: UInt64,
        composed: Bool = False,
        command_output: Bool = False,
        source: Optional[Tuple[UInt64, UInt64]] = None,
    ) raises:
        if text.byte_length() == 0:
            return
        if text.byte_length() > MAX_PENDING_INPUT_BYTES:
            raise Error("pending terminal input exceeds queue limit")
        var paste = pasted_commit(text, composed)
        var kind = PASTE if paste else TEXT
        if command_output:
            kind = OUTPUT_PASTE if paste else OUTPUT_TEXT
        self.queue.append(
            kind, text, pane, epoch, source=source if source else self.source
        )

    def submit(mut self, model: AppModel, epoch: UInt64):
        if self.busy:
            return
        self.job[].model.focus_dirty = (
            model.focus_dirty
            or self.job[].model.session != model.session
            or self.job[].model.window != model.window
            or self.job[].model.pane != model.pane
        )
        self.job[].model.session = model.session
        self.job[].model.window = model.window
        self.job[].model.pane = model.pane
        self.job[].epoch = epoch
        self.job[].actions = self.queue.take()
        swap(self.job[].clipboard_reply, self.clipboard_reply)
        self.job[].done.store(0)
        self.busy = True
        dispatch_async(
            dispatch_global_queue(),
            self.job.ptr().unsafe_bitcast[NoneType](),
            run_job,
        )

    def collect(
        mut self, mut model: AppModel, epoch: UInt64, wait: Bool = False
    ) -> Bool:
        if not self.busy:
            return False
        if not wait and self.job[].done.load() == 0:
            return False
        dispatch_wait(self.job[].semaphore)
        self.busy = False
        self.online = Bool(self.job[].connection) and not self.job[].connection.value().model.server_stopped
        var session = model.session
        var window = model.window
        var pane = model.pane
        var focus_dirty = model.focus_dirty
        model = self.job[].model.copy()
        self.source = (
            model.source_server,
            model.source_client,
        ) if model.generation != 0 else Optional[Tuple[UInt64, UInt64]](None)
        if epoch != self.job[].epoch:
            model.session = session
            model.window = window
            model.pane = pane
            model.focus_dirty = focus_dirty
            model.reconcile()
            try:
                model.capture()
            except error:
                model.error = String(error)
        if self.job[].connection:
            try:
                var source = (
                    self.job[].connection.value().model.paths.registry.source
                )
                if self.path_source and not self.path_source.value().matches(
                    source
                ):
                    self.paths.reset(self.path_source.value())
                self.clipboard.merge(
                    self.job[].connection.value().model.clipboard.take()
                )
                self.search_commands.merge(
                    self.job[].connection.value().model.search_commands.take()
                )
                self.paths.merge(
                    self.job[].connection.value().model.paths.inbox.take()
                )
                self.path_source = source
            except error:
                model.error = String(error)
                self.job[].connection = None
                self.online = False
                try:
                    if self.path_source:
                        self.paths.reset(self.path_source.value())
                except:
                    pass
                self.path_source = None
        elif self.path_source:
            try:
                self.paths.reset(self.path_source.value())
            except error:
                model.error = String(error)
            self.path_source = None
        return True
