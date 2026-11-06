from .tree_model import (
    PaneSnapshot, WindowSnapshot, SessionSnapshot, MuxSnapshot,
    TreeOp, TreeDelta, ClientView, _Order, _ordered,
)

comptime MAX_TREE_BYTES = 64 * 1024 * 1024
comptime MAX_LIVE_TREE_BYTES = 2 * 1024 * 1024
comptime SESSION_NAME: UInt8 = 0
comptime SESSION_VIEWERS: UInt8 = 1
comptime WINDOW_NAME: UInt8 = 2
comptime WINDOW_LAYOUT: UInt8 = 3
comptime WINDOW_FLAGS: UInt8 = 4
comptime WINDOW_PRESENTATION: UInt8 = 5
comptime PANE_TITLE: UInt8 = 6
comptime PANE_STATE: UInt8 = 7
comptime PANE_PRESENTATION: UInt8 = 8
comptime PANE_STATUS: UInt8 = 9
comptime PANE_SUMMARY: UInt8 = 10


@fieldwise_init
struct TreeLocator(TrivialRegisterPassable):
    var group: UInt8
    var source: UInt8
    var operation: UInt32
    var session: UInt32
    var window: UInt32
    var pane: UInt64


def _with_group(value: TreeLocator, group: UInt8) -> TreeLocator:
    var result = value
    result.group = group
    return result


def _window_location(value: TreeLocator, window: Int) -> TreeLocator:
    var result = value
    result.window = UInt32(window)
    return result


def _pane_location(value: TreeLocator, pane: UInt64) -> TreeLocator:
    var result = value
    result.pane = pane
    return result


struct TreeArena(Movable):
    var entries: List[TreeLocator]

    def __init__(out self):
        self.entries = List[TreeLocator]()

    def add(mut self, location: TreeLocator, group: UInt8) -> UInt32:
        var index = UInt32(len(self.entries))
        self.entries.append(_with_group(location, group))
        return index

    def locate(self, index: UInt32, group: UInt8) raises -> TreeLocator:
        if UInt64(index) >= UInt64(len(self.entries)):
            raise Error("Bend returned an invalid tree arena reference")
        var result = self.entries[Int(index)]
        if result.group != group:
            raise Error("Bend returned a tree reference from another semantic group")
        return result


def _window_words(value: WindowSnapshot, maximum: Int) -> Int:
    if len(value.panes) > maximum // 9:
        return maximum + 1
    return 10 + len(value.panes) * 9


def _session_words(value: SessionSnapshot, maximum: Int) -> Int:
    var count = 7
    for window in value.windows:
        count += _window_words(window, maximum)
        if count > maximum:
            break
    return count


def normalized_words(
    snapshot: MuxSnapshot, ops: List[TreeOp], mode: UInt8,
    focus: Optional[UInt64] = None, maximum: Int = MAX_LIVE_TREE_BYTES // 4,
) -> Int:
    if mode > 2 or maximum < 0:
        return maximum + 1
    var count = 4 + (2 if snapshot.focused_window else 0)
    for session in snapshot.sessions:
        count += _session_words(session, maximum)
        if count > maximum:
            return maximum + 1
    count += 5 if mode == 0 else 1 if mode == 1 else 2 + (2 if focus else 0)
    for op in ops:
        if op.kind == 0:
            if not op.session_value:
                return maximum + 1
            count += 1 + _session_words(op.session_value.value(), maximum)
        elif op.kind == 5:
            if not op.window_value:
                return maximum + 1
            count += 3 + _window_words(op.window_value.value(), maximum)
        elif op.kind == 12:
            if not op.pane_value:
                return maximum + 1
            count += 12
        elif op.kind == 1:
            count += 3
        elif op.kind == 2 or op.kind == 4:
            count += 4
        elif op.kind == 3 or op.kind == 6:
            count += 5
        elif op.kind == 7 or op.kind == 9 or op.kind == 11:
            count += 6
        elif op.kind == 8 or op.kind == 10 or op.kind == 13:
            count += 7
        elif op.kind >= 14 and op.kind <= 18:
            count += 8
        else:
            return maximum + 1
        if count > maximum:
            return maximum + 1
    return count


