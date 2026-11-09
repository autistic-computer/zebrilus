from std.math import floor
from .agent_text import (
    text_ceil,
    text_find,
    text_first_scalar,
    text_join,
    text_lines,
    text_slice,
    text_split,
    text_trim,
    text_trim_start,
    text_words,
)

comptime SHAPE_USER: UInt8 = 0
comptime SHAPE_ASSISTANT: UInt8 = 1
comptime SHAPE_NOTICE: UInt8 = 2
comptime SHAPE_REASONING: UInt8 = 3
comptime SHAPE_PLAN: UInt8 = 4
comptime SHAPE_TOOL: UInt8 = 5

comptime TIMELINE_READ: UInt8 = 0
comptime TIMELINE_SEARCH: UInt8 = 1
comptime TIMELINE_EDIT: UInt8 = 2
comptime TIMELINE_EXECUTE: UInt8 = 3
comptime TIMELINE_FETCH: UInt8 = 4
comptime TIMELINE_THINK: UInt8 = 5
comptime TIMELINE_OTHER: UInt8 = 6

comptime STEP_PENDING: UInt8 = 0
comptime STEP_RUNNING: UInt8 = 1
comptime STEP_NEEDS_APPROVAL: UInt8 = 2
comptime STEP_COMPLETED: UInt8 = 3
comptime STEP_FAILED: UInt8 = 4
comptime STEP_CANCELED: UInt8 = 5

comptime ROW_SINGLE: UInt8 = 0
comptime ROW_TURN: UInt8 = 1
comptime ROW_REPLY: UInt8 = 2

comptime HEADER_WAITING: UInt8 = 0
comptime HEADER_LIVE: UInt8 = 1
comptime HEADER_DONE: UInt8 = 2

comptime PLAN_PENDING: UInt8 = 0
comptime PLAN_IN_PROGRESS: UInt8 = 1
comptime PLAN_DONE: UInt8 = 2

comptime COMPOSER_SEND: UInt8 = 0
comptime COMPOSER_QUEUE: UInt8 = 1
comptime COMPOSER_STOP: UInt8 = 2

comptime TRACE_COUNT_FAILED = 7


@fieldwise_init
struct TimelineEntry(Copyable, Movable):
    var id: UInt64
    var parent: Optional[UInt64]
    var shape: UInt8
    var tool: UInt8
    var status: UInt8
    var label: String


@fieldwise_init
struct TimelineRow(Copyable, Movable):
    var kind: UInt8
    var id: UInt64
    var entries: List[TimelineEntry]

    def position(self, id: UInt64) -> Int:
        for index in range(len(self.entries)):
            if self.entries[index].id == id:
                return index
        return -1

    def replace_entry(mut self, id: UInt64, entry: TimelineEntry) -> Bool:
        if entry.id != id:
            return False
        var index = self.position(id)
        if index < 0:
            return False
        self.entries[index] = entry.copy()
        return True


@fieldwise_init
struct TimelineFold(Movable):
    var rows: List[TimelineRow]
    var entry_to_row: List[Int]


def step_running(status: UInt8) -> Bool:
    return status == STEP_PENDING or status == STEP_RUNNING


def groups_as_turn(shape: UInt8) -> Bool:
    return (
        shape == SHAPE_TOOL
        or shape == SHAPE_REASONING
        or shape == SHAPE_PLAN
        or shape == SHAPE_ASSISTANT
    )


def nest_in_row(mut row: TimelineRow, entry: TimelineEntry):
    if row.kind == ROW_SINGLE:
        row.kind = ROW_REPLY if row.entries[0].shape == SHAPE_USER else ROW_TURN
    row.entries.append(entry.copy())


def append_timeline_row(
    mut rows: List[TimelineRow], entry: TimelineEntry
) -> Tuple[Int, Bool]:
    if entry.parent:
        var parent = entry.parent.value()
        var index = len(rows) - 1
        while index >= 0:
            if rows[index].position(parent) >= 0:
                nest_in_row(rows[index], entry)
                return (index, False)
            index -= 1
    if groups_as_turn(entry.shape) and len(rows) > 0:
        var last = len(rows) - 1
        if rows[last].kind == ROW_SINGLE:
            if groups_as_turn(rows[last].entries[0].shape):
                rows[last].kind = ROW_TURN
                rows[last].entries.append(entry.copy())
                return (last, False)
        elif rows[last].kind == ROW_TURN:
            rows[last].entries.append(entry.copy())
            return (last, False)
    var single = List[TimelineEntry]()
    single.append(entry.copy())
    rows.append(TimelineRow(ROW_SINGLE, entry.id, single^))
    return (len(rows) - 1, True)


