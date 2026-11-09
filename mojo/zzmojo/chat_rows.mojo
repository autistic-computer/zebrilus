from .agent_chat import (
    ANSWER_QUESTIONS,
    ASK_QUESTIONS,
    CLOSE_ANSWERED,
    CLOSE_WITHDRAWN,
    ChatView,
    ITEM_AGENT,
    ITEM_EDIT,
    ITEM_EXECUTE,
    ITEM_FETCH,
    ITEM_MCP,
    ITEM_PLAN,
    ITEM_READ,
    ITEM_REASONING,
    ITEM_SEARCH,
    ITEM_SUBAGENT,
    ITEM_TOOL,
    ITEM_USER,
    ITEM_WEB,
    ItemView,
    OUTCOME_CANCELLED,
    OUTCOME_FAILED,
    OUTCOME_OPEN,
    RequestView,
    STATUS_COMPLETED,
    STATUS_DECLINED,
    STATUS_FAILED,
    STATUS_RUNNING,
    is_tool_kind,
)
from .agent_text import (
    text_blank,
    text_contains,
    text_find,
    text_join,
    text_lines,
    text_slice,
    text_split,
    text_trim,
    text_trim_end,
    text_trim_start,
)

comptime UNIT_USER: UInt8 = 0
comptime UNIT_TEXT: UInt8 = 1
comptime UNIT_RUN: UInt8 = 2
comptime UNIT_SUBAGENT: UInt8 = 3
comptime UNIT_AGENTS: UInt8 = 4
comptime UNIT_PLAN: UInt8 = 5
comptime UNIT_NOTICE: UInt8 = 6
comptime UNIT_ASK: UInt8 = 7
comptime UNIT_RECORD: UInt8 = 8
comptime UNIT_NONE: UInt8 = 255

comptime LABEL_LIVE: UInt8 = 0
comptime LABEL_THINKING: UInt8 = 1
comptime LABEL_DONE: UInt8 = 2

comptime FOLD_WORKING: UInt8 = 0
comptime FOLD_WORKED: UInt8 = 1
comptime FOLD_STOPPED: UInt8 = 2
comptime FOLD_FAILED: UInt8 = 3

comptime AGENT_WORKING: UInt8 = 0
comptime AGENT_WAITING: UInt8 = 1
comptime AGENT_DONE: UInt8 = 2
comptime AGENT_FAILED: UInt8 = 3
comptime AGENT_STOPPED: UInt8 = 4


@fieldwise_init
struct RunLabel(Copyable, Movable):
    var kind: UInt8
    var action: String
    var text: Optional[String]


@fieldwise_init
struct Unit(Copyable, Movable):
    var kind: UInt8
    var id: String
    var request: UInt64
    var items: List[String]
    var label: RunLabel


@fieldwise_init
struct TurnRows(Copyable, Movable):
    var folded: List[Unit]
    var shown: List[Unit]
    var has_fold: Bool
    var fold: UInt8
    var ms: UInt64
    var activity: Optional[String]


@fieldwise_init
struct AgentState(Copyable, Movable):
    var kind: UInt8
    var label: RunLabel
    var text: Optional[String]


@fieldwise_init
struct Answered(Copyable, Movable):
    var label: String
    var value: String


def _thinking() -> RunLabel:
    return RunLabel(LABEL_THINKING, String(), None)


def _item_unit(kind: UInt8, id: String) -> Unit:
    return Unit(kind, id, 0, List[String](), _thinking())


def _request_unit(kind: UInt8, request: UInt64) -> Unit:
    return Unit(kind, String(), request, List[String](), _thinking())


def duration(ms: UInt64) -> String:
    var seconds = ms / 1000
    var hours = seconds / 3600
    var minutes = seconds / 60 % 60
    var rest = seconds % 60
    if hours > 0:
        return String(hours, "h ", minutes, "m ", rest, "s")
    if minutes > 0:
        return String(minutes, "m ", rest, "s")
    return String(rest, "s")


def fold_label(fold: UInt8, ms: UInt64) -> String:
    if fold == FOLD_WORKING:
        return String("Working for ", duration(ms))
    if fold == FOLD_WORKED:
        return String("Worked for ", duration(ms))
    if fold == FOLD_STOPPED:
        return String("You stopped after ", duration(ms))
    return String("Failed after ", duration(ms))