def eligible_delta(snapshot: MuxSnapshot, delta: TreeDelta) -> Bool:
    return normalized_words(snapshot, delta.ops, 0) <= MAX_LIVE_TREE_BYTES // 4


def eligible_overlay(snapshot: MuxSnapshot, ops: List[TreeOp]) -> Bool:
    return normalized_words(snapshot, ops, 1) <= MAX_LIVE_TREE_BYTES // 4


def eligible_view(snapshot: MuxSnapshot, view: ClientView) -> Bool:
    return normalized_words(snapshot, view.overlay, 2, view.focused_window) <= MAX_LIVE_TREE_BYTES // 4


def _id(mut words: List[UInt32], value: UInt64):
    words.append(UInt32(value >> 32))
    words.append(UInt32(value & 0xffffffff))


def _optional_id(mut words: List[UInt32], value: Optional[UInt64]):
    words.append(UInt32(1 if value else 0))
    if value:
        _id(words, value.value())


def _pane(mut words: List[UInt32], mut arena: TreeArena, value: PaneSnapshot, location: TreeLocator):
    _id(words, value.id)
    words.append(arena.add(location, PANE_TITLE))
    words.append(arena.add(location, PANE_STATE))
    words.append(arena.add(location, PANE_PRESENTATION))
    words.append(arena.add(location, PANE_STATUS))
    words.append(arena.add(location, PANE_SUMMARY))


def _window(mut words: List[UInt32], mut arena: TreeArena, value: WindowSnapshot, location: TreeLocator) raises:
    _id(words, value.id)
    words.append(value.index)
    words.append(arena.add(location, WINDOW_NAME))
    _id(words, value.active_pane)
    words.append(arena.add(location, WINDOW_LAYOUT))
    words.append(arena.add(location, WINDOW_FLAGS))
    words.append(arena.add(location, WINDOW_PRESENTATION))
    words.append(UInt32(len(value.panes)))
    var keys = List[_Order](capacity=len(value.panes))
    for entry in value.panes.items():
        keys.append(_Order(entry.key, 0, 0))
    var sorted = _ordered(keys^)
    for key in sorted:
        _id(words, key.major)
        _pane(words, arena, value.panes[key.major], _pane_location(location, key.major))


def _session(mut words: List[UInt32], mut arena: TreeArena, value: SessionSnapshot, location: TreeLocator) raises:
    _id(words, value.id)
    words.append(arena.add(location, SESSION_NAME))
    _id(words, value.active_window)
    words.append(arena.add(location, SESSION_VIEWERS))
    words.append(UInt32(len(value.windows)))
    for index in range(len(value.windows)):
        _window(words, arena, value.windows[index], _window_location(location, index))


def _snapshot(mut words: List[UInt32], mut arena: TreeArena, snapshot: MuxSnapshot) raises:
    _id(words, snapshot.generation)
    _optional_id(words, snapshot.focused_window)
    words.append(UInt32(len(snapshot.sessions)))
    for index in range(len(snapshot.sessions)):
        _session(words, arena, snapshot.sessions[index], TreeLocator(0, 0, 0, UInt32(index), 0, 0))


def _operation(mut words: List[UInt32], mut arena: TreeArena, op: TreeOp, index: Int) raises:
    var location = TreeLocator(0, 4, UInt32(index), 0, 0, 0)
    words.append(UInt32(op.kind))
    if op.kind == 0:
        if not op.session_value:
            raise Error("session upsert has no value")
        location.source = 1
        _session(words, arena, op.session_value.value(), location)
        return
    _id(words, op.session)
    if op.kind == 1:
        return
    if op.kind == 2 or op.kind == 4:
        words.append(arena.add(location, SESSION_NAME if op.kind == 2 else SESSION_VIEWERS))
        return
    if op.kind == 5:
        if not op.window_value:
            raise Error("window upsert has no value")
        location.source = 2
        _window(words, arena, op.window_value.value(), location)
        return
    _id(words, op.window)
    if op.kind == 3 or op.kind == 6:
        return
    if op.kind == 7 or op.kind == 9 or op.kind == 11:
        var group = WINDOW_NAME if op.kind == 7 else WINDOW_LAYOUT if op.kind == 9 else WINDOW_PRESENTATION
        words.append(arena.add(location, group))
        return
    if op.kind == 10:
        words.append(op.index)
        words.append(arena.add(location, WINDOW_FLAGS))
        return
    if op.kind == 12:
        if not op.pane_value:
            raise Error("pane upsert has no value")
        location.source = 3
        _pane(words, arena, op.pane_value.value(), location)
        return
    _id(words, op.pane)
    if op.kind == 8 or op.kind == 13:
        return
    if op.kind >= 14 and op.kind <= 18:
        words.append(arena.add(location, UInt8(PANE_TITLE + op.kind - 14)))
        return
    raise Error("unknown tree operation")