def _positions(entries: List[TimelineEntry]) -> Dict[UInt64, Int]:
    var positions = Dict[UInt64, Int]()
    for index in range(len(entries)):
        positions[entries[index].id] = index
    return positions^


def _parent_of(
    entries: List[TimelineEntry], positions: Dict[UInt64, Int], index: Int
) -> Int:
    if not entries[index].parent:
        return -1
    return positions.get(entries[index].parent.value(), -1)


def _step_root(
    entries: List[TimelineEntry], positions: Dict[UInt64, Int], index: Int
) -> Int:
    var at = index
    for _ in range(len(entries)):
        var parent = _parent_of(entries, positions, at)
        if parent < 0:
            return at if at != index else -1
        at = parent
    return -1


def fold_timeline_rows(entries: List[TimelineEntry]) -> TimelineFold:
    var positions = _positions(entries)
    var roots = List[Int]()
    for index in range(len(entries)):
        roots.append(_step_root(entries, positions, index))
    var rows = List[TimelineRow]()
    var entry_to_row = List[Int](length=len(entries), fill=0)
    for index in range(len(entries)):
        if roots[index] < 0:
            entry_to_row[index] = append_timeline_row(rows, entries[index])[0]
    for index in range(len(entries)):
        if roots[index] >= 0:
            var row = entry_to_row[roots[index]]
            nest_in_row(rows[row], entries[index])
            entry_to_row[index] = row
    return TimelineFold(rows^, entry_to_row^)


def is_turn_row(row: TimelineRow) -> Bool:
    if row.kind == ROW_SINGLE:
        return groups_as_turn(row.entries[0].shape)
    return row.kind == ROW_TURN


def final_assistant_entry_id(rows: List[TimelineRow]) -> Optional[UInt64]:
    var row = len(rows) - 1
    while row >= 0:
        var index = len(rows[row].entries) - 1
        while index >= 0:
            if rows[row].entries[index].shape == SHAPE_ASSISTANT:
                return Optional[UInt64](rows[row].entries[index].id)
            index -= 1
        row -= 1
    return None


struct StepNesting(Movable):
    var top: List[Int]
    var owners: Dict[UInt64, Int]
    var steps: List[List[Int]]

    def __init__(out self, members: List[TimelineEntry]):
        self.top = List[Int]()
        self.owners = Dict[UInt64, Int]()
        self.steps = List[List[Int]]()
        var positions = _positions(members)
        for index in range(len(members)):
            var parent = _parent_of(members, positions, index)
            var reaches = True
            var at = index
            var hops = 0
            while True:
                var above = _parent_of(members, positions, at)
                if above < 0:
                    break
                hops += 1
                if hops > len(members):
                    reaches = False
                    break
                at = above
            if parent >= 0 and reaches:
                var owner = members[parent].id
                var slot = self.owners.get(owner, -1)
                if slot < 0:
                    slot = len(self.steps)
                    self.owners[owner] = slot
                    self.steps.append(List[Int]())
                self.steps[slot].append(index)
            else:
                self.top.append(index)

    def substeps(self, id: UInt64) -> Int:
        var slot = self.owners.get(id, -1)
        return len(self.steps[slot]) if slot >= 0 else 0


def split_turn(
    members: List[TimelineEntry], nesting: StepNesting
) -> Tuple[List[Int], List[Int]]:
    var shown = List[Int]()
    for index in nesting.top:
        if members[index].shape != SHAPE_PLAN:
            shown.append(index)
    var start = 0
    var at = len(shown) - 1
    while at >= 0:
        if members[shown[at]].shape != SHAPE_ASSISTANT:
            start = at + 1
            break
        at -= 1
    var answer = List[Int]()
    for index in range(start, len(shown)):
        answer.append(shown[index])
    shown.shrink(start)
    return (shown^, answer^)


