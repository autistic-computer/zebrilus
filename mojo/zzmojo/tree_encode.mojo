from .protocol import append_uint, append_text, envelope
from .tree_model import (
    TmuxColour,
    PaneKind,
    PaneSummary,
    PaneStatus,
    PaneSnapshot,
    LayoutTree,
    FloatingPane,
    ModalPane,
    WindowSnapshot,
    SessionViewer,
    SessionSnapshot,
    MuxSnapshot,
    TreeOp,
    TreeDelta,
    ClientView,
    _Order,
    _ordered,
)


def _bool(mut output: List[UInt8], value: Bool):
    output.append(UInt8(1 if value else 0))


def _optional_id(mut output: List[UInt8], value: Optional[UInt64]):
    _bool(output, Bool(value))
    if value:
        append_uint(output, value.value())


def _optional_text(mut output: List[UInt8], value: Optional[String]):
    _bool(output, Bool(value))
    if value:
        append_text(output, value.value())


def _optional_u32(mut output: List[UInt8], value: Optional[UInt32]):
    _bool(output, Bool(value))
    if value:
        append_uint(output, UInt64(value.value()))


def _ids(mut output: List[UInt8], values: List[UInt64]):
    append_uint(output, UInt64(len(values)))
    for value in values:
        append_uint(output, value)


def _strings(mut output: List[UInt8], values: List[String]):
    append_uint(output, UInt64(len(values)))
    for value in values:
        append_text(output, value)


def _colour(mut output: List[UInt8], value: TmuxColour):
    append_uint(output, UInt64(value.kind))
    if value.kind == 0 or value.kind == 1 or value.kind == 5:
        output.append(UInt8(value.value))
    elif value.kind == 2:
        append_uint(output, UInt64(value.value))


def _optional_colour(mut output: List[UInt8], value: Optional[TmuxColour]):
    _bool(output, Bool(value))
    if value:
        _colour(output, value.value())


def _kind(mut output: List[UInt8], value: PaneKind) raises:
    append_uint(output, UInt64(value.kind))
    if value.kind == 2:
        _strings(output, value.tabs)
        append_uint(output, value.active_tab)
        append_text(output, value.profile)
    elif value.kind == 3:
        _optional_text(output, value.path)
        if not value.cwd:
            raise Error("editor descriptor has no working directory")
        append_text(output, value.cwd.value())
    elif value.kind == 4:
        append_uint(output, UInt64(value.provider))
        _optional_text(output, value.cwd)
        _optional_text(output, value.session_id)
        _optional_text(output, value.model)
        _optional_text(output, value.effort)


def _summary(mut output: List[UInt8], value: PaneSummary):
    append_uint(output, value.changed_at)
    _bool(output, value.unseen)
    _bool(output, Bool(value.command))
    if value.command:
        append_uint(output, UInt64(value.command.value()))
        if value.command.value() == 1:
            output.append(value.command_exit)
    append_text(output, value.name)
    append_text(output, value.cwd)
    append_text(output, value.repo)
    append_text(output, value.branch)
    _strings(output, value.process)
    _bool(output, value.flip_armed)


def _status(mut output: List[UInt8], value: Optional[PaneStatus]):
    _bool(output, Bool(value))
    if not value:
        return
    var status = value.value().copy()
    append_uint(output, UInt64(status.state))
    _bool(output, Bool(status.kind))
    if status.kind:
        append_uint(output, UInt64(status.kind.value()))
    _bool(output, Bool(status.progress))
    if status.progress:
        output.append(status.progress.value())
    append_text(output, status.app)
    append_text(output, status.title)
    append_text(output, status.message)


def _mode(
    mut output: List[UInt8], kind: Optional[UInt8], bytes: List[UInt8]
) raises:
    if len(bytes) == 0:
        if kind:
            raise Error("pane mode has no retained wire value")
        output.append(0)
        return
    output.extend(Span(bytes))


def _pane(mut output: List[UInt8], pane: PaneSnapshot) raises:
    append_uint(output, pane.id)
    append_text(output, pane.title)
    _kind(output, pane.kind)
    _bool(output, pane.synchronized_input)
    _bool(output, pane.bell)
    _bool(output, pane.dead)
    _optional_u32(output, pane.dead_status)
    _optional_colour(output, pane.border_colour)
    _optional_colour(output, pane.active_border_colour)
    append_text(output, pane.border_status_text)
    _mode(output, pane.mode_kind, pane.mode_bytes)
    _status(output, pane.status)
    _summary(output, pane.summary)


