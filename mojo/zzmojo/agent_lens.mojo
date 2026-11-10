from .agent_chat import ChatImage, ChatQuestion, chat_question
from .agent_completion import AgentCommand
from .agent_json import (
    JSON_ARRAY,
    JSON_OBJECT,
    JSON_STRING,
    JSON_TRUE,
    Json,
    base64_bytes,
    json_text,
    utf8_valid,
)
from .agent_settings import AGENT_CLAUDE

comptime AGENT_MAX_COMMANDS = 256

comptime CONNECTION_STARTING: UInt8 = 0
comptime CONNECTION_READY: UInt8 = 1
comptime CONNECTION_RUNNING: UInt8 = 2
comptime CONNECTION_AWAITING: UInt8 = 3
comptime CONNECTION_FAILED: UInt8 = 4

comptime CURSOR_ACCEPTED: UInt8 = 0
comptime CURSOR_STALE: UInt8 = 1
comptime CURSOR_INVALID: UInt8 = 2
comptime CURSOR_GAP: UInt8 = 3
comptime CURSOR_LOST: UInt8 = 4

comptime ALLOW_UNKNOWN: UInt8 = 0
comptime ALLOW_YES: UInt8 = 1
comptime ALLOW_NO: UInt8 = 2

comptime AGENT_WELCOME_DETAIL = "Ask it anything. To use the CLI's own screen, switch the pane to the CLI."
comptime PARKED_PERMISSION_TITLE = "This agent needs your permission"

comptime _SEQ_MAX: UInt64 = 18446744073709551615


def _after(seq: UInt64) -> UInt64:
    return _SEQ_MAX if seq == _SEQ_MAX else seq + 1


def seq_at(first: UInt64, offset: Int) -> UInt64:
    var step = UInt64(offset)
    return _SEQ_MAX if first > _SEQ_MAX - step else first + step


def _command_update(mut json: Json, update: Int):
    if json.kind(update) != JSON_OBJECT:
        json.failed = True
        return
    _ = json.field(update, "sessionUpdate", False)
    _ = json.field(update, "availableCommands", False)


def _envelope(mut json: Json) -> Bool:
    if not json.ok() or json.kind(0) != JSON_OBJECT:
        return False
    _ = json.req_unsigned(0, "seq")
    var item = json.need(0, "item", JSON_STRING)
    if item >= 0 and json.escaped(item):
        return False
    _ = json.def_bool(0, "restoring")
    var capabilities = json.defaulted(0, "capabilities", JSON_OBJECT)
    if capabilities >= 0:
        _ = json.def_bool(capabilities, "list")
        _ = json.def_bool(capabilities, "load")
        _ = json.def_bool(capabilities, "delete")
        _ = json.def_bool(capabilities, "verbs")
        _ = json.def_bool(capabilities, "watch")
    var update = json.maybe(0, "update", JSON_OBJECT)
    if update >= 0:
        _command_update(json, update)
    for replayed in json.def_list(0, "replay"):
        _command_update(json, replayed)
    return not json.failed


def _commands(json: Json, node: Int) -> List[AgentCommand]:
    var commands = List[AgentCommand]()
    if node < 0 or json.kind(node) != JSON_ARRAY:
        return commands^
    var taken = 0
    for command in json.children(node):
        if taken == AGENT_MAX_COMMANDS:
            break
        taken += 1
        var name = json.last_text(command, "name")
        if not name:
            continue
        var description = json.last_text(command, "description")
        var skill = json.last(json.last(json.last(command, "_meta"), "zz"), "skill")
        commands.append(
            AgentCommand(
                name.take(),
                description.take() if description else String(),
                json.last_text(json.last(command, "input"), "hint"),
                skill >= 0 and json.kind(skill) == JSON_TRUE,
            )
        )
    return commands^


