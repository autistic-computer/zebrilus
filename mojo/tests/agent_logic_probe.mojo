from std.sys import argv
from std.testing import assert_equal, assert_true
from browser_fixture import Source
from agent_fixture import AgentCase, bit, number, numbers, parsed, read_case, some, unsigned
from zzmojo.agent_text import text_join, text_split
from zzmojo.agent_flip import (
    AgentFlips,
    FLIP_ARMED,
    FLIP_BUSY,
    FLIP_BUSY_TITLE,
    FLIP_CANCEL_LABEL,
    FLIP_DISMISSED,
    FLIP_IDLE,
    FLIP_NOW_LABEL,
    FLIP_SENDING,
    FLIP_WHEN_IDLE_LABEL,
    Flip,
    FlipSide,
    busy_reason,
    flip_armed_label,
    flip_busy_detail,
    flip_command,
    flip_label,
    flip_tooltip,
)
from zzmojo.agent_output import output_pane_args
from zzmojo.agent_timeline import (
    HEADER_DONE,
    HEADER_LIVE,
    HEADER_WAITING,
    ROW_SINGLE,
    ROW_TURN,
    SHAPE_ASSISTANT,
    SHAPE_NOTICE,
    SHAPE_PLAN,
    SHAPE_REASONING,
    SHAPE_TOOL,
    SHAPE_USER,
    STEP_FAILED,
    StepNesting,
    TimelineEntry,
    TimelineRow,
    TraceHeader,
    append_timeline_row,
    composer_action,
    context_usage_fraction,
    context_usage_tooltip,
    duration_label,
    ending_failure,
    final_assistant_entry_id,
    fold_timeline_rows,
    git_file_count_label,
    git_summary_tooltip,
    is_turn_row,
    plan_chip_label,
    plan_items,
    plan_progress,
    single_line,
    split_turn,
    task_tray_label,
    thought_tail,
    tool_tag,
    trace_header,
)


def entries(spec: String) raises -> List[TimelineEntry]:
    var result = List[TimelineEntry]()
    for part in text_split(spec, ";"):
        if part.byte_length() == 0:
            continue
        var fields = text_split(part, "|")
        var label = List[String]()
        for index in range(3, len(fields)):
            label.append(fields[index])
        var parent = parsed(fields[1])
        var code = fields[2].as_bytes()[0]
        var shape = SHAPE_TOOL
        var tool = UInt8(0)
        var status = UInt8(0)
        if code == 117:
            shape = SHAPE_USER
        elif code == 97:
            shape = SHAPE_ASSISTANT
        elif code == 110:
            shape = SHAPE_NOTICE
        elif code == 114:
            shape = SHAPE_REASONING
        elif code == 112:
            shape = SHAPE_PLAN
        else:
            tool = fields[2].as_bytes()[1] - 48
            status = fields[2].as_bytes()[2] - 48
        result.append(
            TimelineEntry(
                unsigned(fields[0]),
                Optional[UInt64](UInt64(parent.value())) if parent else None,
                shape,
                tool,
                status,
                text_join(label, "|"),
            )
        )
    return result^


def rows_dump(rows: List[TimelineRow]) -> String:
    var parts = List[String]()
    for row in rows:
        if row.kind == ROW_SINGLE:
            parts.append(String("S", row.entries[0].id))
            continue
        var ids = List[String]()
        for entry in row.entries:
            ids.append(String(entry.id))
        parts.append(
            String(
                "T" if row.kind == ROW_TURN else "R",
                row.id,
                "[",
                text_join(ids, ","),
                "]",
            )
        )
    return text_join(parts, ";")


def header_dump(header: TraceHeader) -> String:
    var body: String
    if header.kind == HEADER_WAITING:
        body = String("W:", header.label)
    elif header.kind == HEADER_LIVE:
        body = String("L:", header.label, ":", bit(header.tool))
    else:
        var counts = List[String]()
        for count in header.counts:
            counts.append(
                String(
                    count.slot,
                    "=",
                    count.count,
                    "/",
                    bit(count.failed),
                    "/",
                    count.describe(),
                )
            )
        body = String("D:", header.label, ":", text_join(counts, ","))
    return String(body, "|", header.title())


def failure(index: Int) -> String:
    return "None" if index < 0 else String("Some(", index, ")")