def _starts(text: String, prefix: String) -> Bool:
    var width = prefix.byte_length()
    if text.byte_length() < width:
        return False
    for index in range(width):
        if text.as_bytes()[index] != prefix.as_bytes()[index]:
            return False
    return True


def _ends(text: String, suffix: String) -> Bool:
    var width = suffix.byte_length()
    var start = text.byte_length() - width
    if start < 0:
        return False
    for index in range(width):
        if text.as_bytes()[start + index] != suffix.as_bytes()[index]:
            return False
    return True


def _heading_line(raw: String) -> Bool:
    var line = text_trim(raw)
    var count = line.byte_length()
    if _starts(line, "**"):
        var rest = text_slice(line, 2, count)
        if _ends(rest, "**"):
            var inner = text_slice(rest, 0, rest.byte_length() - 2)
            return not text_blank(inner) and not text_contains(inner, "**")
    var marks = 0
    while marks < count and line.as_bytes()[marks] == 35:
        marks += 1
    if marks < 1 or marks > 6:
        return False
    if marks == count:
        return True
    return line.as_bytes()[marks] == 32 or line.as_bytes()[marks] == 9


def reasoning_body(text: String) -> Optional[String]:
    var rest = text_trim_start(text)
    while True:
        var count = rest.byte_length()
        if count == 0:
            break
        var newline = text_find(rest, "\n")
        var end = count if newline < 0 else newline
        if newline > 0 and rest.as_bytes()[newline - 1] == 13:
            end = newline - 1
        if not _heading_line(text_slice(rest, 0, end)):
            break
        rest = text_trim_start(text_slice(rest, end, count))
    if text_blank(rest):
        return None
    return Optional[String](text_trim_end(rest))


def _heading_of(raw: String) -> Optional[String]:
    var line = text_trim(raw)
    var count = line.byte_length()
    var inner: String
    if _starts(line, "**"):
        var rest = text_slice(line, 2, count)
        var close = text_find(rest, "**")
        inner = rest if close < 0 else text_slice(rest, 0, close)
    elif count > 0 and line.as_bytes()[0] == 35:
        var start = 1
        while start < count and line.as_bytes()[start] == 35:
            start += 1
        inner = text_slice(line, start, count)
    else:
        return None
    inner = text_trim(inner)
    var end = inner.byte_length()
    while end > 0 and inner.as_bytes()[end - 1] == 46:
        end -= 1
    if end == 0:
        return None
    return Optional[String](text_slice(inner, 0, end))


def _heading(text: String) -> Optional[String]:
    for line in text_lines(text):
        var found = _heading_of(line)
        if found:
            return found
    return None


def _last_heading(text: String) -> Optional[String]:
    var lines = text_lines(text)
    var index = len(lines) - 1
    while index >= 0:
        var found = _heading_of(lines[index])
        if found:
            return found
        index -= 1
    return None


def _action(kind: UInt8) -> String:
    if kind == ITEM_READ:
        return "Reading"
    if kind == ITEM_EDIT:
        return "Editing"
    if kind == ITEM_SEARCH:
        return "Searching for"
    if kind == ITEM_EXECUTE:
        return "Running"
    if kind == ITEM_FETCH:
        return "Fetching"
    if kind == ITEM_WEB:
        return "Searching the web for"
    return "Using"


def _detail(item: ItemView) -> String:
    if item.body.target:
        return item.body.target.value()
    return item.body.title


def _doing(item: ItemView) -> String:
    if item.body.activity:
        var activity = text_trim(item.body.activity.value())
        if activity.byte_length() > 0:
            return activity^
    if item.kind == ITEM_SUBAGENT and not text_blank(item.body.title):
        return item.body.title
    return String(_action(item.kind), " ", _detail(item))


def _subtree(view: ChatView, root: String) -> List[String]:
    var found = List[String]()
    found.append(root)
    var index = 0
    while index < len(found):
        var at = view.item(found[index])
        if at >= 0:
            for child in view.items[at].children:
                found.append(child)
        index += 1
    return found^


def _within(ids: List[String], id: String) -> Bool:
    for known in ids:
        if known == id:
            return True
    return False


def _open_within(view: ChatView, ids: List[String]) -> Bool:
    for id in ids:
        var at = view.item(id)
        if at >= 0 and view.items[at].is_open():
            return True
    return False


def _asked_within(view: ChatView, ids: List[String]) -> Bool:
    for request in view.requests:
        if (
            request.is_open()
            and request.item
            and _within(ids, request.item.value())
        ):
            return True
    return False


