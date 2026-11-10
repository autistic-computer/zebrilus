from .agent_chat import ChatView, chat_event_decode
from .agent_json import JSON_ARRAY, JSON_OBJECT, JSON_STRING, Json, json_parse
from .agent_lens import (
    ALLOW_NO,
    ALLOW_YES,
    CONNECTION_FAILED,
    CONNECTION_STARTING,
    CURSOR_GAP,
    CURSOR_INVALID,
    CURSOR_LOST,
    CURSOR_STALE,
    AgentCursor,
    AgentUsage,
    PendingPermission,
    PermissionChoice,
    RestoredPrompt,
    conversation_changed,
    item_kind,
    parked_permission,
    permission_questions,
    restore_reclaim_id,
    restored_prompts,
    seq_at,
    track_usage,
)
from .agent_settings import LensSettings, settings_from_json, settings_parse

comptime FEED_OK: UInt8 = 0
comptime FEED_REFUSED_BATCH: UInt8 = 1
comptime FEED_REFUSED_STATE: UInt8 = 2

comptime FEED_MAX_BATCH_BYTES = 9 * 1024 * 1024
comptime FEED_MAX_PENDING = 64
comptime FEED_MAX_OPTIONS = 32
comptime FEED_MAX_RESTORED = 64
comptime FEED_MAX_RESTORED_BYTES = 6 * 1024 * 1024
comptime FEED_MAX_QUEUED = 4
comptime FEED_MAX_SESSION_BYTES = 16 * 1024
comptime FEED_MAX_BLOB_BYTES = 256 * 1024
comptime FEED_MAX_PERMISSION_BYTES = 64 * 1024

comptime FEED_TOOL_TITLE = "Tool approval"


struct AgentPaneState(Copyable, Movable):
    var phase: UInt8
    var failure: String
    var queued_prompts: Int
    var session_id: Optional[String]
    var title: Optional[String]
    var error: Optional[String]
    var config_options: String
    var parked: Bool
    var permission_id: UInt64
    var permission_payload: String

    def __init__(out self):
        self.phase = CONNECTION_STARTING
        self.failure = String()
        self.queued_prompts = 0
        self.session_id = None
        self.title = None
        self.error = None
        self.config_options = String()
        self.parked = False
        self.permission_id = 0
        self.permission_payload = String()

    def fits(self) -> Bool:
        if self.phase > CONNECTION_FAILED:
            return False
        if self.queued_prompts < 0 or self.queued_prompts > FEED_MAX_QUEUED:
            return False
        if self.failure.byte_length() > FEED_MAX_BLOB_BYTES:
            return False
        if self.config_options.byte_length() > FEED_MAX_BLOB_BYTES:
            return False
        if self.permission_payload.byte_length() > FEED_MAX_PERMISSION_BYTES:
            return False
        if (
            self.session_id
            and self.session_id.value().byte_length() > FEED_MAX_SESSION_BYTES
        ):
            return False
        if self.title and self.title.value().byte_length() > FEED_MAX_BLOB_BYTES:
            return False
        if self.error and self.error.value().byte_length() > FEED_MAX_BLOB_BYTES:
            return False
        return True


@fieldwise_init
struct FeedResult(Copyable, Movable):
    var refusal: UInt8
    var replay: Optional[UInt64]
    var invalid: Bool
    var restarted: Bool
    var switched: Bool
    var accepted: Int


def _stream_choices(json: Json, options: Int) -> Optional[List[PermissionChoice]]:
    var choices = List[PermissionChoice]()
    for option in json.children(options):
        var id = json.last_text(option, "optionId")
        var name = json.last_text(option, "name")
        var kind = json.last_text(option, "kind")
        if not id or not name or not kind:
            return None
        var allow: UInt8
        if kind.value() == "allow_once" or kind.value() == "allow_always":
            allow = ALLOW_YES
        elif kind.value() == "reject_once" or kind.value() == "reject_always":
            allow = ALLOW_NO
        else:
            return None
        if len(choices) < FEED_MAX_OPTIONS:
            choices.append(PermissionChoice(id.take(), name.take(), allow))
    return Optional[List[PermissionChoice]](choices^)


