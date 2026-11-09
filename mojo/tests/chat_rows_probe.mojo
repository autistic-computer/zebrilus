from std.sys import argv
from std.testing import assert_equal
from browser_fixture import Source
from agent_fixture import AgentCase, bit, number, read_case, some, unsigned
from zzmojo.agent_text import text_join, text_split
from zzmojo.agent_json import json_text
from zzmojo.agent_chat import (
    ANSWER_CANCEL,
    ANSWER_OPTION,
    ASK_APPROVAL,
    ASK_PLAN,
    AskOption,
    Ask,
    CHAT_MAX_ITEMS,
    CHAT_REFUSED_FULL,
    CLOSE_DISMISSED,
    CLOSE_WITHDRAWN,
    ChatView,
    ITEM_REASONING,
    ITEM_SUBAGENT,
    OUTCOME_CANCELLED,
    OUTCOME_COMPLETED,
    OUTCOME_FAILED,
    OUTCOME_OPEN,
    RequestView,
    chat_event_decode,
)
from zzmojo.chat_rows import (
    AGENT_DONE,
    AGENT_FAILED,
    AGENT_WAITING,
    AGENT_WORKING,
    LABEL_LIVE,
    LABEL_THINKING,
    RunLabel,
    UNIT_AGENTS,
    UNIT_ASK,
    UNIT_RECORD,
    UNIT_RUN,
    Unit,
    agent_ms,
    agent_prompt,
    agent_rows,
    agent_state,
    answered,
    background_live,
    background_rows,
    duration,
    fold_label,
    live_line,
    reasoning_body,
    run_shows,
    run_summary,
    turn_rows,
    unit_at,
)

comptime FIELD = "\x1f"
comptime RECORD = "\x1e"
comptime ONE = "\x1d"
comptime TWO = "\x1c"
comptime THREE = "\x1b"
comptime FOUR = "\x1a"


def listed(parts: List[String], separator: String) -> String:
    var out = String(len(parts))
    for part in parts:
        out += separator
        out += part
    return out^


def record(mut out: String, fields: List[String]):
    out += text_join(fields, FIELD)
    out += RECORD


def count(value: Optional[UInt64]) -> String:
    if value:
        return String("S", value.value())
    return "N"


def less(left: String, right: String) -> Bool:
    var shared = min(left.byte_length(), right.byte_length())
    for index in range(shared):
        var first = left.as_bytes()[index]
        var second = right.as_bytes()[index]
        if first != second:
            return first < second
    return left.byte_length() < right.byte_length()


def label_text(label: RunLabel) -> String:
    var text = String()
    if label.text:
        text = label.text.value()
    if label.kind == LABEL_LIVE:
        return String("l", label.action, THREE, text)
    if label.kind == LABEL_THINKING:
        return String("t", some(label.text))
    return String("d", text)


def unit_text(view: ChatView, unit: Unit) -> String:
    var fields = List[String]()
    fields.append(String(Int(unit.kind)))
    if unit.kind == UNIT_RUN or unit.kind == UNIT_AGENTS:
        fields.append(text_join(unit.items, THREE))
    elif unit.kind == UNIT_ASK or unit.kind == UNIT_RECORD:
        fields.append(String(unit.request))
    else:
        fields.append(unit.id)
    fields.append(count(unit_at(view, unit)))
    if unit.kind == UNIT_RUN:
        fields.append(label_text(unit.label))
        fields.append(run_summary(view, unit.items))
        fields.append(bit(run_shows(view, unit.items)))
    return text_join(fields, TWO)


def units_text(view: ChatView, units: List[Unit]) -> String:
    var parts = List[String]()
    for unit in units:
        parts.append(unit_text(view, unit))
    return listed(parts, ONE)


def choices(options: List[AskOption]) -> String:
    var parts = List[String]()
    for option in options:
        parts.append(String(option.id, THREE, option.label, THREE, Int(option.kind)))
    return listed(parts, TWO)


def asked(ask: Ask) -> String:
    if ask.kind == ASK_APPROVAL:
        return String(
            "a", ONE, ask.title, ONE, some(ask.detail), ONE, choices(ask.options)
        )
    if ask.kind == ASK_PLAN:
        return String("p", ONE, ask.markdown, ONE, choices(ask.options))
    var out = String("q")
    for question in ask.questions:
        var options = List[String]()
        for option in question.options:
            options.append(String(option.label, FOUR, some(option.description)))
        var fields = List[String]()
        fields.append(question.id)
        fields.append(some(question.header))
        fields.append(question.question)
        fields.append(
            String(
                bit(question.multi_select),
                bit(question.allow_other),
                bit(question.secret),
            )
        )
        fields.append(listed(options, THREE))
        out += ONE
        out += text_join(fields, TWO)
    return out^


