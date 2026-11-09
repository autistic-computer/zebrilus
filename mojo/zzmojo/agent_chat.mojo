from .agent_json import (
    JSON_OBJECT,
    JSON_STRING,
    Json,
    base64_bytes,
)
from .agent_text import text_split

comptime ITEM_USER: UInt8 = 0
comptime ITEM_AGENT: UInt8 = 1
comptime ITEM_REASONING: UInt8 = 2
comptime ITEM_PLAN: UInt8 = 3
comptime ITEM_NOTICE: UInt8 = 4
comptime ITEM_ERROR: UInt8 = 5
comptime ITEM_SUBAGENT: UInt8 = 6
comptime ITEM_READ: UInt8 = 7
comptime ITEM_EDIT: UInt8 = 8
comptime ITEM_SEARCH: UInt8 = 9
comptime ITEM_EXECUTE: UInt8 = 10
comptime ITEM_FETCH: UInt8 = 11
comptime ITEM_WEB: UInt8 = 12
comptime ITEM_MCP: UInt8 = 13
comptime ITEM_TOOL: UInt8 = 14
comptime ITEM_KINDS = "user,agent,reasoning,plan,notice,error,subagent,read,edit,search,execute,fetch,web,mcp,tool"

comptime STATUS_RUNNING: UInt8 = 0
comptime STATUS_COMPLETED: UInt8 = 1
comptime STATUS_FAILED: UInt8 = 2
comptime STATUS_DECLINED: UInt8 = 3
comptime STATUS_INTERRUPTED: UInt8 = 4
comptime ITEM_STATUSES = "running,completed,failed,declined,interrupted"

comptime PLAN_STEP_PENDING: UInt8 = 0
comptime PLAN_STEP_RUNNING: UInt8 = 1
comptime PLAN_STEP_DONE: UInt8 = 2
comptime PLAN_STEP_CANCELLED: UInt8 = 3
comptime STEP_STATUSES = "pending,running,done,cancelled"

comptime OPTION_ALLOW: UInt8 = 0
comptime OPTION_ALLOW_ALWAYS: UInt8 = 1
comptime OPTION_DENY: UInt8 = 2
comptime OPTION_OTHER: UInt8 = 3
comptime OPTION_KINDS = "allow,allowAlways,deny,other"

comptime ASK_APPROVAL: UInt8 = 0
comptime ASK_QUESTIONS: UInt8 = 1
comptime ASK_PLAN: UInt8 = 2
comptime ASK_KINDS = "approval,questions,plan"

comptime CLOSE_ANSWERED: UInt8 = 0
comptime CLOSE_WITHDRAWN: UInt8 = 1
comptime CLOSE_DISMISSED: UInt8 = 2
comptime CLOSE_KINDS = "answered,withdrawn,dismissed"

comptime ANSWER_OPTION: UInt8 = 0
comptime ANSWER_QUESTIONS: UInt8 = 1
comptime ANSWER_CANCEL: UInt8 = 2
comptime ANSWER_KINDS = "option,questions,cancel"

comptime OUTCOME_COMPLETED: UInt8 = 0
comptime OUTCOME_CANCELLED: UInt8 = 1
comptime OUTCOME_FAILED: UInt8 = 2
comptime OUTCOME_DROPPED: UInt8 = 3
comptime OUTCOME_OPEN: UInt8 = 255
comptime OUTCOME_KINDS = "completed,cancelled,failed,dropped"