def trace_noun(slot: Int, count: Int) -> String:
    if slot == 0:
        return "read" if count == 1 else "reads"
    if slot == 1:
        return "search" if count == 1 else "searches"
    if slot == 2:
        return "edit" if count == 1 else "edits"
    if slot == 3:
        return "command" if count == 1 else "commands"
    if slot == 4:
        return "fetch" if count == 1 else "fetches"
    if slot == 5:
        return "tool call" if count == 1 else "tool calls"
    if slot == 6:
        return "agent" if count == 1 else "agents"
    return "failed"


@fieldwise_init
struct TraceCount(Copyable, Movable):
    var slot: Int
    var count: Int
    var failed: Bool

    def describe(self) -> String:
        return String(self.count, " ", trace_noun(self.slot, self.count))


@fieldwise_init
struct TraceHeader(Movable):
    var kind: UInt8
    var label: String
    var tool: Bool
    var counts: List[TraceCount]

    def title(self) -> String:
        return "Waiting for you" if self.kind == HEADER_WAITING else self.label


def trace_counts(
    members: List[TimelineEntry], nesting: StepNesting, steps: List[Int]
) -> List[TraceCount]:
    var counts = List[Int](length=7, fill=0)
    var failed = 0
    for index in steps:
        if members[index].shape != SHAPE_TOOL:
            continue
        if members[index].status == STEP_FAILED:
            failed += 1
        var slot: Int
        if nesting.substeps(members[index].id) > 0:
            slot = 6
        elif members[index].tool == TIMELINE_THINK:
            continue
        elif members[index].tool == TIMELINE_OTHER:
            slot = 5
        else:
            slot = Int(members[index].tool)
        counts[slot] += 1
    var result = List[TraceCount]()
    for slot in range(7):
        if counts[slot] > 0:
            result.append(TraceCount(slot, counts[slot], False))
    if failed > 0:
        result.append(TraceCount(TRACE_COUNT_FAILED, failed, True))
    return result^


def duration_label(seconds: UInt64) -> String:
    var total = max(seconds, UInt64(1))
    var hours = total // 3600
    var minutes = total % 3600 // 60
    var rest = total % 60
    if hours == 0 and minutes == 0:
        return String(rest, "s")
    if hours == 0:
        return String(minutes, "m ", rest, "s")
    return String(hours, "h ", minutes, "m")


def single_line(text: String) -> String:
    if text_find(text, "\n") < 0 and text_find(text, "\r") < 0:
        return text
    var parts = List[String]()
    for line in text_split(text.replace("\r", "\n"), "\n"):
        var trimmed = text_trim(line)
        if trimmed.byte_length() > 0:
            parts.append(trimmed^)
    return text_join(parts, " · ")


def trace_header(
    members: List[TimelineEntry],
    nesting: StepNesting,
    steps: List[Int],
    answering: Bool,
    live: Bool,
    elapsed: Optional[UInt64],
) -> TraceHeader:
    if live and not answering:
        for index in steps:
            if (
                members[index].shape == SHAPE_TOOL
                and members[index].status == STEP_NEEDS_APPROVAL
            ):
                return TraceHeader(
                    HEADER_WAITING,
                    single_line(members[index].label),
                    False,
                    List[TraceCount](),
                )
        if len(steps) > 0:
            var last = steps[len(steps) - 1]
            if members[last].shape == SHAPE_TOOL and step_running(
                members[last].status
            ):
                return TraceHeader(
                    HEADER_LIVE,
                    single_line(members[last].label),
                    True,
                    List[TraceCount](),
                )
            if members[last].shape == SHAPE_REASONING:
                return TraceHeader(
                    HEADER_LIVE, "Thinking", False, List[TraceCount]()
                )
        return TraceHeader(HEADER_LIVE, "Working", False, List[TraceCount]())
    var thought_only = True
    for index in steps:
        if members[index].shape == SHAPE_TOOL:
            thought_only = False
    var label: String
    if thought_only and elapsed:
        label = String("Thought for ", duration_label(elapsed.value()))
    elif thought_only:
        label = "Thought"
    elif elapsed:
        label = String("Worked ", duration_label(elapsed.value()))
    else:
        label = "Worked"
    return TraceHeader(
        HEADER_DONE, label, False, trace_counts(members, nesting, steps)
    )