struct AgentFeed(Movable):
    var cursor: AgentCursor
    var chat: ChatView
    var settings: Optional[LensSettings]
    var session_reset: Bool
    var usage: AgentUsage
    var state: AgentPaneState
    var connected: Bool
    var pending: List[PendingPermission]
    var restored: List[RestoredPrompt]
    var restored_bytes: Int
    var acknowledged: List[UInt64]
    var last_reclaim: UInt64
    var client: Optional[UInt64]
    var revision: Int
    var refused_batches: Int
    var refused_states: Int
    var refused_pending: Int
    var refused_restores: Int

    def __init__(out self, client: Optional[UInt64] = None):
        self.cursor = AgentCursor()
        self.chat = ChatView()
        self.settings = None
        self.session_reset = False
        self.usage = AgentUsage(False, 0, 0)
        self.state = AgentPaneState()
        self.connected = False
        self.pending = List[PendingPermission]()
        self.restored = List[RestoredPrompt]()
        self.restored_bytes = 0
        self.acknowledged = List[UInt64]()
        self.last_reclaim = 0
        self.client = client
        self.revision = 0
        self.refused_batches = 0
        self.refused_states = 0
        self.refused_pending = 0
        self.refused_restores = 0

    def set_state(mut self, state: AgentPaneState) -> UInt8:
        if not state.fits():
            self.refused_states += 1
            return FEED_REFUSED_STATE
        self.state = state.copy()
        self.connected = True
        if state.config_options.byte_length() > 0:
            var found = settings_from_json(state.config_options)
            if found:
                self.settings = found.take()
        self.revision += 1
        return FEED_OK

    def disconnect(mut self):
        self.connected = False
        self.revision += 1

    def lagged(mut self) -> UInt64:
        return self.cursor.replay_from()

    def updates(mut self, first_seq: UInt64, items: List[List[UInt8]]) -> FeedResult:
        var result = FeedResult(FEED_OK, None, False, False, False, 0)
        var total = 0
        for item in items:
            total += len(item)
        if total > FEED_MAX_BATCH_BYTES:
            self.refused_batches += 1
            result.refusal = FEED_REFUSED_BATCH
            return result^
        for offset in range(len(items)):
            var json = json_parse(items[offset])
            var step = self.cursor.admit(
                seq_at(first_seq, offset), json, len(items[offset])
            )
            if step == CURSOR_GAP or step == CURSOR_LOST:
                result.replay = self.cursor.request
                if step == CURSOR_LOST:
                    result.invalid = True
                if result.accepted > 0:
                    self.revision += 1
                return result^
            if step == CURSOR_INVALID:
                result.invalid = True
                continue
            if step == CURSOR_STALE:
                continue
            result.accepted += 1
            if conversation_changed(json, 0):
                result.switched = True
            if self.consume(json):
                result.restarted = True
        self.cursor.settle(result.accepted > 0)
        if result.accepted > 0:
            self.revision += 1
        return result^

    def consume(mut self, mut json: Json) -> Bool:
        self.restore(json)
        track_usage(self.usage, json, 0)
        var kind = item_kind(json, 0)
        if (
            kind == "sessionReady"
            or kind == "sessionSwitched"
            or kind == "configOptionsChanged"
        ):
            var options = json.last(0, "config_options")
            if options < 0:
                options = json.last(0, "configOptions")
            var found = settings_parse(json, options)
            if found:
                self.settings = found.take()
        var restarted = False
        if kind == "sessionReset":
            self.pending.clear()
            restarted = True
            self.session_reset = True
        elif kind == "sessionReady":
            self.session_reset = False
            self.pending.clear()
        elif kind == "sessionSwitched":
            if not self.session_reset:
                self.pending.clear()
                restarted = True
            self.session_reset = False
        elif kind == "chat":
            var event = chat_event_decode(json, json.last(0, "event"))
            json.failed = False
            if event:
                _ = self.chat.apply(event.value())
        elif kind == "permissionRequested":
            self.request(json)
        elif kind == "permissionResolved":
            var resolved = json.last_unsigned(0, "request_id")
            if resolved:
                self.resolve(resolved.value())
        elif kind == "promptFinished" or kind == "paneFailed":
            self.pending.clear()
        return restarted

    def restore(mut self, json: Json):
        var reclaim = restore_reclaim_id(json, 0)
        if not reclaim.restores:
            return
        if reclaim.id and reclaim.id.value() <= self.last_reclaim:
            return
        var prompts = restored_prompts(json, 0, self.client)
        if len(prompts) == 0:
            return
        var size = 0
        for prompt in prompts:
            size += prompt.text.byte_length()
            for image in prompt.images:
                size += len(image.data)
        if (
            len(self.restored) + len(prompts) > FEED_MAX_RESTORED
            or self.restored_bytes + size > FEED_MAX_RESTORED_BYTES
        ):
            self.refused_restores += 1
            return
        for prompt in prompts:
            self.restored.append(prompt.copy())
        self.restored_bytes += size
        if reclaim.id:
            self.last_reclaim = reclaim.id.value()
            self.acknowledged.append(reclaim.id.value())

    def request(mut self, mut json: Json):
        var id = json.last_unsigned(0, "request_id")
        var tool = json.last(0, "tool_call")
        var options = json.last(0, "options")
        if not id or tool < 0 or options < 0:
            return
        if json.kind(tool) != JSON_OBJECT or json.kind(options) != JSON_ARRAY:
            return
        if not json.last_text(tool, "toolCallId"):
            return
        var choices = _stream_choices(json, options)
        if not choices:
            return
        var title = json.last_text(tool, "title")
        var request = PendingPermission(
            id.value(),
            title.take() if title else String(FEED_TOOL_TITLE),
            choices.take(),
            permission_questions(json, 0),
        )
        for index in range(len(self.pending)):
            if self.pending[index].request_id == request.request_id:
                self.pending[index] = request^
                return
        if len(self.pending) >= FEED_MAX_PENDING:
            self.refused_pending += 1
            return
        self.pending.append(request^)

    def resolve(mut self, request_id: UInt64):
        var kept = List[PendingPermission]()
        for request in self.pending:
            if request.request_id != request_id:
                kept.append(request.copy())
        self.pending = kept^

    def permissions(self) -> List[PendingPermission]:
        var found = self.pending.copy()
        if self.connected and self.state.parked:
            for request in found:
                if request.request_id == self.state.permission_id:
                    return found^
            found.append(
                parked_permission(
                    self.state.permission_id, self.state.permission_payload
                )
            )
        return found^

    def take_restored(mut self) -> List[RestoredPrompt]:
        var found = self.restored.copy()
        self.restored = List[RestoredPrompt]()
        self.restored_bytes = 0
        return found^

    def take_acknowledged(mut self) -> List[UInt64]:
        var found = self.acknowledged.copy()
        self.acknowledged = List[UInt64]()
        return found^
