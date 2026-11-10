from std.sys import argv
from browser_fixture import Source
from agent_fixture import AgentCase, bit, number, read_case, some, unsigned
from zzmojo.agent_text import text_join, text_slice, text_split
from zzmojo.agent_chat import ChatQuestion, ChatView
from zzmojo.agent_completion import AgentCommand
from zzmojo.agent_json import json_text
from zzmojo.agent_events import AgentFeed, AgentPaneState, FeedResult
from zzmojo.agent_lens import (
    AGENT_WELCOME_DETAIL,
    ALLOW_NO,
    ALLOW_YES,
    AgentCursor,
    AgentUsage,
    PendingPermission,
    RestoredPrompt,
    accepts_prompt,
    agent_empty,
    agent_placeholder,
    conversation_changed,
    parked_permission,
    restore_reclaim_id,
    restored_prompts,
    track_usage,
    usage_from_update,
)
from zzmojo.agent_settings import LensSettings

comptime FIELD = "\x1f"
comptime RECORD = "\x1e"
comptime ONE = "\x1d"
comptime TWO = "\x1c"
comptime THREE = "\x1b"
comptime FOUR = "\x1a"
comptime FIVE = "\x19"
comptime SIX = "\x18"
comptime HEX = "0123456789abcdef"


def listed(parts: List[String], separator: String) -> String:
    var out = String(len(parts))
    for part in parts:
        out += separator
        out += part
    return out^


def unlisted(text: String, separator: String) -> List[String]:
    var parts = text_split(text, separator)
    var found = List[String]()
    for index in range(1, len(parts)):
        found.append(parts[index].copy())
    return found^


def optional(text: String) -> Optional[String]:
    if text.byte_length() == 0 or text.as_bytes()[0] != 83:
        return None
    return Optional[String](text_slice(text, 1, text.byte_length()))


def counted(text: String) -> Optional[UInt64]:
    var found = optional(text)
    if not found:
        return None
    return Optional[UInt64](unsigned(found.value()))


def same(op: String, index: Int, actual: String, expected: String) raises:
    if actual == expected:
        return
    var shared = min(actual.byte_length(), expected.byte_length())
    var at = 0
    while at < shared and actual.as_bytes()[at] == expected.as_bytes()[at]:
        at += 1
    var start = max(at - 100, 0)
    raise Error(
        String(
            op,
            " case ",
            index,
            " differs at byte ",
            at,
            "\n  actual:   ",
            repr(text_slice(actual, start, at + 160)),
            "\n  expected: ",
            repr(text_slice(expected, start, at + 160)),
        )
    )


def hex(bytes: List[UInt8]) -> String:
    var out = String()
    for byte in bytes:
        out += text_slice(HEX, Int(byte >> 4), Int(byte >> 4) + 1)
        out += text_slice(HEX, Int(byte & 15), Int(byte & 15) + 1)
    return out^


def requested(request: Optional[UInt64]) -> String:
    if request:
        return String("S", request.value())
    return "N"


def commands_text(commands: List[AgentCommand]) -> String:
    var parts = List[String]()
    for command in commands:
        var row: List[String] = [
            command.name.copy(),
            command.description.copy(),
            some(command.input_hint),
            bit(command.skill),
        ]
        parts.append(text_join(row, THREE))
    return listed(parts, TWO)


def cursor_state(cursor: AgentCursor) -> List[String]:
    var flags = String(
        bit(cursor.history_supported),
        bit(cursor.session_load_supported),
        bit(cursor.session_delete_supported),
        bit(cursor.verbs_supported),
        bit(cursor.watch),
    )
    var state: List[String] = [
        String(cursor.last_seq),
        bit(cursor.replay_pending),
        flags^,
        commands_text(cursor.commands),
        String(cursor.bytes),
    ]
    return state^


def question_text(question: ChatQuestion) -> String:
    var options = List[String]()
    for option in question.options:
        options.append(String(option.label, SIX, some(option.description)))
    var row: List[String] = [
        question.id.copy(),
        some(question.header),
        question.question.copy(),
        listed(options, FIVE),
        bit(question.multi_select),
        bit(question.allow_other),
        bit(question.secret),
    ]
    return text_join(row, FOUR)


def pending_text(pending: PendingPermission) -> String:
    var options = List[String]()
    for option in pending.options:
        var allow = "N"
        if option.allow == ALLOW_YES:
            allow = "1"
        elif option.allow == ALLOW_NO:
            allow = "0"
        options.append(String(option.id, FOUR, option.name, FOUR, allow))
    var questions = List[String]()
    for question in pending.questions:
        questions.append(question_text(question))
    var row: List[String] = [
        String(pending.request_id),
        pending.title.copy(),
        listed(options, THREE),
        listed(questions, THREE),
    ]
    return text_join(row, TWO)


def restored_text(prompts: List[RestoredPrompt]) -> String:
    var parts = List[String]()
    for prompt in prompts:
        var images = List[String]()
        for image in prompt.images:
            images.append(String(image.format, FOUR, hex(image.data)))
        parts.append(String(prompt.text, TWO, listed(images, THREE)))
    return listed(parts, ONE)


def usage_text(usage: AgentUsage) -> String:
    if not usage.known:
        return "N"
    return String(usage.used, ",", usage.size)


def settings_text(settings: Optional[LensSettings]) -> String:
    if not settings:
        return "N"
    return String(
        "S",
        settings.value().model,
        ONE,
        settings.value().model_label,
        ONE,
        settings.value().effort,
        ONE,
        settings.value().summary(),
    )