def _alive(view: ChatView, id: String) -> Bool:
    var subtree = _subtree(view, id)
    return _open_within(view, subtree) or _asked_within(view, subtree)


def _still_running(view: ChatView, unit: Unit) -> Bool:
    if unit.kind == UNIT_USER or unit.kind == UNIT_RECORD:
        return True
    if unit.kind == UNIT_ASK:
        var found = view.request(unit.request)
        return found >= 0 and view.requests[found].is_open()
    if unit.kind == UNIT_SUBAGENT:
        return _alive(view, unit.id)
    if unit.kind == UNIT_AGENTS or unit.kind == UNIT_RUN:
        for id in unit.items:
            if _alive(view, id):
                return True
    return False


def _unattached(view: ChatView, turn: Optional[UInt64]) -> List[Int]:
    var found = List[Int]()
    for index in range(len(view.requests)):
        var owner = view.requests[index].turn
        var same = (not owner and not turn) or (
            owner and turn and owner.value() == turn.value()
        )
        if not same:
            continue
        if (
            not view.requests[index].item
            or view.item(view.requests[index].item.value()) < 0
        ):
            found.append(index)
    return found^


def _asks(request: RequestView) -> UInt8:
    if request.is_open():
        return UNIT_ASK
    if request.ask.kind == ASK_QUESTIONS and request.close.kind != CLOSE_WITHDRAWN:
        return UNIT_RECORD
    return UNIT_NONE


def _questioned(view: ChatView, id: String) -> Bool:
    for request in view.requests:
        if (
            request.ask.kind == ASK_QUESTIONS
            and request.item
            and request.item.value() == id
        ):
            return True
    return False


def _tools(view: ChatView, items: List[String]) -> List[Int]:
    var tools = List[Int]()
    for id in items:
        var at = view.item(id)
        if at >= 0 and is_tool_kind(view.items[at].kind):
            tools.append(at)
    return tools^


def _outcomes(view: ChatView, tools: List[Int]) -> String:
    var failed = 0
    var denied = 0
    for tool in tools:
        if view.items[tool].status == STATUS_FAILED:
            failed += 1
        elif view.items[tool].status == STATUS_DECLINED:
            denied += 1
    var suffix = String()
    if failed > 0:
        suffix += String(" · ", failed, " failed")
    if denied > 0:
        suffix += String(" · ", denied, " denied")
    return suffix^


def _short(target: String) -> String:
    if text_contains(target, " "):
        return target
    var parts = text_split(target, "/")
    var index = len(parts) - 1
    while index >= 0:
        if parts[index] == "" or parts[index] == ".":
            index -= 1
            continue
        if parts[index] == "..":
            return target
        return parts[index]
    return target


def _category(kind: UInt8) -> Int:
    if kind == ITEM_EDIT:
        return 0
    if kind == ITEM_READ:
        return 1
    if kind == ITEM_SEARCH:
        return 2
    if kind == ITEM_EXECUTE:
        return 3
    if kind == ITEM_FETCH or kind == ITEM_WEB:
        return 4
    return 5


def _plural(category: Int, count: Int) -> String:
    if category == 0:
        return "edited a file" if count == 1 else String("edited ", count, " files")
    if category == 1:
        return "read a file" if count == 1 else String("read ", count, " files")
    if category == 2:
        return "searched once" if count == 1 else String("searched ", count, " times")
    if category == 3:
        return "ran a command" if count == 1 else String("ran ", count, " commands")
    if category == 4:
        return (
            "looked something up online"
            if count == 1
            else String("looked up ", count, " pages")
        )
    return "used a tool" if count == 1 else String("used ", count, " tools")


def _verb(category: Int) -> String:
    if category == 0:
        return "edited"
    if category == 1:
        return "read"
    if category == 2:
        return "searched for"
    return "ran"


def run_summary(view: ChatView, items: List[String]) -> String:
    var tools = _tools(view, items)
    if len(tools) == 0:
        return "Thought"
    var parts = List[String]()
    for category in range(6):
        var count = 0
        var only = -1
        for tool in tools:
            if _category(view.items[tool].kind) == category:
                count += 1
                only = tool
        if count == 0:
            continue
        if count == 1 and category < 4 and view.items[only].body.target:
            parts.append(
                String(
                    _verb(category),
                    " ",
                    _short(view.items[only].body.target.value()),
                )
            )
        else:
            parts.append(_plural(category, count))
    var summary = text_join(parts, ", ")
    var first = Int(summary.as_bytes()[0])
    if first >= 97 and first <= 122:
        var raised = List[UInt8]()
        raised.append(UInt8(first - 32))
        summary = String(
            String(from_utf8_lossy=Span(raised)),
            text_slice(summary, 1, summary.byte_length()),
        )
    summary += _outcomes(view, tools)
    return summary^