comptime EVENT_SESSION_READY: UInt8 = 0
comptime EVENT_SESSION_ENDED: UInt8 = 1
comptime EVENT_TURN_STARTED: UInt8 = 2
comptime EVENT_TURN_CLAIMED: UInt8 = 3
comptime EVENT_TURN_STOPPING: UInt8 = 4
comptime EVENT_TURN_ENDED: UInt8 = 5
comptime EVENT_TURN_ACTIVITY: UInt8 = 6
comptime EVENT_ITEM_STARTED: UInt8 = 7
comptime EVENT_ITEM_APPENDED: UInt8 = 8
comptime EVENT_ITEM_CHANGED: UInt8 = 9
comptime EVENT_ITEM_BACKGROUNDED: UInt8 = 10
comptime EVENT_ITEM_ENDED: UInt8 = 11
comptime EVENT_ITEMS_SUMMARIZED: UInt8 = 12
comptime EVENT_REQUEST_OPENED: UInt8 = 13
comptime EVENT_REQUEST_CLOSED: UInt8 = 14
comptime EVENT_QUEUE_CHANGED: UInt8 = 15
comptime EVENT_USAGE_CHANGED: UInt8 = 16
comptime EVENT_RESTART: UInt8 = 17
comptime EVENT_KINDS = "sessionReady,sessionEnded,turnStarted,turnClaimed,turnStopping,turnEnded,turnActivity,itemStarted,itemAppended,itemChanged,itemBackgrounded,itemEnded,itemsSummarized,requestOpened,requestClosed,queueChanged,usageChanged,restart"

comptime SESSION_STARTING: UInt8 = 0
comptime SESSION_READY: UInt8 = 1
comptime SESSION_CLOSED: UInt8 = 2
comptime SESSION_EXITED: UInt8 = 3

comptime PHASE_STARTING: UInt8 = 0
comptime PHASE_IDLE: UInt8 = 1
comptime PHASE_RUNNING: UInt8 = 2
comptime PHASE_WAITING: UInt8 = 3
comptime PHASE_STOPPING: UInt8 = 4
comptime PHASE_ENDED: UInt8 = 5

comptime CHAT_APPLIED = 0
comptime SKIP_TURN_STILL_OPEN = 1
comptime SKIP_NOT_RUNNING = 2
comptime SKIP_UNKNOWN_TURN = 3
comptime SKIP_UNKNOWN_ITEM = 4
comptime SKIP_DUPLICATE_ITEM = 5
comptime SKIP_ITEM_CLOSED = 6
comptime SKIP_PARENT_CLOSED = 7
comptime SKIP_CHILDREN_OPEN = 8
comptime SKIP_STILL_RUNNING = 9
comptime SKIP_UNKNOWN_REQUEST = 10
comptime SKIP_DUPLICATE_REQUEST = 11
comptime SKIP_REQUEST_CLOSED = 12
comptime SKIP_REQUESTS_OPEN = 13
comptime CHAT_REFUSED_FULL = 14

comptime CHAT_KEPT_IMAGES = 8
comptime CHAT_MAX_ITEMS = 10000
comptime CHAT_MAX_TURNS = 10000
comptime CHAT_MAX_REQUESTS = 10000
comptime CHAT_MAX_BYTES = 32 * 1024 * 1024


def is_text_kind(kind: UInt8) -> Bool:
    return kind <= ITEM_ERROR


def is_tool_kind(kind: UInt8) -> Bool:
    return kind >= ITEM_READ and kind <= ITEM_TOOL


@fieldwise_init
struct PlanStep(Copyable, Movable):
    var text: String
    var status: UInt8


@fieldwise_init
struct ChatImage(Copyable, Movable):
    var format: String
    var data: List[UInt8]


struct ItemBody(Copyable, Movable):
    var title: String
    var target: Optional[String]
    var text: String
    var steps: List[PlanStep]
    var exit_code: Optional[Int]
    var images: List[ChatImage]
    var activity: Optional[String]

    def __init__(out self):
        self.title = String()
        self.target = None
        self.text = String()
        self.steps = List[PlanStep]()
        self.exit_code = None
        self.images = List[ChatImage]()
        self.activity = None

    def bytes(self) -> Int:
        var total = self.title.byte_length() + self.text.byte_length()
        if self.target:
            total += self.target.value().byte_length()
        if self.activity:
            total += self.activity.value().byte_length()
        for step in self.steps:
            total += step.text.byte_length()
        for image in self.images:
            total += len(image.data)
        return total


@fieldwise_init
struct AskOption(Copyable, Movable):
    var id: String
    var label: String
    var kind: UInt8


@fieldwise_init
struct ChatChoice(Copyable, Movable):
    var label: String
    var description: Optional[String]