struct AgentCursor(Copyable, Movable):
    var last_seq: UInt64
    var replay_pending: Bool
    var bytes: Int
    var count: Int
    var history_supported: Bool
    var session_load_supported: Bool
    var session_delete_supported: Bool
    var verbs_supported: Bool
    var watch: Bool
    var commands: List[AgentCommand]
    var command_session_reset: Bool
    var request: Optional[UInt64]

    def __init__(out self):
        self.last_seq = 0
        self.replay_pending = False
        self.bytes = 0
        self.count = 0
        self.history_supported = False
        self.session_load_supported = False
        self.session_delete_supported = False
        self.verbs_supported = False
        self.watch = False
        self.commands = List[AgentCommand]()
        self.command_session_reset = False
        self.request = None

    def gap(mut self):
        self.request = None
        if not self.replay_pending:
            self.request = Optional[UInt64](self.last_seq)
        self.replay_pending = True

    def admit(mut self, positional: UInt64, mut json: Json, size: Int) -> UInt8:
        self.request = None
        if not _envelope(json):
            if positional > _after(self.last_seq):
                self.gap()
                return CURSOR_LOST
            if positional > self.last_seq:
                self.last_seq = positional
            return CURSOR_INVALID
        var seq = json.req_unsigned(0, "seq")
        var name = json.req_text(0, "item")
        if seq <= self.last_seq:
            return CURSOR_STALE
        if name == "sessionReset" and json.def_bool(0, "restoring"):
            self.last_seq = seq - 1
        if seq > _after(self.last_seq):
            self.gap()
            return CURSOR_GAP
        if name == "ready":
            var capabilities = json.member(0, "capabilities")
            var granted = capabilities >= 0
            self.history_supported = granted and json.def_bool(capabilities, "list")
            self.session_load_supported = granted and json.def_bool(
                capabilities, "load"
            )
            self.session_delete_supported = granted and json.def_bool(
                capabilities, "delete"
            )
            self.verbs_supported = granted and json.def_bool(capabilities, "verbs")
            self.watch = granted and json.def_bool(capabilities, "watch")
        if name == "sessionReset":
            self.count = 0
            self.bytes = 0
            self.commands = List[AgentCommand]()
            self.command_session_reset = True
        elif name == "sessionReady":
            self.command_session_reset = False
        elif name == "sessionSwitched":
            if not self.command_session_reset:
                self.commands = List[AgentCommand]()
            self.command_session_reset = False
        var updates = List[Int]()
        var update = json.maybe(0, "update", JSON_OBJECT)
        if update >= 0:
            updates.append(update)
        for replayed in json.def_list(0, "replay"):
            updates.append(replayed)
        for entry in updates:
            var kind = json.last_text(entry, "sessionUpdate")
            if kind and kind.value() == "available_commands_update":
                self.commands = _commands(json, json.last(entry, "availableCommands"))
        self.count += 1
        self.bytes += size
        self.last_seq = seq
        return CURSOR_ACCEPTED

    def settle(mut self, accepted: Bool):
        if accepted:
            self.replay_pending = False

    def replay_from(mut self) -> UInt64:
        self.replay_pending = True
        return self.last_seq


def item_kind(json: Json, node: Int) -> String:
    var kind = json.last_text(node, "item")
    return kind.take() if kind else String()


def conversation_changed(json: Json, node: Int) -> Bool:
    var kind = item_kind(json, node)
    return kind == "sessionReset" or kind == "sessionSwitched"


@fieldwise_init
struct PermissionChoice(Copyable, Movable):
    var id: String
    var name: String
    var allow: UInt8


@fieldwise_init
struct PendingPermission(Copyable, Movable):
    var request_id: UInt64
    var title: String
    var options: List[PermissionChoice]
    var questions: List[ChatQuestion]


def permission_choices(json: Json, node: Int) -> List[PermissionChoice]:
    var choices = List[PermissionChoice]()
    for option in json.last_list(node, "options"):
        var found = json.last(option, "optionId")
        if found < 0:
            found = json.last(option, "id")
        if found < 0 or json.kind(found) != JSON_STRING:
            continue
        var id = json.text(found)
        var name = json.last_text(option, "name")
        var kind = json.last_text(option, "kind")
        var allow = ALLOW_UNKNOWN
        if kind:
            if kind.value() == "allow_once" or kind.value() == "allow_always":
                allow = ALLOW_YES
            elif kind.value() == "reject_once" or kind.value() == "reject_always":
                allow = ALLOW_NO
        choices.append(
            PermissionChoice(id.copy(), name.take() if name else id.copy(), allow)
        )
    return choices^


def permission_questions(mut json: Json, node: Int) -> List[ChatQuestion]:
    var questions = List[ChatQuestion]()
    var found = json.last(node, "questions")
    if found < 0 or json.kind(found) != JSON_ARRAY:
        return questions^
    var before = json.failed
    json.failed = False
    for question in json.children(found):
        if json.kind(question) != JSON_OBJECT:
            json.failed = True
            break
        questions.append(chat_question(json, question))
    var broken = json.failed
    json.failed = before
    if broken:
        return List[ChatQuestion]()
    return questions^


def parked_permission(request_id: UInt64, payload: String) -> PendingPermission:
    var json = json_text(payload)
    var node = 0 if json.ok() else -1
    var title = json.last_text(json.last(node, "tool_call"), "title")
    if json.last(json.last(node, "tool_call"), "title") < 0:
        title = json.last_text(json.last(node, "toolCall"), "title")
    return PendingPermission(
        request_id,
        title.take() if title else String(PARKED_PERMISSION_TITLE),
        permission_choices(json, node),
        permission_questions(json, node),
    )