def run_shows(view: ChatView, items: List[String]) -> Bool:
    for id in items:
        var at = view.item(id)
        if at < 0:
            continue
        if is_tool_kind(view.items[at].kind) or reasoning_body(view.items[at].body.text):
            return True
    return False


def _batches(view: ChatView, items: List[String]) -> List[List[String]]:
    var out = List[List[String]]()
    var current = List[String]()
    var known = False
    var batch: UInt32 = 0
    var thoughts = 0
    for id in items:
        var at = view.item(id)
        if at >= 0 and is_tool_kind(view.items[at].kind):
            var this = view.items[at].batch
            if known and batch != this:
                var carried = List[String]()
                for index in range(len(current) - thoughts, len(current)):
                    carried.append(current[index])
                current.shrink(len(current) - thoughts)
                out.append(current^)
                current = carried^
            known = True
            batch = this
            thoughts = 0
        else:
            thoughts += 1
        current.append(id)
    if known and batch > 0 and thoughts > 0:
        var carried = List[String]()
        for index in range(len(current) - thoughts, len(current)):
            carried.append(current[index])
        current.shrink(len(current) - thoughts)
        out.append(current^)
        current = carried^
    out.append(current^)
    return out^


def _summarized(view: ChatView, tools: List[Int]) -> Bool:
    if len(tools) == 0 or view.items[tools[0]].batch == 0:
        return False
    for tool in tools:
        if (
            view.items[tool].batch != view.items[tools[0]].batch
            or view.items[tool].summary != view.items[tools[0]].summary
        ):
            return False
    return True


def _push_run(view: ChatView, items: List[String], mut out: List[Unit], tail: Bool):
    if not tail and not run_shows(view, items):
        return
    var running = -1
    var index = len(items) - 1
    while index >= 0:
        var at = view.item(items[index])
        if at >= 0 and is_tool_kind(view.items[at].kind) and view.items[at].is_open():
            running = at
            break
        index -= 1
    var label = _thinking()
    var tools = _tools(view, items)
    if running >= 0:
        label = RunLabel(
            LABEL_LIVE,
            _action(view.items[running].kind),
            Optional[String](_detail(view.items[running])),
        )
    elif _summarized(view, tools):
        label = RunLabel(
            LABEL_DONE,
            String(),
            Optional[String](
                String(view.items[tools[0]].summary, _outcomes(view, tools))
            ),
        )
    elif tail:
        index = len(items) - 1
        while index >= 0:
            var at = view.item(items[index])
            if at >= 0 and view.items[at].kind == ITEM_REASONING:
                label.text = _heading(view.items[at].body.text)
                break
            index -= 1
    else:
        label = RunLabel(
            LABEL_DONE, String(), Optional[String](run_summary(view, items))
        )
    out.append(Unit(UNIT_RUN, String(), 0, items.copy(), label^))


def _flush(view: ChatView, mut run: List[String], mut out: List[Unit], tail: Bool):
    if len(run) == 0:
        return
    var batches = _batches(view, run)
    run.clear()
    var last = len(batches) - 1
    for index in range(len(batches)):
        _push_run(view, batches[index], out, tail and index == last)


def _group(units: List[Unit]) -> List[Unit]:
    var out = List[Unit]()
    for unit in units:
        var last = len(out) - 1
        if unit.kind != UNIT_SUBAGENT or last < 0:
            out.append(unit.copy())
        elif out[last].kind == UNIT_SUBAGENT:
            var ids = List[String]()
            ids.append(out[last].id)
            ids.append(unit.id)
            out[last] = Unit(UNIT_AGENTS, String(), 0, ids^, _thinking())
        elif out[last].kind == UNIT_AGENTS:
            out[last].items.append(unit.id)
        else:
            out.append(unit.copy())
    return out^