@fieldwise_init
struct ChatQuestion(Copyable, Movable):
    var id: String
    var header: Optional[String]
    var question: String
    var options: List[ChatChoice]
    var multi_select: Bool
    var allow_other: Bool
    var secret: Bool


struct Ask(Copyable, Movable):
    var kind: UInt8
    var title: String
    var detail: Optional[String]
    var options: List[AskOption]
    var questions: List[ChatQuestion]
    var markdown: String

    def __init__(out self):
        self.kind = ASK_APPROVAL
        self.title = String()
        self.detail = None
        self.options = List[AskOption]()
        self.questions = List[ChatQuestion]()
        self.markdown = String()


@fieldwise_init
struct QuestionAnswer(Copyable, Movable):
    var question: String
    var answers: List[String]


struct RequestClose(Copyable, Movable):
    var kind: UInt8
    var answer: UInt8
    var id: String
    var answers: List[QuestionAnswer]

    def __init__(out self):
        self.kind = CLOSE_DISMISSED
        self.answer = ANSWER_CANCEL
        self.id = String()
        self.answers = List[QuestionAnswer]()


@fieldwise_init
struct QueuedPrompt(Copyable, Movable):
    var id: UInt64
    var text: String


struct ChatEvent(Copyable, Movable):
    var kind: UInt8
    var session: String
    var exited: Bool
    var turn: Optional[UInt64]
    var by_user: Bool
    var prompt: UInt64
    var at: UInt64
    var outcome: UInt8
    var detail: String
    var text: Optional[String]
    var item: String
    var parent: Optional[String]
    var linked: Optional[String]
    var item_kind: UInt8
    var body: ItemBody
    var status: UInt8
    var message: Optional[String]
    var items: List[String]
    var request: UInt64
    var ask: Ask
    var blocking: Bool
    var close: RequestClose
    var prompts: List[QueuedPrompt]
    var used: UInt64
    var size: UInt64

    def __init__(out self, kind: UInt8):
        self.kind = kind
        self.session = String()
        self.exited = False
        self.turn = None
        self.by_user = False
        self.prompt = 0
        self.at = 0
        self.outcome = OUTCOME_OPEN
        self.detail = String()
        self.text = None
        self.item = String()
        self.parent = None
        self.linked = None
        self.item_kind = ITEM_NOTICE
        self.body = ItemBody()
        self.status = STATUS_RUNNING
        self.message = None
        self.items = List[String]()
        self.request = 0
        self.ask = Ask()
        self.blocking = False
        self.close = RequestClose()
        self.prompts = List[QueuedPrompt]()
        self.used = 0
        self.size = 0


def _variant(mut json: Json, names: String, value: String) -> UInt8:
    var known = text_split(names, ",")
    for index in range(len(known)):
        if known[index] == value:
            return UInt8(index)
    json.failed = True
    return 0


def _tagged(mut json: Json, node: Int, tag: String, names: String) -> UInt8:
    return _variant(json, names, json.req_text(node, tag))


def _named(mut json: Json, node: Int, key: String, names: String) -> UInt8:
    return _variant(json, names, json.req_text(node, key))


def _body(mut json: Json, node: Int) -> ItemBody:
    var body = ItemBody()
    body.title = json.def_text(node, "title")
    body.target = json.opt_text(node, "target")
    body.text = json.def_text(node, "text")
    for step in json.def_list(node, "steps"):
        body.steps.append(
            PlanStep(
                json.req_text(step, "text"),
                _named(json, step, "status", STEP_STATUSES),
            )
        )
    body.exit_code = json.opt_integer(node, "exitCode", -2147483648, 2147483647)
    for image in json.def_list(node, "images"):
        var data = base64_bytes(json.req_text(image, "data"))
        if not data:
            json.failed = True
            break
        body.images.append(ChatImage(json.req_text(image, "format"), data.take()))
    body.activity = json.opt_text(node, "activity")
    return body^


def _options(mut json: Json, node: Int) -> List[AskOption]:
    var options = List[AskOption]()
    for option in json.req_list(node, "options"):
        options.append(
            AskOption(
                json.req_text(option, "id"),
                json.req_text(option, "label"),
                _named(json, option, "kind", OPTION_KINDS),
            )
        )
    return options^