def closed(request: RequestView) -> String:
    if not request.closed:
        return "N"
    if request.close.kind == CLOSE_WITHDRAWN:
        return "w"
    if request.close.kind == CLOSE_DISMISSED:
        return "d"
    if request.close.answer == ANSWER_CANCEL:
        return "c"
    if request.close.answer == ANSWER_OPTION:
        return String("o", request.close.id)
    var out = String("q")
    for answer in request.close.answers:
        out += ONE
        out += answer.question
        for text in answer.answers:
            out += TWO
            out += text
    return out^


def state(view: ChatView, agent: String) -> String:
    var found = agent_state(view, agent)
    if not found:
        return "N"
    var kind = found.value().kind
    if kind == AGENT_WORKING:
        return String("w", label_text(found.value().label))
    if kind == AGENT_WAITING:
        return "W"
    if kind == AGENT_DONE:
        return String("d", some(found.value().text))
    if kind == AGENT_FAILED:
        return String("f", some(found.value().text))
    return "s"


def sorted_items(view: ChatView) -> List[Int]:
    var order = List[Int]()
    for index in range(len(view.items)):
        var at = len(order)
        order.append(index)
        while at > 0 and less(view.items[index].id, view.items[order[at - 1]].id):
            order[at] = order[at - 1]
            at -= 1
        order[at] = index
    return order^


def dump(view: ChatView, results: String, now: UInt64) -> String:
    var out = String()
    var fields = List[String]()
    fields.append("results")
    fields.append(results)
    record(out, fields)
    fields = List[String]()
    fields.append("view")
    fields.append(String(Int(view.phase())))
    fields.append(String(Int(view.session_kind)))
    fields.append(view.session)
    fields.append(some(view.last_session))
    fields.append(String(view.skipped))
    if view.has_usage:
        fields.append(String("S", view.used, "/", view.size))
    else:
        fields.append("N")
    record(out, fields)
    var queued = List[String]()
    for prompt in view.queue:
        queued.append(String(prompt.id, TWO, prompt.text))
    fields = List[String]()
    fields.append("queue")
    fields.append(listed(queued, ONE))
    record(out, fields)
    for turn in view.turns:
        fields = List[String]()
        fields.append("turn")
        fields.append(String(turn.id))
        if turn.by_user:
            fields.append(String("u", turn.prompt))
        else:
            fields.append("a")
        fields.append(String(turn.started_at))
        fields.append(count(turn.ended_at))
        if turn.outcome == OUTCOME_OPEN:
            fields.append("N")
        elif turn.outcome == OUTCOME_COMPLETED:
            fields.append(String("c", turn.detail))
        elif turn.outcome == OUTCOME_CANCELLED:
            fields.append("x")
        elif turn.outcome == OUTCOME_FAILED:
            fields.append(String("f", turn.detail))
        else:
            fields.append("d")
        fields.append(bit(turn.stopping))
        fields.append(listed(turn.items, ONE))
        if turn.activity:
            fields.append(String("S", turn.activity_at, ":", turn.activity.value()))
        else:
            fields.append("N")
        record(out, fields)
    var order = sorted_items(view)
    for at in order:
        fields = List[String]()
        fields.append("item")
        fields.append(view.items[at].id)
        fields.append(String(Int(view.items[at].kind)))
        fields.append(String(Int(view.items[at].status)))
        fields.append(some(view.items[at].message))
        fields.append(count(view.items[at].turn))
        fields.append(some(view.items[at].parent))
        fields.append(listed(view.items[at].children, ONE))
        fields.append(bit(view.items[at].background))
        fields.append(String(view.items[at].started_at))
        fields.append(count(view.items[at].ended_at))
        if view.items[at].batch == 0:
            fields.append("N")
        else:
            fields.append(
                String("S", view.items[at].batch, ":", view.items[at].summary)
            )
        fields.append(view.items[at].body.title)
        fields.append(some(view.items[at].body.target))
        fields.append(view.items[at].body.text)
        var steps = List[String]()
        for step in view.items[at].body.steps:
            steps.append(String(Int(step.status), step.text))
        fields.append(listed(steps, ONE))
        if view.items[at].body.exit_code:
            fields.append(String("S", view.items[at].body.exit_code.value()))
        else:
            fields.append("N")
        var images = List[String]()
        for image in view.items[at].body.images:
            var total = 0
            for byte in image.data:
                total += Int(byte)
            images.append(String(image.format, ":", len(image.data), ":", total))
        fields.append(listed(images, ONE))
        fields.append(some(view.items[at].body.activity))
        record(out, fields)
    fields = List[String]()
    fields.append("background")
    fields.append(listed(view.background, ONE))
    record(out, fields)
    for request in view.requests:
        fields = List[String]()
        fields.append("request")
        fields.append(String(request.id))
        fields.append(count(request.turn))
        fields.append(some(request.item))
        fields.append(bit(request.blocking))
        fields.append(String(request.opened_at))
        if request.closed:
            fields.append(String("S", request.closed_at))
        else:
            fields.append("N")
        fields.append(asked(request.ask))
        fields.append(closed(request))
        var pairs = List[String]()
        for pair in answered(request):
            pairs.append(String(pair.label, TWO, pair.value))
        fields.append(listed(pairs, ONE))
        record(out, fields)
    for index in range(len(view.turns)):
        var rows = turn_rows(view, index, now)
        fields = List[String]()
        fields.append("rows")
        fields.append(String(view.turns[index].id))
        fields.append(units_text(view, rows.folded))
        fields.append(units_text(view, rows.shown))
        if rows.has_fold:
            fields.append(String("S", fold_label(rows.fold, rows.ms)))
        else:
            fields.append("N")
        fields.append(some(rows.activity))
        fields.append(some(live_line(view, index)))
        record(out, fields)
    fields = List[String]()
    fields.append("bg")
    fields.append(units_text(view, background_rows(view)))
    fields.append(bit(background_live(view)))
    record(out, fields)
    for at in order:
        if view.items[at].kind == ITEM_SUBAGENT:
            var id = view.items[at].id
            fields = List[String]()
            fields.append("agent")
            fields.append(id)
            fields.append(units_text(view, agent_rows(view, id)))
            fields.append(state(view, id))
            fields.append(count(agent_ms(view, id, now)))
            fields.append(some(agent_prompt(view, id)))
            record(out, fields)
        if view.items[at].kind == ITEM_REASONING:
            fields = List[String]()
            fields.append("think")
            fields.append(view.items[at].id)
            fields.append(some(reasoning_body(view.items[at].body.text)))
            record(out, fields)
    return out^