def _units(
    view: ChatView, ids: List[String], standalone: List[Int], live: Bool
) -> List[Unit]:
    var times = List[UInt64]()
    var entries = List[Int]()
    for id in ids:
        var at = view.item(id)
        if at >= 0:
            times.append(view.items[at].started_at)
            entries.append(at)
    for request in standalone:
        times.append(view.requests[request].opened_at)
        entries.append(-request - 1)
    for index in range(1, len(entries)):
        var at = times[index]
        var entry = entries[index]
        var slot = index
        while slot > 0 and times[slot - 1] > at:
            times[slot] = times[slot - 1]
            entries[slot] = entries[slot - 1]
            slot -= 1
        times[slot] = at
        entries[slot] = entry
    var out = List[Unit]()
    var run = List[String]()
    for entry in entries:
        if entry < 0:
            var asks = _asks(view.requests[-entry - 1])
            if asks != UNIT_NONE:
                _flush(view, run, out, False)
                out.append(_request_unit(asks, view.requests[-entry - 1].id))
            continue
        var kind = view.items[entry].kind
        var id = view.items[entry].id
        if is_tool_kind(kind) or kind == ITEM_REASONING:
            if not _questioned(view, id):
                run.append(id)
        else:
            _flush(view, run, out, False)
            var unit = UNIT_NOTICE
            if kind == ITEM_USER:
                unit = UNIT_USER
            elif kind == ITEM_AGENT:
                unit = UNIT_TEXT
            elif kind == ITEM_SUBAGENT:
                unit = UNIT_SUBAGENT
            elif kind == ITEM_PLAN:
                unit = UNIT_PLAN
            out.append(_item_unit(unit, id))
        var flushed = False
        for request in view.requests:
            if not request.item or request.item.value() != id:
                continue
            var asks = _asks(request)
            if asks == UNIT_NONE:
                continue
            if not flushed:
                _flush(view, run, out, False)
                flushed = True
            out.append(_request_unit(asks, request.id))
    _flush(view, run, out, live)
    return _group(out)


def live_line(view: ChatView, turn: Int) -> Optional[String]:
    var known = Bool(view.turns[turn].activity)
    var line_at = view.turns[turn].activity_at
    var line = view.turns[turn].activity
    var busy = False
    for id in view.turns[turn].items:
        var at = view.item(id)
        if at < 0:
            continue
        var kind = view.items[at].kind
        var text: Optional[String] = None
        if kind == ITEM_REASONING:
            text = _last_heading(view.items[at].body.text)
            if not text:
                continue
        elif (
            (is_tool_kind(kind) or kind == ITEM_SUBAGENT)
            and view.items[at].is_open()
            and not view.items[at].background
        ):
            busy = True
            text = Optional[String](_doing(view.items[at]))
        elif kind != ITEM_USER and kind != ITEM_AGENT:
            continue
        if not known or view.items[at].started_at >= line_at:
            known = True
            line_at = view.items[at].started_at
            line = text
    if not busy and view.asking() >= 0:
        return Optional[String]("Waiting for your answer")
    return line


def turn_rows(view: ChatView, turn: Int, now: UInt64) -> TurnRows:
    var live = view.turns[turn].is_open()
    var units = _units(
        view,
        view.turns[turn].items,
        _unattached(view, Optional[UInt64](view.turns[turn].id)),
        live,
    )
    var activity: Optional[String] = None
    if live:
        activity = live_line(view, turn)
    var end = now
    if view.turns[turn].ended_at:
        end = view.turns[turn].ended_at.value()
    var started = view.turns[turn].started_at
    var ms = end - started if end > started else UInt64(0)
    var fold = FOLD_WORKED
    var outcome = view.turns[turn].outcome
    if outcome == OUTCOME_OPEN:
        fold = FOLD_WORKING
    elif outcome == OUTCOME_CANCELLED:
        fold = FOLD_STOPPED
    elif outcome == OUTCOME_FAILED:
        fold = FOLD_FAILED
    var answer = -1
    if not live:
        var last = -1
        for index in range(len(units)):
            if units[index].kind == UNIT_TEXT:
                last = index
        if last >= 0:
            answer = 0
            var index = last - 1
            while index >= 0:
                if units[index].kind != UNIT_TEXT:
                    answer = index + 1
                    break
                index -= 1
            var other = False
            for before in range(answer):
                if units[before].kind != UNIT_USER:
                    other = True
            if not other:
                answer = -1
    if answer < 0:
        return TurnRows(List[Unit](), units^, live, fold, ms, activity)
    var folded = List[Unit]()
    var shown = List[Unit]()
    for index in range(len(units)):
        if index >= answer or _still_running(view, units[index]):
            shown.append(units[index].copy())
        else:
            folded.append(units[index].copy())
    return TurnRows(folded^, shown^, True, fold, ms, activity)