def ending_failure(members: List[TimelineEntry], steps: List[Int]) -> Int:
    var at = len(steps) - 1
    while at >= 0:
        if members[steps[at]].shape == SHAPE_TOOL:
            return steps[at] if members[steps[at]].status == STEP_FAILED else -1
        at -= 1
    return -1


def tool_tag(status: UInt8, exit_code: Optional[Int64]) -> Optional[String]:
    if status == STEP_FAILED:
        if exit_code and exit_code.value() != 0:
            return Optional[String](String("exit ", exit_code.value()))
        return Optional[String]("failed")
    if status == STEP_CANCELED:
        return Optional[String]("canceled")
    return None


def thought_tail(text: String) -> Optional[String]:
    var flat = text_join(text_words(text), " ")
    var count = flat.byte_length()
    if count == 0:
        return None
    if count <= 220:
        return Optional[String](flat^)
    var tail = text_slice(flat, text_ceil(flat, count - 220), count)
    var space = text_find(tail, " ")
    if space >= 0:
        tail = text_slice(tail, space + 1, tail.byte_length())
    return Optional[String](String("…", tail))


@fieldwise_init
struct PlanItem(Copyable, Movable):
    var state: UInt8
    var text: String


def plan_items(source: String) -> List[PlanItem]:
    var items = List[PlanItem]()
    for line in text_lines(source):
        var rest = text_trim_start(line)
        if not rest.startswith("- ["):
            continue
        rest = text_slice(rest, 3, rest.byte_length())
        var mark = text_first_scalar(rest)
        if mark < 0:
            continue
        var width = 1 if mark < 128 else 2 if mark < 2048 else 3 if mark < 65536 else 4
        rest = text_slice(rest, width, rest.byte_length())
        if not rest.startswith("]"):
            continue
        var state = PLAN_PENDING
        if mark == 120 or mark == 88:
            state = PLAN_DONE
        elif mark == 126:
            state = PLAN_IN_PROGRESS
        items.append(
            PlanItem(state, text_trim(text_slice(rest, 1, rest.byte_length())))
        )
    return items^


@fieldwise_init
struct PlanProgress(Movable):
    var done: Int
    var total: Int
    var current: Optional[String]


def plan_progress(source: String) -> PlanProgress:
    var items = plan_items(source)
    var done = 0
    var current = Optional[String]()
    var pending = Optional[String]()
    for item in items:
        if item.state == PLAN_DONE:
            done += 1
        elif item.state == PLAN_IN_PROGRESS:
            if not current:
                current = Optional[String](item.text)
        elif not pending:
            pending = Optional[String](item.text)
    if not current:
        current = pending.copy()
    return PlanProgress(done, len(items), current^)


def plan_chip_label(source: String) -> Optional[String]:
    var progress = plan_progress(source)
    if progress.total == 0:
        return None
    return Optional[String](String("Plan ", progress.done, "/", progress.total))


def task_tray_label(count: Int) -> String:
    return String(count, " running")


def composer_action(active_turn: Bool, has_content: Bool) -> UInt8:
    if not active_turn:
        return COMPOSER_SEND
    return COMPOSER_QUEUE if has_content else COMPOSER_STOP


def git_file_count_label(count: UInt32) -> String:
    if count == 1:
        return "1 file"
    return String(count, " files")


def git_summary_tooltip(
    branch: Optional[String], files: UInt32, additions: UInt32, deletions: UInt32
) -> String:
    return String(
        branch.value() if branch else String("Detached HEAD"),
        ": ",
        git_file_count_label(files),
        ", +",
        additions,
        " additions, -",
        deletions,
        " deletions",
    )


def context_usage_fraction(used: UInt64, size: UInt64) -> Float64:
    if size == 0:
        return 0.0
    return min(max(Float64(min(used, size)) / Float64(size), 0.0), 1.0)


def round_half_even(value: Float64) -> Int:
    var base = floor(value)
    var rest = value - base
    var whole = Int(base)
    if rest > 0.5 or rest == 0.5 and whole % 2 != 0:
        return whole + 1
    return whole


def context_usage_tooltip(used: UInt64, size: UInt64) -> String:
    if size == 0:
        return "Context window usage unavailable"
    return String(
        used,
        " of ",
        size,
        " context tokens used (",
        round_half_even(context_usage_fraction(used, size) * 100.0),
        "%)",
    )