def feed(mut view: ChatView, line: String) -> String:
    var json = json_text(line)
    var event = chat_event_decode(json, 0)
    if not event:
        return "!"
    var result = view.apply(event.value())
    if result == 0:
        return "."
    return chr(96 + result)


def chat(item: AgentCase) -> String:
    var view = ChatView()
    var results = String()
    if number(item.fields[2]) > 0:
        for line in text_split(item.fields[0], "\n"):
            results += feed(view, line)
    return dump(view, results, unsigned(item.fields[1]))


def full() raises:
    var view = ChatView()
    for index in range(CHAT_MAX_ITEMS):
        var line = String(
            '{"event":"itemStarted","item":"n',
            index,
            '","kind":"notice","body":{},"at":1}',
        )
        assert_equal(feed(view, line), ".")
    var last = '{"event":"itemStarted","item":"over","kind":"notice","body":{},"at":1}'
    var json = json_text(last)
    var event = chat_event_decode(json, 0)
    assert_equal(view.apply(event.value()), CHAT_REFUSED_FULL)
    assert_equal(len(view.items), CHAT_MAX_ITEMS)
    assert_equal(Int(view.refused), 1)
    assert_equal(Int(view.skipped), 0)
    assert_equal(view.item("over"), -1)


def main() raises:
    var arguments = argv()
    var bytes: List[UInt8]
    with open(String(arguments[1]), "r") as file:
        bytes = file.read_bytes()
    var source = Source(bytes^)
    var total = Int(source.uint(4))
    var chats = 0
    var durations = 0
    var reasonings = 0
    var events = 0
    for _ in range(total):
        var item = read_case(source)
        if item.op == "chat":
            assert_equal(chat(item), item.expected)
            chats += 1
            events += number(item.fields[2])
        elif item.op == "duration":
            assert_equal(duration(unsigned(item.fields[0])), item.expected)
            durations += 1
        elif item.op == "reasoning":
            assert_equal(some(reasoning_body(item.fields[0])), item.expected)
            reasonings += 1
        else:
            raise Error(String("unknown op ", item.op))
    full()
    print(
        String(
            "chat=",
            chats,
            " events=",
            events,
            " duration=",
            durations,
            " reasoning=",
            reasonings,
            " full=1",
        )
    )