def background_live(view: ChatView) -> Bool:
    for root in view.background:
        if _open_within(view, _subtree(view, root)):
            return True
    return False


def background_rows(view: ChatView) -> List[Unit]:
    return _units(view, view.background, _unattached(view, None), background_live(view))


def unit_at(view: ChatView, unit: Unit) -> Optional[UInt64]:
    if unit.kind == UNIT_ASK or unit.kind == UNIT_RECORD:
        var found = view.request(unit.request)
        if found < 0:
            return None
        return Optional[UInt64](view.requests[found].opened_at)
    var at: Int
    if unit.kind == UNIT_RUN or unit.kind == UNIT_AGENTS:
        if len(unit.items) == 0:
            return None
        at = view.item(unit.items[0])
    else:
        at = view.item(unit.id)
    if at < 0:
        return None
    return Optional[UInt64](view.items[at].started_at)


def agent_rows(view: ChatView, agent: String) -> List[Unit]:
    var at = view.item(agent)
    if at < 0:
        return List[Unit]()
    return _units(view, view.items[at].children, List[Int](), view.items[at].is_open())


def agent_state(view: ChatView, agent: String) -> Optional[AgentState]:
    var at = view.item(agent)
    if at < 0:
        return None
    if _asked_within(view, _subtree(view, agent)):
        return Optional[AgentState](AgentState(AGENT_WAITING, _thinking(), None))
    var status = view.items[at].status
    if status == STATUS_RUNNING:
        var rows = agent_rows(view, agent)
        var label = _thinking()
        if len(rows) > 0 and rows[len(rows) - 1].kind == UNIT_RUN:
            label = rows[len(rows) - 1].label.copy()
        return Optional[AgentState](AgentState(AGENT_WORKING, label^, None))
    if status == STATUS_COMPLETED:
        var result: Optional[String] = None
        var index = len(view.items[at].children) - 1
        while index >= 0:
            var child = view.item(view.items[at].children[index])
            if (
                child >= 0
                and view.items[child].kind == ITEM_AGENT
                and view.items[child].body.text.byte_length() > 0
            ):
                result = Optional[String](view.items[child].body.text)
                break
            index -= 1
        if not result and view.items[at].body.text.byte_length() > 0:
            result = Optional[String](view.items[at].body.text)
        return Optional[AgentState](AgentState(AGENT_DONE, _thinking(), result))
    if status == STATUS_FAILED:
        return Optional[AgentState](
            AgentState(AGENT_FAILED, _thinking(), view.items[at].message)
        )
    return Optional[AgentState](AgentState(AGENT_STOPPED, _thinking(), None))


def agent_ms(view: ChatView, agent: String, now: UInt64) -> Optional[UInt64]:
    var at = view.item(agent)
    if at < 0:
        return None
    var end = now
    if view.items[at].ended_at:
        end = view.items[at].ended_at.value()
    var started = view.items[at].started_at
    return Optional[UInt64](end - started if end > started else UInt64(0))


def agent_prompt(view: ChatView, agent: String) -> Optional[String]:
    var at = view.item(agent)
    if at < 0 or not view.items[at].body.target:
        return None
    if view.items[at].body.target.value().byte_length() == 0:
        return None
    return view.items[at].body.target


def answered(request: RequestView) -> List[Answered]:
    var out = List[Answered]()
    if request.ask.kind != ASK_QUESTIONS or not request.closed:
        return out^
    var usable = (
        request.close.kind == CLOSE_ANSWERED
        and request.close.answer == ANSWER_QUESTIONS
    )
    for question in request.ask.questions:
        var label = question.question
        if label.byte_length() == 0 and question.header:
            label = question.header.value()
        var found = -1
        if usable:
            for index in range(len(request.close.answers)):
                if request.close.answers[index].question == question.id:
                    found = index
                    break
            if found < 0:
                for index in range(len(request.close.answers)):
                    if request.close.answers[index].question == question.question:
                        found = index
                        break
        var value = String()
        if found >= 0:
            value = text_join(request.close.answers[found].answers, ", ")
        if value.byte_length() == 0:
            value = "no answer"
        out.append(Answered(label, value))
    return out^