struct TreeWords(Movable):
    var current: List[UInt32]
    var update: List[UInt32]
    var arena: TreeArena

    def __init__(out self):
        self.current = List[UInt32]()
        self.update = List[UInt32]()
        self.arena = TreeArena()


def normalize(
    snapshot: MuxSnapshot, ops: List[TreeOp], mode: UInt8,
    base: UInt64 = 0, version: UInt64 = 0, focus: Optional[UInt64] = None,
    maximum: Int = MAX_LIVE_TREE_BYTES // 4,
) raises -> TreeWords:
    if maximum > (MAX_TREE_BYTES - 64) // 4 or normalized_words(snapshot, ops, mode, focus, maximum) > maximum:
        raise Error("normalized tree input exceeds its byte budget")
    var result = TreeWords()
    _snapshot(result.current, result.arena, snapshot)
    if mode == 0:
        _id(result.update, base)
        _id(result.update, version)
    elif mode == 2:
        _optional_id(result.update, focus)
    result.update.append(UInt32(len(ops)))
    for index in range(len(ops)):
        _operation(result.update, result.arena, ops[index], index)
    return result^


def _copy_window(mut target: WindowSnapshot, source: WindowSnapshot, group: UInt8):
    if group == 2:
        target.name = source.name
    elif group == 3:
        target.layout = source.layout.copy()
        target.zoomed_pane = source.zoomed_pane
        target.layout_dump = source.layout_dump
        target.visible_layout_dump = source.visible_layout_dump
        target.pane_order = source.pane_order.copy()
        target.pane_z_order = source.pane_z_order.copy()
        target.floating = source.floating.copy()
        target.modal = source.modal
        target.sx = source.sx
        target.sy = source.sy
    elif group == 4:
        target.automatic_rename = source.automatic_rename
        target.activity = source.activity
        target.silence = source.silence
        target.pane_base_index = source.pane_base_index
    elif group == 5:
        target.status_label = source.status_label
        target.pane_border_status = source.pane_border_status
        target.pane_border_lines = source.pane_border_lines
        target.pane_border_indicators = source.pane_border_indicators


def _copy_window_op(mut target: WindowSnapshot, source: TreeOp, group: UInt8):
    if group == 2:
        target.name = source.name
    elif group == 3:
        target.layout = source.layout.copy()
        target.zoomed_pane = source.zoomed_pane
        target.layout_dump = source.layout_dump
        target.visible_layout_dump = source.visible_layout_dump
        target.pane_order = source.pane_order.copy()
        target.pane_z_order = source.pane_z_order.copy()
        target.floating = source.floating.copy()
        target.modal = source.modal
        target.sx = source.sx
        target.sy = source.sy
    elif group == 4:
        target.automatic_rename = source.automatic_rename
        target.activity = source.activity
        target.silence = source.silence
        target.pane_base_index = source.pane_base_index
    elif group == 5:
        target.status_label = source.name
        target.pane_border_status = source.pane_border_status
        target.pane_border_lines = source.pane_border_lines
        target.pane_border_indicators = source.pane_border_indicators


def _copy_pane(mut target: PaneSnapshot, source: PaneSnapshot, group: UInt8):
    if group == 6:
        target.title = source.title
    elif group == 7:
        target.kind = source.kind.copy()
        target.synchronized_input = source.synchronized_input
        target.bell = source.bell
        target.dead = source.dead
        target.dead_status = source.dead_status
    elif group == 8:
        target.border_colour = source.border_colour
        target.active_border_colour = source.active_border_colour
        target.border_status_text = source.border_status_text
        target.mode_kind = source.mode_kind
        target.mode_bytes = source.mode_bytes.copy()
    elif group == 9:
        target.status = source.status.copy()
    elif group == 10:
        target.summary = source.summary.copy()