@fieldwise_init
struct RestoredPrompt(Copyable, Movable):
    var text: String
    var images: List[ChatImage]


def _restored(json: Json, prompt: Int, client: Optional[UInt64]) -> Optional[RestoredPrompt]:
    var owner: UInt64 = 0
    if json.last(prompt, "owner") >= 0:
        var found = json.last_unsigned(prompt, "owner")
        if not found:
            return None
        owner = found.value()
    if owner != 0 and (not client or client.value() != owner):
        return None
    var encoded = json.last_text(prompt, "text")
    if not encoded:
        return None
    var bytes = base64_bytes(encoded.value())
    if not bytes or not utf8_valid(bytes.value()):
        return None
    var images = List[ChatImage]()
    for image in json.last_list(prompt, "images"):
        var format = json.last_text(image, "format")
        var data = json.last_text(image, "data")
        if not format or not data:
            return None
        var decoded = base64_bytes(data.value())
        if not decoded:
            return None
        images.append(ChatImage(format.take(), decoded.take()))
    return Optional[RestoredPrompt](
        RestoredPrompt(String(from_utf8_lossy=Span(bytes.value())), images^)
    )


def restored_prompts(
    json: Json, node: Int, client: Optional[UInt64]
) -> List[RestoredPrompt]:
    var prompts = List[RestoredPrompt]()
    for prompt in json.last_list(node, "prompts"):
        var found = _restored(json, prompt, client)
        if found:
            prompts.append(found.take())
    return prompts^


@fieldwise_init
struct Reclaim(Copyable, Movable):
    var restores: Bool
    var id: Optional[UInt64]


def restore_reclaim_id(json: Json, node: Int) -> Reclaim:
    var kind = item_kind(json, node)
    if kind != "promptsReclaimed" and kind != "promptsRestored":
        return Reclaim(False, None)
    if json.last(node, "reclaim_id") >= 0:
        return Reclaim(True, json.last_unsigned(node, "reclaim_id"))
    return Reclaim(True, json.last_unsigned(node, "reclaimId"))


@fieldwise_init
struct AgentUsage(Copyable, Movable):
    var known: Bool
    var used: UInt64
    var size: UInt64


def usage_from_update(json: Json, node: Int) -> AgentUsage:
    var update = json.last(node, "update")
    if update < 0:
        update = node
    var kind = json.last_text(update, "sessionUpdate")
    if not kind or kind.value() != "usage_update":
        return AgentUsage(False, 0, 0)
    var used = json.last_unsigned(update, "used")
    var size = json.last_unsigned(update, "size")
    if not used or not size:
        return AgentUsage(False, 0, 0)
    return AgentUsage(True, used.value(), size.value())


def track_usage(mut usage: AgentUsage, json: Json, node: Int):
    if conversation_changed(json, node):
        usage = AgentUsage(False, 0, 0)
        for replayed in json.last_list(node, "replay"):
            track_usage(usage, json, replayed)
    var found = usage_from_update(json, node)
    if found.known:
        usage = found^


def turn_running(phase: UInt8) -> Bool:
    return phase == CONNECTION_RUNNING or phase == CONNECTION_AWAITING


def accepts_prompt(phase: UInt8, parked: Bool) -> Bool:
    return turn_running(phase) or (phase == CONNECTION_READY and not parked)


def provider_label(provider: UInt8) -> String:
    return "Claude Code" if provider == AGENT_CLAUDE else "Codex"


def _agent(provider: Optional[UInt8]) -> String:
    return provider_label(provider.value()) if provider else String("the agent")


def agent_welcome_title(provider: UInt8) -> String:
    return String(provider_label(provider), " is ready")


def agent_placeholder(provider: Optional[UInt8]) -> String:
    return String("Message ", _agent(provider), "…")


@fieldwise_init
struct AgentEmpty(Copyable, Movable):
    var welcome: Bool
    var text: String
    var busy: Bool


def agent_empty(connected: Bool, phase: UInt8, provider: Optional[UInt8]) -> AgentEmpty:
    var agent = _agent(provider)
    if not connected:
        return AgentEmpty(False, "The agent is offline.", False)
    var text: String
    if phase == CONNECTION_STARTING:
        text = String("Starting ", agent, "…")
    elif phase == CONNECTION_READY:
        if provider:
            return AgentEmpty(True, agent_welcome_title(provider.value()), False)
        text = String(agent, " is ready.")
    elif phase == CONNECTION_RUNNING:
        text = String("Waiting for ", agent, "’s first update…")
    elif phase == CONNECTION_AWAITING:
        text = "The agent needs your permission."
    else:
        text = String(agent, " stopped.")
    return AgentEmpty(
        False, text^, phase == CONNECTION_STARTING or phase == CONNECTION_RUNNING
    )