def fold(spec: String) raises -> String:
    var members = entries(spec)
    var folded = fold_timeline_rows(members)
    var flags = String()
    for row in folded.rows:
        flags += bit(is_turn_row(row))
    var final = final_assistant_entry_id(folded.rows)
    return String(
        rows_dump(folded.rows),
        "#",
        numbers(folded.entry_to_row),
        "#",
        String(final.value()) if final else String("-"),
        "#",
        flags,
    )


def append(spec: String) raises -> String:
    var members = entries(spec)
    var rows = List[TimelineRow]()
    var steps = String()
    for entry in members:
        var placed = append_timeline_row(rows, entry)
        steps += String("(", placed[0], ",", bit(placed[1]), ")")
    var replaced = False
    if len(rows) > 0:
        var entry = members[0].copy()
        entry.label = "replaced"
        var id = rows[0].id
        replaced = rows[0].replace_entry(id, entry) and not rows[
            0
        ].replace_entry(id + 1000, entry)
    return String(steps, "#", rows_dump(rows), "#", bit(replaced))


def nest(spec: String) raises -> String:
    var members = entries(spec)
    var nesting = StepNesting(members)
    var split = split_turn(members, nesting)
    var all = List[Int]()
    for index in range(len(members)):
        all.append(index)
    var dump = String(numbers(nesting.top), "#")
    for index in range(len(members)):
        var first = True
        for earlier in range(index):
            if members[earlier].id == members[index].id:
                first = False
        if first:
            dump += String(
                members[index].id, "=", nesting.substeps(members[index].id), ";"
            )
    dump += String("#", numbers(split[0]), "#", numbers(split[1]))
    for answering in range(2):
        for live in range(2):
            for timed in range(2):
                var elapsed = Optional[UInt64](3725) if timed == 1 else None
                dump += "#"
                dump += header_dump(
                    trace_header(
                        members, nesting, split[0], answering == 1, live == 1, elapsed
                    )
                )
                dump += "#"
                dump += header_dump(
                    trace_header(
                        members, nesting, all, answering == 1, live == 1, elapsed
                    )
                )
    dump += String(
        "#",
        failure(ending_failure(members, split[0])),
        "#",
        failure(ending_failure(members, all)),
    )
    return dump^


def plan(source: String) -> String:
    var items = List[String]()
    for item in plan_items(source):
        items.append(String(2 if item.state == 2 else Int(item.state), ":", item.text))
    var progress = plan_progress(source)
    return String(
        text_join(items, "\n"),
        "#",
        progress.done,
        "/",
        progress.total,
        "/",
        some(progress.current),
        "#",
        some(plan_chip_label(source)),
    )


def flip_state(flip: Flip) -> String:
    var body: String
    if flip.kind == FLIP_IDLE:
        body = "i"
    elif flip.kind == FLIP_SENDING:
        body = String("s", flip.mode)
    elif flip.kind == FLIP_BUSY:
        body = String("b(", flip.reason, ")")
    else:
        body = "a"
    return String(body, bit(flip.pending()), bit(flip.armed()), bit(flip.busy()))


def request(text: String) -> Optional[UInt64]:
    var value = parsed(text)
    if value and value.value() >= 0:
        return Optional[UInt64](UInt64(value.value()))
    return None


def flip_script(script: String) raises -> String:
    var flips = AgentFlips()
    var dump = String()
    for line in text_split(script, "\n"):
        var fields = text_split(line, "\t")
        var result = String()
        if fields[0] == "p":
            var pane = unsigned(fields[1])
            var mode = flips.press(pane)
            if mode >= 0:
                result = bit(
                    flips.sent(
                        pane,
                        UInt8(0 if unsigned(fields[2]) == 0 else 1),
                        mode,
                        request(fields[3]),
                    )
                )
            else:
                result = bit(mode == FLIP_DISMISSED)
        elif fields[0] == "c":
            var pane = unsigned(fields[1])
            var mode = number(fields[2])
            var sent = False
            if flips.choose(pane, mode):
                sent = flips.sent(
                    pane, flips.side(pane).value(), mode, request(fields[3])
                )
            result = bit(sent)
        elif fields[0] == "d":
            flips.dismiss(unsigned(fields[1]))
        elif fields[0] == "s":
            var failed = (
                None if fields[2] == "ok" else Optional[String](fields[2])
            )
            result = some(flips.settle(unsigned(fields[1]), failed))
        else:
            var sides = List[FlipSide]()
            for index in range(1, len(fields)):
                if fields[index].byte_length() == 0:
                    continue
                var parts = text_split(fields[index], ",")
                sides.append(
                    FlipSide(
                        unsigned(parts[0]),
                        UInt8(0 if unsigned(parts[1]) == 0 else 1),
                        parts[2] == "1",
                    )
                )
            flips.observe_panes(sides)
        dump += result
        for pane in range(1, 5):
            dump += " "
            dump += flip_state(flips.state(UInt64(pane)))
        dump += " "
        for owned in range(1, 10):
            dump += bit(flips.owns(UInt64(owned)))
        dump += "\n"
    return dump^