@inline(.never)
def _layout_node(
    mut output: List[UInt8], layout: LayoutTree, index: Int, depth: Int
) raises:
    if depth >= 256 or index < 0 or index >= len(layout.nodes):
        raise Error("invalid layout arena")
    var node = layout.nodes[index]
    append_uint(output, UInt64(node.kind))
    if node.kind == 0:
        append_uint(output, node.id)
    elif node.kind == 1:
        append_uint(output, node.id)
        append_uint(output, UInt64(node.axis))
        var ratio = node.ratio
        var bits = Pointer(to=ratio).unsafe_bitcast[UInt32]()[]
        for shift in range(0, 32, 8):
            output.append(UInt8((bits >> UInt32(shift)) & 255))
        _layout_node(output, layout, node.first, depth + 1)
        _layout_node(output, layout, node.second, depth + 1)
    elif node.kind != 2:
        raise Error("unknown layout kind")


def _layout(mut output: List[UInt8], layout: LayoutTree) raises:
    if layout.root < 0 and len(layout.nodes) == 0:
        append_uint(output, 2)
        return
    _layout_node(output, layout, layout.root, 0)


def _floating(mut output: List[UInt8], values: List[FloatingPane]):
    append_uint(output, UInt64(len(values)))
    for pane in values:
        append_uint(output, pane.pane)
        append_uint(
            output, UInt64((Int64(pane.xoff) << 1) ^ (Int64(pane.xoff) >> 31))
        )
        append_uint(
            output, UInt64((Int64(pane.yoff) << 1) ^ (Int64(pane.yoff) >> 31))
        )
        append_uint(output, UInt64(pane.sx))
        append_uint(output, UInt64(pane.sy))
        _bool(output, pane.visible)
        append_uint(output, UInt64(pane.border_lines))
        append_uint(output, UInt64(pane.border_status))


def _modal(mut output: List[UInt8], value: Optional[ModalPane]):
    _bool(output, Bool(value))
    if value:
        var pane = value.value()
        append_uint(output, pane.pane)
        _bool(output, pane.capture_keys)
        _bool(output, pane.close_on_click)
        _bool(output, pane.close_on_cancel)


@inline(.never)
def _window(mut output: List[UInt8], window: WindowSnapshot) raises:
    append_uint(output, window.id)
    append_uint(output, UInt64(window.index))
    append_text(output, window.name)
    _bool(output, window.automatic_rename)
    append_uint(output, window.active_pane)
    _optional_id(output, window.zoomed_pane)
    _layout(output, window.layout)
    append_uint(output, UInt64(len(window.panes)))
    var keys = List[_Order]()
    for entry in window.panes.items():
        keys.append(_Order(entry.key, 0, 0))
    var sorted = _ordered(keys^)
    for key in sorted:
        append_uint(output, key.major)
        _pane(output, window.panes[key.major])
    append_text(output, window.layout_dump)
    append_text(output, window.visible_layout_dump)
    append_text(output, window.status_label)
    _bool(output, window.activity)
    _bool(output, window.silence)
    append_uint(output, UInt64(window.pane_border_status))
    append_uint(output, UInt64(window.pane_border_lines))
    append_uint(output, UInt64(window.pane_border_indicators))
    _ids(output, window.pane_order)
    _ids(output, window.pane_z_order)
    _floating(output, window.floating)
    _modal(output, window.modal)
    append_uint(output, UInt64(window.sx))
    append_uint(output, UInt64(window.sy))
    append_uint(output, UInt64(window.pane_base_index))


def _viewers(mut output: List[UInt8], values: List[SessionViewer]):
    append_uint(output, UInt64(len(values)))
    for viewer in values:
        append_text(output, viewer.name)
        append_uint(output, viewer.window)
        _bool(output, viewer.is_self)


def _session(mut output: List[UInt8], session: SessionSnapshot) raises:
    append_uint(output, session.id)
    append_text(output, session.name)
    append_uint(output, session.active_window)
    append_uint(output, UInt64(len(session.windows)))
    for window in session.windows:
        _window(output, window)
    _viewers(output, session.viewers)