def _ask(mut json: Json, node: Int) -> Ask:
    var ask = Ask()
    ask.kind = _tagged(json, node, "ask", ASK_KINDS)
    if json.failed:
        return ask^
    if ask.kind == ASK_APPROVAL:
        ask.title = json.req_text(node, "title")
        ask.detail = json.opt_text(node, "detail")
        ask.options = _options(json, node)
    elif ask.kind == ASK_PLAN:
        ask.markdown = json.req_text(node, "markdown")
        ask.options = _options(json, node)
    else:
        for question in json.req_list(node, "questions"):
            var choices = List[ChatChoice]()
            for choice in json.def_list(question, "options"):
                choices.append(
                    ChatChoice(
                        json.req_text(choice, "label"),
                        json.opt_text(choice, "description"),
                    )
                )
            ask.questions.append(
                ChatQuestion(
                    json.req_text(question, "id"),
                    json.opt_text(question, "header"),
                    json.req_text(question, "question"),
                    choices^,
                    json.def_bool(question, "multiSelect"),
                    json.def_bool(question, "allowOther"),
                    json.def_bool(question, "secret"),
                )
            )
    return ask^


def _close(mut json: Json, node: Int) -> RequestClose:
    var close = RequestClose()
    close.kind = _tagged(json, node, "close", CLOSE_KINDS)
    if json.failed or close.kind != CLOSE_ANSWERED:
        return close^
    var answer = json.req_object(node, "answer")
    close.answer = _tagged(json, answer, "answer", ANSWER_KINDS)
    if json.failed:
        return close^
    if close.answer == ANSWER_OPTION:
        close.id = json.req_text(answer, "id")
    elif close.answer == ANSWER_QUESTIONS:
        for entry in json.req_list(answer, "answers"):
            var answers = List[String]()
            for text in json.req_list(entry, "answers"):
                answers.append(json.string(text))
            close.answers.append(
                QuestionAnswer(json.req_text(entry, "question"), answers^)
            )
    return close^