def flip_labels() -> String:
    var labels = String(
        flip_label(0),
        "|",
        flip_label(1),
        "|",
        FLIP_BUSY_TITLE,
        "|",
        FLIP_WHEN_IDLE_LABEL,
        "|",
        FLIP_NOW_LABEL,
        "|",
        FLIP_CANCEL_LABEL,
        "|",
        flip_busy_detail("a turn"),
    )
    for to in range(2):
        labels += String(
            "|",
            flip_armed_label(UInt8(to)),
            "|",
            flip_tooltip(UInt8(to), False),
            "|",
            flip_tooltip(UInt8(to), True),
        )
    for mode in range(4):
        labels += "|flip-agent"
        for arg in flip_command(42, mode):
            labels += String(" ", arg)
    return labels^


def run(item: AgentCase) raises -> String:
    if item.op == "fold":
        return fold(item.fields[0])
    if item.op == "append":
        return append(item.fields[0])
    if item.op == "nest":
        return nest(item.fields[0])
    if item.op == "tool_tag":
        var status = UInt8(number(item.fields[0]))
        var exit = parsed(item.fields[1])
        var tag = tool_tag(
            status, Optional[Int64](Int64(exit.value())) if exit else None
        )
        if not tag:
            return "N"
        return String("S", tag.value(), ":", bit(status == STEP_FAILED))
    if item.op == "duration":
        return duration_label(unsigned(item.fields[0]))
    if item.op == "thought_tail":
        return some(thought_tail(item.fields[0]))
    if item.op == "single_line":
        return single_line(item.fields[0])
    if item.op == "plan":
        return plan(item.fields[0])
    if item.op == "tray":
        return task_tray_label(number(item.fields[0]))
    if item.op == "composer":
        return String(
            Int(composer_action(item.fields[0] == "1", item.fields[1] == "1"))
        )
    if item.op == "git":
        var files = UInt32(unsigned(item.fields[1]))
        return String(
            git_file_count_label(files),
            "|",
            git_summary_tooltip(
                None if item.fields[0] == "-" else Optional[String](item.fields[0]),
                files,
                UInt32(unsigned(item.fields[2])),
                UInt32(unsigned(item.fields[3])),
            ),
        )
    if item.op == "usage":
        var used = unsigned(item.fields[0])
        var size = unsigned(item.fields[1])
        var fraction = context_usage_fraction(used, size)
        return String(
            Pointer(to=fraction).unsafe_bitcast[UInt64]()[],
            "|",
            context_usage_tooltip(used, size),
        )
    if item.op == "busy_reason":
        return some(busy_reason(item.fields[0]))
    if item.op == "flip_labels":
        return flip_labels()
    if item.op == "flip_script":
        return flip_script(item.fields[0])
    if item.op == "output":
        var exit = parsed(item.fields[3])
        var args = output_pane_args(
            unsigned(item.fields[0]),
            item.fields[1],
            item.fields[2],
            Optional[Int64](Int64(exit.value())) if exit else None,
            item.fields[4],
        )
        if not args:
            return "N"
        return String("S", text_join(args.value(), "\x1f"))
    raise Error(String("unknown agent logic case ", item.op))


def main() raises:
    var arguments = argv()
    var bytes: List[UInt8]
    with open(String(arguments[1]), "r") as file:
        bytes = file.read_bytes()
    assert_true(len(bytes) < 8 * 1024 * 1024)
    var source = Source(bytes^)
    var count = Int(source.uint(4))
    var timeline = 0
    var flips = 0
    var outputs = 0
    for _ in range(count):
        var item = read_case(source)
        assert_equal(run(item), item.expected)
        if item.op == "output":
            outputs += 1
        elif item.op.startswith("flip") or item.op == "busy_reason":
            flips += 1
        else:
            timeline += 1
    assert_equal(source.offset, len(source.bytes))
    print(timeline, flips, outputs)