def write_snapshot(mut output: List[UInt8], snapshot: MuxSnapshot) raises:
    append_uint(output, snapshot.generation)
    append_uint(output, UInt64(len(snapshot.sessions)))
    for session in snapshot.sessions:
        _session(output, session)
    _optional_id(output, snapshot.focused_window)


@inline(.never)
def write_tree_op(mut output: List[UInt8], op: TreeOp) raises:
    append_uint(output, UInt64(op.kind))
    if op.kind == 0:
        if not op.session_value:
            raise Error("missing session upsert value")
        _session(output, op.session_value.value())
        return
    append_uint(output, op.session)
    if op.kind == 1:
        return
    if op.kind == 2:
        append_text(output, op.name)
        return
    if op.kind == 3:
        append_uint(output, op.window)
        return
    if op.kind == 4:
        _viewers(output, op.viewers)
        return
    if op.kind == 5:
        if not op.window_value:
            raise Error("missing window upsert value")
        _window(output, op.window_value.value())
        return
    append_uint(output, op.window)
    if op.kind == 6:
        return
    if op.kind == 7:
        append_text(output, op.name)
        return
    if op.kind == 8:
        append_uint(output, op.pane)
        return
    if op.kind == 9:
        _layout(output, op.layout)
        _optional_id(output, op.zoomed_pane)
        append_text(output, op.layout_dump)
        append_text(output, op.visible_layout_dump)
        _ids(output, op.pane_order)
        _ids(output, op.pane_z_order)
        _floating(output, op.floating)
        _modal(output, op.modal)
        append_uint(output, UInt64(op.sx))
        append_uint(output, UInt64(op.sy))
        return
    if op.kind == 10:
        append_uint(output, UInt64(op.index))
        _bool(output, op.automatic_rename)
        _bool(output, op.activity)
        _bool(output, op.silence)
        append_uint(output, UInt64(op.pane_base_index))
        return
    if op.kind == 11:
        append_text(output, op.name)
        append_uint(output, UInt64(op.pane_border_status))
        append_uint(output, UInt64(op.pane_border_lines))
        append_uint(output, UInt64(op.pane_border_indicators))
        return
    if op.kind == 12:
        if not op.pane_value:
            raise Error("missing pane upsert value")
        _pane(output, op.pane_value.value())
        return
    append_uint(output, op.pane)
    if op.kind == 13:
        return
    if op.kind == 14:
        append_text(output, op.name)
        return
    if op.kind == 15:
        _kind(output, op.pane_kind)
        _bool(output, op.synchronized_input)
        _bool(output, op.bell)
        _bool(output, op.dead)
        _optional_u32(output, op.dead_status)
        return
    if op.kind == 16:
        _optional_colour(output, op.border_colour)
        _optional_colour(output, op.active_border_colour)
        append_text(output, op.name)
        _mode(output, op.mode_kind, op.mode_bytes)
        return
    if op.kind == 17:
        _status(output, op.status)
        return
    if op.kind == 18:
        _summary(output, op.summary)
        return
    raise Error("unknown tree operation")


def write_ops(mut output: List[UInt8], ops: List[TreeOp]) raises:
    append_uint(output, UInt64(len(ops)))
    for op in ops:
        write_tree_op(output, op)


def write_delta(mut output: List[UInt8], delta: TreeDelta) raises:
    append_uint(output, delta.base)
    append_uint(output, delta.version)
    write_ops(output, delta.ops)


def write_view(mut output: List[UInt8], view: ClientView) raises:
    _optional_id(output, view.session)
    _optional_id(output, view.focused_window)
    _bool(output, view.read_only)
    append_text(output, view.client_flags)
    append_uint(output, view.layout_generation)
    append_uint(output, view.attachment_generation)
    write_ops(output, view.overlay)


def snapshot_frame(
    snapshot: MuxSnapshot, sequence: UInt64 = 0
) raises -> List[UInt8]:
    var output: List[UInt8] = [10]
    append_uint(output, sequence)
    append_uint(output, 0)
    write_snapshot(output, snapshot)
    return envelope(Span(output))