def chat_text(view: ChatView) -> String:
    var requests = List[String]()
    for request in view.requests:
        requests.append(String(request.id, ":", bit(request.is_open())))
    var row: List[String] = [
        String(len(view.turns)),
        String(len(view.items)),
        listed(requests, TWO),
        String(len(view.queue)),
        String(view.skipped),
        String(view.phase()),
    ]
    return text_join(row, ONE)


def items_of(batch: String) -> List[List[UInt8]]:
    var parts = text_split(batch, ONE)
    var items = List[List[UInt8]]()
    for index in range(2, len(parts)):
        var bytes = List[UInt8]()
        for byte in parts[index].as_bytes():
            bytes.append(byte)
        items.append(bytes^)
    return items^


def first_of(batch: String) -> UInt64:
    return unsigned(text_split(batch, ONE)[0])


def cursor(item: AgentCase) -> String:
    var feed = AgentFeed()
    var rows = List[String]()
    for batch in unlisted(item.fields[0], RECORD):
        var result = feed.updates(first_of(batch), items_of(batch))
        var row: List[String] = [requested(result.replay), bit(result.invalid)]
        for part in cursor_state(feed.cursor):
            row.append(part.copy())
        row.append(String(feed.cursor.count))
        rows.append(text_join(row, FIELD))
    return text_join(rows, RECORD)


def feed(item: AgentCase) -> String:
    var feed = AgentFeed(counted(item.fields[0]))
    var parked = counted(item.fields[1])
    if parked:
        var state = AgentPaneState()
        state.parked = True
        state.permission_id = parked.value()
        state.permission_payload = item.fields[2].copy()
        _ = feed.set_state(state)
    var rows = List[String]()
    for batch in unlisted(item.fields[3], RECORD):
        var result = feed.updates(first_of(batch), items_of(batch))
        var row: List[String] = [
            requested(result.replay),
            bit(result.invalid),
            String(result.accepted),
            bit(result.restarted),
            bit(result.switched),
        ]
        for part in cursor_state(feed.cursor):
            row.append(part.copy())
        row.append(usage_text(feed.usage))
        row.append(settings_text(feed.settings))
        row.append(chat_text(feed.chat))
        var waiting = List[String]()
        for pending in feed.permissions():
            waiting.append(pending_text(pending))
        row.append(listed(waiting, ONE))
        row.append(restored_text(feed.take_restored()))
        var acknowledged = List[String]()
        for id in feed.take_acknowledged():
            acknowledged.append(String(id))
        row.append(listed(acknowledged, ","))
        rows.append(text_join(row, FIELD))
    return text_join(rows, RECORD)


def parked(item: AgentCase) -> String:
    var found: List[String] = [
        pending_text(parked_permission(unsigned(item.fields[0]), item.fields[1]))
    ]
    return listed(found, ONE)


def reclaim(item: AgentCase) -> String:
    var json = json_text(item.fields[0])
    var found = restore_reclaim_id(json, 0)
    if not found.restores:
        return "N"
    return String("S", requested(found.id))


def tracked(item: AgentCase) -> String:
    var usage = AgentUsage(False, 0, 0)
    for text in unlisted(item.fields[0], RECORD):
        var json = json_text(text)
        track_usage(usage, json, 0)
    return usage_text(usage)


def provider(text: String) -> Optional[UInt8]:
    if text == "N":
        return None
    return Optional[UInt8](UInt8(number(text)))


def empty(item: AgentCase) -> String:
    var found = agent_empty(
        item.fields[0] == "1", UInt8(number(item.fields[1])), provider(item.fields[2])
    )
    if found.welcome:
        return String("W", found.text)
    return String("M", found.text, FIELD, bit(found.busy))


def run(item: AgentCase) raises -> String:
    if item.op == "cursor":
        return cursor(item)
    if item.op == "feed":
        return feed(item)
    if item.op == "parked":
        return parked(item)
    if item.op == "restored":
        var json = json_text(item.fields[0])
        return restored_text(restored_prompts(json, 0, counted(item.fields[1])))
    if item.op == "reclaim":
        return reclaim(item)
    if item.op == "changed":
        var json = json_text(item.fields[0])
        return bit(conversation_changed(json, 0))
    if item.op == "usage":
        var json = json_text(item.fields[0])
        return usage_text(usage_from_update(json, 0))
    if item.op == "track":
        return tracked(item)
    if item.op == "accepts":
        return bit(
            accepts_prompt(UInt8(number(item.fields[0])), item.fields[1] == "1")
        )
    if item.op == "empty":
        return empty(item)
    if item.op == "placeholder":
        return agent_placeholder(provider(item.fields[0]))
    if item.op == "detail":
        return AGENT_WELCOME_DETAIL
    raise Error(String("unknown op ", item.op))


def main() raises:
    var arguments = argv()
    var bytes: List[UInt8]
    with open(String(arguments[1]), "r") as file:
        bytes = file.read_bytes()
    var source = Source(bytes^)
    var total = Int(source.uint(4))
    var names: List[String] = [
        "cursor",
        "feed",
        "parked",
        "restored",
        "reclaim",
        "changed",
        "usage",
        "track",
        "accepts",
        "empty",
        "placeholder",
        "detail",
    ]
    var counts = List[Int]()
    for _ in range(len(names)):
        counts.append(0)
    for index in range(total):
        var item = read_case(source)
        same(item.op, index, run(item), item.expected)
        for slot in range(len(names)):
            if names[slot] == item.op:
                counts[slot] += 1
    var out = String()
    for slot in range(len(names)):
        if slot > 0:
            out += " "
        out += String(names[slot], "=", counts[slot])
    print(out)