def _copy_pane_op(mut target: PaneSnapshot, source: TreeOp, group: UInt8):
    if group == 6:
        target.title = source.name
    elif group == 7:
        target.kind = source.pane_kind.copy()
        target.synchronized_input = source.synchronized_input
        target.bell = source.bell
        target.dead = source.dead
        target.dead_status = source.dead_status
    elif group == 8:
        target.border_colour = source.border_colour
        target.active_border_colour = source.active_border_colour
        target.border_status_text = source.name
        target.mode_kind = source.mode_kind
        target.mode_bytes = source.mode_bytes.copy()
    elif group == 9:
        target.status = source.status.copy()
    elif group == 10:
        target.summary = source.summary.copy()


def _restore_session(mut target: SessionSnapshot, location: TreeLocator, snapshot: MuxSnapshot, ops: List[TreeOp]) raises:
    if location.source == 0:
        if location.group == SESSION_NAME:
            target.name = snapshot.sessions[Int(location.session)].name
        else:
            target.viewers = snapshot.sessions[Int(location.session)].viewers.copy()
    elif location.source == 1:
        if location.group == SESSION_NAME:
            target.name = ops[Int(location.operation)].session_value.value().name
        else:
            target.viewers = ops[Int(location.operation)].session_value.value().viewers.copy()
    elif location.source == 4:
        if location.group == SESSION_NAME:
            target.name = ops[Int(location.operation)].name
        else:
            target.viewers = ops[Int(location.operation)].viewers.copy()
    else:
        raise Error("invalid tree session metadata source")


def _restore_window(mut target: WindowSnapshot, location: TreeLocator, snapshot: MuxSnapshot, ops: List[TreeOp]) raises:
    if location.source == 0:
        _copy_window(target, snapshot.sessions[Int(location.session)].windows[Int(location.window)], location.group)
    elif location.source == 1:
        _copy_window(target, ops[Int(location.operation)].session_value.value().windows[Int(location.window)], location.group)
    elif location.source == 2:
        _copy_window(target, ops[Int(location.operation)].window_value.value(), location.group)
    elif location.source == 4:
        _copy_window_op(target, ops[Int(location.operation)], location.group)
    else:
        raise Error("invalid tree window metadata source")


def _restore_pane(mut target: PaneSnapshot, location: TreeLocator, snapshot: MuxSnapshot, ops: List[TreeOp]) raises:
    if location.source == 0:
        _copy_pane(target, snapshot.sessions[Int(location.session)].windows[Int(location.window)].panes[location.pane], location.group)
    elif location.source == 1:
        _copy_pane(target, ops[Int(location.operation)].session_value.value().windows[Int(location.window)].panes[location.pane], location.group)
    elif location.source == 2:
        _copy_pane(target, ops[Int(location.operation)].window_value.value().panes[location.pane], location.group)
    elif location.source == 3:
        _copy_pane(target, ops[Int(location.operation)].pane_value.value(), location.group)
    elif location.source == 4:
        _copy_pane_op(target, ops[Int(location.operation)], location.group)
    else:
        raise Error("invalid tree pane metadata source")