def chat_event_decode(mut json: Json, node: Int) -> Optional[ChatEvent]:
    json.failed = False
    if not json.ok() or node < 0 or json.kind(node) != JSON_OBJECT:
        return None
    var event = ChatEvent(_tagged(json, node, "event", EVENT_KINDS))
    if json.failed:
        return None
    var kind = event.kind
    if kind == EVENT_SESSION_READY:
        event.session = json.req_text(node, "session")
    elif kind == EVENT_SESSION_ENDED:
        var end = json.req_object(node, "end")
        event.exited = _tagged(json, end, "end", "closed,exited") == 1
        if event.exited:
            event.detail = json.req_text(end, "message")
    elif kind == EVENT_TURN_STARTED:
        event.turn = Optional[UInt64](json.req_unsigned(node, "turn"))
        var by = json.req_object(node, "by")
        event.by_user = _tagged(json, by, "by", "user,agent") == 0
        if event.by_user and not json.failed:
            event.prompt = json.req_unsigned(by, "prompt")
        event.at = json.req_unsigned(node, "at")
    elif kind == EVENT_TURN_CLAIMED:
        event.turn = Optional[UInt64](json.req_unsigned(node, "turn"))
        event.prompt = json.req_unsigned(node, "prompt")
    elif kind == EVENT_TURN_STOPPING:
        event.turn = Optional[UInt64](json.req_unsigned(node, "turn"))
    elif kind == EVENT_TURN_ENDED:
        event.turn = Optional[UInt64](json.req_unsigned(node, "turn"))
        var outcome = json.req_object(node, "outcome")
        event.outcome = _tagged(json, outcome, "outcome", OUTCOME_KINDS)
        if not json.failed and event.outcome == OUTCOME_COMPLETED:
            event.detail = json.req_text(outcome, "stop_reason")
        elif not json.failed and event.outcome == OUTCOME_FAILED:
            event.detail = json.req_text(outcome, "message")
        event.at = json.req_unsigned(node, "at")
    elif kind == EVENT_TURN_ACTIVITY:
        event.turn = Optional[UInt64](json.req_unsigned(node, "turn"))
        event.text = json.opt_text(node, "text")
        event.at = json.req_unsigned(node, "at")
    elif kind == EVENT_ITEM_STARTED:
        event.item = json.req_text(node, "item")
        event.turn = json.opt_unsigned(node, "turn")
        event.parent = json.opt_text(node, "parent")
        event.item_kind = _named(json, node, "kind", ITEM_KINDS)
        event.body = _body(json, json.req_object(node, "body"))
        event.at = json.req_unsigned(node, "at")
    elif kind == EVENT_ITEM_APPENDED:
        event.item = json.req_text(node, "item")
        event.text = Optional[String](json.req_text(node, "text"))
    elif kind == EVENT_ITEM_CHANGED:
        event.item = json.req_text(node, "item")
        event.body = _body(json, json.req_object(node, "body"))
    elif kind == EVENT_ITEM_BACKGROUNDED:
        event.item = json.req_text(node, "item")
    elif kind == EVENT_ITEM_ENDED:
        event.item = json.req_text(node, "item")
        var status = json.req_object(node, "status")
        event.status = _tagged(json, status, "status", ITEM_STATUSES)
        if not json.failed and event.status == STATUS_FAILED:
            event.message = json.opt_text(status, "message")
        event.at = json.req_unsigned(node, "at")
    elif kind == EVENT_ITEMS_SUMMARIZED:
        for item in json.req_list(node, "items"):
            event.items.append(json.string(item))
        event.text = Optional[String](json.req_text(node, "summary"))
    elif kind == EVENT_REQUEST_OPENED:
        event.request = json.req_unsigned(node, "request")
        event.turn = json.opt_unsigned(node, "turn")
        event.linked = json.opt_text(node, "item")
        event.ask = _ask(json, json.req_object(node, "ask"))
        event.blocking = json.req_bool(node, "blocking")
        event.at = json.req_unsigned(node, "at")
    elif kind == EVENT_REQUEST_CLOSED:
        event.request = json.req_unsigned(node, "request")
        event.close = _close(json, json.req_object(node, "close"))
        event.at = json.req_unsigned(node, "at")
    elif kind == EVENT_QUEUE_CHANGED:
        for prompt in json.req_list(node, "prompts"):
            event.prompts.append(
                QueuedPrompt(
                    json.req_unsigned(prompt, "id"), json.req_text(prompt, "text")
                )
            )
    elif kind == EVENT_USAGE_CHANGED:
        var usage = json.req_object(node, "usage")
        event.used = json.req_unsigned(usage, "used")
        event.size = json.req_unsigned(usage, "size")
    if json.failed:
        return None
    return Optional[ChatEvent](event^)


@fieldwise_init
struct TurnView(Copyable, Movable):
    var id: UInt64
    var by_user: Bool
    var prompt: UInt64
    var started_at: UInt64
    var ended_at: Optional[UInt64]
    var outcome: UInt8
    var detail: String
    var stopping: Bool
    var items: List[String]
    var activity: Optional[String]
    var activity_at: UInt64

    def is_open(self) -> Bool:
        return self.outcome == OUTCOME_OPEN


@fieldwise_init
struct ItemView(Copyable, Movable):
    var id: String
    var kind: UInt8
    var body: ItemBody
    var status: UInt8
    var message: Optional[String]
    var turn: Optional[UInt64]
    var parent: Optional[String]
    var children: List[String]
    var background: Bool
    var started_at: UInt64
    var ended_at: Optional[UInt64]
    var batch: UInt32
    var summary: String

    def is_open(self) -> Bool:
        return self.status == STATUS_RUNNING


@fieldwise_init
struct RequestView(Copyable, Movable):
    var id: UInt64
    var turn: Optional[UInt64]
    var item: Optional[String]
    var ask: Ask
    var blocking: Bool
    var opened_at: UInt64
    var closed: Bool
    var close: RequestClose
    var closed_at: UInt64

    def is_open(self) -> Bool:
        return not self.closed