struct TreeReader(Movable):
    var words: List[UInt32]
    var offset: Int

    def __init__(out self, var words: List[UInt32]) raises:
        if len(words) > MAX_TREE_BYTES // 4:
            raise Error("normalized tree result exceeds its byte budget")
        self.words = words^
        self.offset = 0

    def word(mut self) raises -> UInt32:
        if self.offset >= len(self.words):
            raise Error("truncated normalized tree result")
        var result = self.words[self.offset]
        self.offset += 1
        return result

    def identifier(mut self) raises -> UInt64:
        var high = self.word()
        var low = self.word()
        return (UInt64(high) << 32) | UInt64(low)

    def optional_id(mut self) raises -> Optional[UInt64]:
        var tag = self.word()
        if tag == 0:
            return None
        if tag == 1:
            return self.identifier()
        raise Error("invalid normalized tree optional ID")

    def count(mut self, minimum: Int) raises -> Int:
        var count = self.word()
        if UInt64(count) > UInt64((len(self.words) - self.offset) // minimum):
            raise Error("normalized tree count exceeds remaining words")
        return Int(count)

    def finish(self) raises:
        if self.offset != len(self.words):
            raise Error("trailing normalized tree result words")


def _read_pane(mut reader: TreeReader, arena: TreeArena, snapshot: MuxSnapshot, ops: List[TreeOp]) raises -> PaneSnapshot:
    var result = PaneSnapshot()
    result.id = reader.identifier()
    for group in range(Int(PANE_TITLE), Int(PANE_SUMMARY) + 1):
        var location = arena.locate(reader.word(), UInt8(group))
        _restore_pane(result, location, snapshot, ops)
    return result^


def _read_window(mut reader: TreeReader, arena: TreeArena, snapshot: MuxSnapshot, ops: List[TreeOp]) raises -> WindowSnapshot:
    var result = WindowSnapshot()
    result.id = reader.identifier()
    result.index = reader.word()
    _restore_window(result, arena.locate(reader.word(), WINDOW_NAME), snapshot, ops)
    result.active_pane = reader.identifier()
    for group in range(Int(WINDOW_LAYOUT), Int(WINDOW_PRESENTATION) + 1):
        _restore_window(result, arena.locate(reader.word(), UInt8(group)), snapshot, ops)
    var count = reader.count(9)
    var previous: UInt64 = 0
    for index in range(count):
        var key = reader.identifier()
        if index > 0 and key <= previous:
            raise Error("Bend returned unsorted or duplicate tree pane keys")
        result.panes[key] = _read_pane(reader, arena, snapshot, ops)
        previous = key
    return result^


def _read_session(mut reader: TreeReader, arena: TreeArena, snapshot: MuxSnapshot, ops: List[TreeOp]) raises -> SessionSnapshot:
    var result = SessionSnapshot()
    result.id = reader.identifier()
    _restore_session(result, arena.locate(reader.word(), SESSION_NAME), snapshot, ops)
    result.active_window = reader.identifier()
    _restore_session(result, arena.locate(reader.word(), SESSION_VIEWERS), snapshot, ops)
    var count = reader.count(10)
    result.windows = List[WindowSnapshot](capacity=count)
    for _ in range(count):
        result.windows.append(_read_window(reader, arena, snapshot, ops))
    return result^


def _read_snapshot(mut reader: TreeReader, arena: TreeArena, snapshot: MuxSnapshot, ops: List[TreeOp]) raises -> MuxSnapshot:
    var result = MuxSnapshot()
    result.generation = reader.identifier()
    result.focused_window = reader.optional_id()
    var count = reader.count(7)
    result.sessions = List[SessionSnapshot](capacity=count)
    for _ in range(count):
        result.sessions.append(_read_session(reader, arena, snapshot, ops))
    return result^


def snapshot_from_words(var words: List[UInt32], arena: TreeArena, snapshot: MuxSnapshot, ops: List[TreeOp]) raises -> MuxSnapshot:
    var reader = TreeReader(words^)
    var result = _read_snapshot(reader, arena, snapshot, ops)
    reader.finish()
    return result^


struct TreeResult(Movable):
    var status: UInt8
    var expected: UInt64
    var received: UInt64
    var target: UInt64
    var snapshot: MuxSnapshot

    def __init__(out self):
        self.status = 0
        self.expected = 0
        self.received = 0
        self.target = 0
        self.snapshot = MuxSnapshot()


def result_from_words(var words: List[UInt32], arena: TreeArena, snapshot: MuxSnapshot, ops: List[TreeOp]) raises -> TreeResult:
    var reader = TreeReader(words^)
    var result = TreeResult()
    var status = reader.word()
    if status > 4:
        raise Error("Bend returned an invalid tree status")
    result.status = UInt8(status)
    if status == 1:
        result.expected = reader.identifier()
        result.received = reader.identifier()
    elif status >= 2:
        result.target = reader.identifier()
    result.snapshot = _read_snapshot(reader, arena, snapshot, ops)
    reader.finish()
    return result^