def _same(left: Optional[UInt64], right: Optional[UInt64]) -> Bool:
    if left and right:
        return left.value() == right.value()
    return not left and not right


struct ChatView(Movable):
    var session_kind: UInt8
    var session: String
    var turns: List[TurnView]
    var items: List[ItemView]
    var index: Dict[String, Int]
    var background: List[String]
    var requests: List[RequestView]
    var queue: List[QueuedPrompt]
    var has_usage: Bool
    var used: UInt64
    var size: UInt64
    var skipped: UInt32
    var refused: UInt32
    var last_session: Optional[String]
    var pictured: List[String]
    var batches: UInt32
    var bytes: Int

    def __init__(out self):
        self.session_kind = SESSION_STARTING
        self.session = String()
        self.turns = List[TurnView]()
        self.items = List[ItemView]()
        self.index = Dict[String, Int]()
        self.background = List[String]()
        self.requests = List[RequestView]()
        self.queue = List[QueuedPrompt]()
        self.has_usage = False
        self.used = 0
        self.size = 0
        self.skipped = 0
        self.refused = 0
        self.last_session = None
        self.pictured = List[String]()
        self.batches = 0
        self.bytes = 0

    def running(self) -> Int:
        var last = len(self.turns) - 1
        if last >= 0 and self.turns[last].is_open():
            return last
        return -1

    def item(self, id: String) -> Int:
        return self.index.get(id, -1)

    def request(self, id: UInt64) -> Int:
        for index in range(len(self.requests)):
            if self.requests[index].id == id:
                return index
        return -1

    def waiting(self) -> Bool:
        for request in self.requests:
            if request.is_open() and request.blocking:
                return True
        return False

    def asking(self) -> Int:
        for index in range(len(self.requests)):
            if (
                self.requests[index].is_open()
                and not self.requests[index].blocking
                and self.requests[index].ask.kind == ASK_QUESTIONS
            ):
                return index
        return -1

    def phase(self) -> UInt8:
        if self.session_kind == SESSION_STARTING:
            return PHASE_STARTING
        if self.session_kind != SESSION_READY:
            return PHASE_ENDED
        if self.waiting():
            return PHASE_WAITING
        var running = self.running()
        if running < 0:
            return PHASE_IDLE
        return PHASE_STOPPING if self.turns[running].stopping else PHASE_RUNNING

    def clear(mut self):
        self.turns.clear()
        self.items.clear()
        self.index = Dict[String, Int]()
        self.background.clear()
        self.requests.clear()
        self.pictured.clear()
        self.bytes = 0

    def apply(mut self, event: ChatEvent) -> Int:
        var result = self.try_apply(event)
        if result == CHAT_REFUSED_FULL:
            if self.refused < UInt32.MAX:
                self.refused += 1
        elif result != CHAT_APPLIED and self.skipped < UInt32.MAX:
            self.skipped += 1
        return result

    def running_turn(self, turn: Optional[UInt64]) -> Int:
        var running = self.running()
        if running >= 0 and turn and self.turns[running].id == turn.value():
            return running
        return -1

    def open_item(self, id: String, mut found: Int) -> Int:
        found = self.item(id)
        if found < 0:
            return SKIP_UNKNOWN_ITEM
        if not self.items[found].is_open():
            return SKIP_ITEM_CLOSED
        return CHAT_APPLIED

    def unpicture(mut self, id: String):
        var kept = List[String]()
        for known in self.pictured:
            if known != id:
                kept.append(known)
        self.pictured = kept^

    def picture(mut self, id: String):
        self.unpicture(id)
        self.pictured.append(id)
        while len(self.pictured) > CHAT_KEPT_IMAGES:
            var oldest = self.pictured.pop(0)
            var found = self.item(oldest)
            if found >= 0:
                for image in self.items[found].body.images:
                    self.bytes -= len(image.data)
                self.items[found].body.images = List[ChatImage]()

    def try_apply(mut self, event: ChatEvent) -> Int:
        var kind = event.kind
        if kind == EVENT_SESSION_READY:
            if self.running() >= 0:
                return SKIP_TURN_STILL_OPEN
            if event.session.byte_length() > 0:
                if self.last_session and self.last_session.value() != event.session:
                    self.clear()
                self.last_session = Optional[String](event.session)
            self.session_kind = SESSION_READY
            self.session = event.session
        elif kind == EVENT_SESSION_ENDED:
            if self.running() >= 0:
                return SKIP_TURN_STILL_OPEN
            for item in self.items:
                if item.is_open():
                    return SKIP_STILL_RUNNING
            for request in self.requests:
                if request.is_open():
                    return SKIP_REQUESTS_OPEN
            self.session_kind = SESSION_EXITED if event.exited else SESSION_CLOSED
            self.session = event.detail
        elif kind == EVENT_TURN_STARTED:
            if self.running() >= 0:
                return SKIP_TURN_STILL_OPEN
            var id = event.turn.value()
            if len(self.turns) > 0 and self.turns[len(self.turns) - 1].id >= id:
                return SKIP_UNKNOWN_TURN
            if len(self.turns) >= CHAT_MAX_TURNS:
                return CHAT_REFUSED_FULL
            self.turns.append(
                TurnView(
                    id,
                    event.by_user,
                    event.prompt,
                    event.at,
                    None,
                    OUTCOME_OPEN,
                    String(),
                    False,
                    List[String](),
                    None,
                    0,
                )
            )
        elif kind == EVENT_TURN_CLAIMED:
            var running = self.running_turn(event.turn)
            if running < 0:
                return SKIP_NOT_RUNNING
            self.turns[running].by_user = True
            self.turns[running].prompt = event.prompt
        elif kind == EVENT_TURN_STOPPING:
            var running = self.running_turn(event.turn)
            if running < 0:
                return SKIP_NOT_RUNNING
            self.turns[running].stopping = True
        elif kind == EVENT_TURN_ENDED:
            for item in self.items:
                if _same(item.turn, event.turn) and not item.background and item.is_open():
                    return SKIP_STILL_RUNNING
            for request in self.requests:
                if _same(request.turn, event.turn) and request.is_open():
                    return SKIP_REQUESTS_OPEN
            var running = self.running_turn(event.turn)
            if running < 0:
                return SKIP_NOT_RUNNING
            self.turns[running].outcome = event.outcome
            self.turns[running].detail = event.detail
            self.turns[running].ended_at = Optional[UInt64](event.at)
        elif kind == EVENT_TURN_ACTIVITY:
            var running = self.running_turn(event.turn)
            if running < 0:
                return SKIP_NOT_RUNNING
            self.turns[running].activity = event.text
            self.turns[running].activity_at = event.at if event.text else 0
        elif kind == EVENT_ITEM_STARTED:
            return self.start(event)
        elif kind == EVENT_ITEM_APPENDED:
            var found = -1
            var refusal = self.open_item(event.item, found)
            if refusal != CHAT_APPLIED:
                return refusal
            var added = event.text.value().byte_length()
            if self.bytes + added > CHAT_MAX_BYTES:
                return CHAT_REFUSED_FULL
            self.bytes += added
            self.items[found].body.text += event.text.value()
        elif kind == EVENT_ITEM_CHANGED:
            var found = -1
            var refusal = self.open_item(event.item, found)
            if refusal != CHAT_APPLIED:
                return refusal
            var grown = event.body.bytes() - self.items[found].body.bytes()
            if self.bytes + grown > CHAT_MAX_BYTES:
                return CHAT_REFUSED_FULL
            self.bytes += grown
            self.items[found].body = event.body.copy()
            if len(event.body.images) == 0:
                self.unpicture(event.item)
            else:
                self.picture(event.item)
        elif kind == EVENT_ITEM_BACKGROUNDED:
            var found = -1
            var refusal = self.open_item(event.item, found)
            if refusal != CHAT_APPLIED:
                return refusal
            var stack = List[Int]()
            stack.append(found)
            while len(stack) > 0:
                var at = stack.pop()
                self.items[at].background = True
                for child in self.items[at].children:
                    var below = self.item(child)
                    if below >= 0:
                        stack.append(below)
            for index in range(len(self.requests)):
                if self.requests[index].is_open() and self.requests[index].item:
                    var owner = self.item(self.requests[index].item.value())
                    if owner >= 0 and self.items[owner].background:
                        self.requests[index].turn = None
        elif kind == EVENT_ITEM_ENDED:
            if event.status == STATUS_RUNNING:
                return SKIP_STILL_RUNNING
            var found = self.item(event.item)
            if found < 0:
                return SKIP_UNKNOWN_ITEM
            if not self.items[found].is_open():
                return SKIP_ITEM_CLOSED
            for child in self.items[found].children:
                var below = self.item(child)
                if below < 0 or not self.items[below].is_open():
                    continue
                if not (self.items[below].background and not self.items[found].background):
                    return SKIP_CHILDREN_OPEN
            self.items[found].status = event.status
            self.items[found].message = event.message
            self.items[found].ended_at = Optional[UInt64](event.at)
        elif kind == EVENT_ITEMS_SUMMARIZED:
            var known = False
            for item in event.items:
                if self.item(item) >= 0:
                    known = True
            if not known:
                return SKIP_UNKNOWN_ITEM
            self.batches += 1
            for item in event.items:
                var found = self.item(item)
                if found >= 0:
                    self.items[found].batch = self.batches
                    self.items[found].summary = event.text.value()
        elif kind == EVENT_REQUEST_OPENED:
            if self.request(event.request) >= 0:
                return SKIP_DUPLICATE_REQUEST
            if event.turn and self.running_turn(event.turn) < 0:
                return SKIP_NOT_RUNNING
            if len(self.requests) >= CHAT_MAX_REQUESTS:
                return CHAT_REFUSED_FULL
            self.requests.append(
                RequestView(
                    event.request,
                    event.turn,
                    event.linked,
                    event.ask.copy(),
                    event.blocking,
                    event.at,
                    False,
                    RequestClose(),
                    0,
                )
            )
        elif kind == EVENT_REQUEST_CLOSED:
            var found = self.request(event.request)
            if found < 0:
                return SKIP_UNKNOWN_REQUEST
            if not self.requests[found].is_open():
                return SKIP_REQUEST_CLOSED
            self.requests[found].closed = True
            self.requests[found].close = event.close.copy()
            self.requests[found].closed_at = event.at
        elif kind == EVENT_QUEUE_CHANGED:
            self.queue = event.prompts.copy()
        elif kind == EVENT_USAGE_CHANGED:
            self.has_usage = True
            self.used = event.used
            self.size = event.size
        else:
            self = ChatView()
        return CHAT_APPLIED

    def start(mut self, event: ChatEvent) -> Int:
        if self.item(event.item) >= 0:
            return SKIP_DUPLICATE_ITEM
        if event.turn and self.running_turn(event.turn) < 0:
            return SKIP_NOT_RUNNING
        var background = False
        var above = -1
        if event.parent:
            above = self.item(event.parent.value())
            if above < 0:
                return SKIP_UNKNOWN_ITEM
            if not self.items[above].is_open():
                return SKIP_PARENT_CLOSED
            background = self.items[above].background
        var added = event.body.bytes()
        if len(self.items) >= CHAT_MAX_ITEMS or self.bytes + added > CHAT_MAX_BYTES:
            return CHAT_REFUSED_FULL
        if above >= 0:
            self.items[above].children.append(event.item)
        elif event.turn:
            self.turns[self.running()].items.append(event.item)
        else:
            background = True
            self.background.append(event.item)
        self.bytes += added
        self.index[event.item] = len(self.items)
        self.items.append(
            ItemView(
                event.item,
                event.item_kind,
                event.body.copy(),
                STATUS_RUNNING,
                None,
                event.turn,
                event.parent,
                List[String](),
                background,
                event.at,
                None,
                0,
                String(),
            )
        )
        for index in range(len(self.requests)):
            if (
                self.requests[index].is_open()
                and self.requests[index].item
                and self.requests[index].item.value() == event.item
            ):
                self.requests[index].turn = event.turn
        if len(event.body.images) > 0:
            self.picture(event.item)
        return CHAT_APPLIED
