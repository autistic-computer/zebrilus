from .agent_chat import ChatImage, ChatQuestion, QuestionAnswer
from .agent_completion import (
    CommandCompletion,
    composer_completions,
    pane_commands,
    rendered_error,
)
from .agent_events import AgentFeed, AgentPaneState, FeedResult
from .agent_lens import (
    CONNECTION_FAILED,
    CONNECTION_READY,
    CONNECTION_RUNNING,
    PendingPermission,
    accepts_prompt,
    turn_running,
)
from .agent_question import (
    QUESTION_DISMISS,
    QUESTION_HANDLED,
    QUESTION_OTHER,
    QUESTION_SUBMIT,
    QuestionCard,
    QuestionStep,
)
from .agent_settings import SETTER_LIST, SETTER_SET, composer_setter
from .agent_text import text_blank, text_ceil, text_floor, text_slice
from .agent_timeline import COMPOSER_STOP, composer_action

comptime EFFECT_PROMPT: UInt8 = 0
comptime EFFECT_CANCEL: UInt8 = 1
comptime EFFECT_UNQUEUE: UInt8 = 2
comptime EFFECT_PERMISSION: UInt8 = 3
comptime EFFECT_ANSWER: UInt8 = 4
comptime EFFECT_SET_OPTION: UInt8 = 5
comptime EFFECT_REPLAY: UInt8 = 6
comptime EFFECT_ACKNOWLEDGE: UInt8 = 7

comptime PANE_OK: UInt8 = 0
comptime PANE_REFUSED_OFFLINE: UInt8 = 1
comptime PANE_REFUSED_READ_ONLY: UInt8 = 2
comptime PANE_REFUSED_PHASE: UInt8 = 3
comptime PANE_REFUSED_EMPTY: UInt8 = 4
comptime PANE_REFUSED_IMAGES: UInt8 = 5
comptime PANE_REFUSED_QUEUE: UInt8 = 6
comptime PANE_REFUSED_OUTBOX: UInt8 = 7
comptime PANE_REFUSED_TEXT: UInt8 = 8
comptime PANE_REFUSED_REQUEST: UInt8 = 9
comptime PANE_REFUSED_OPTION: UInt8 = 10

comptime PANE_MAX_OUTBOX = 64
comptime PANE_MAX_PROMPT_BYTES = 6 * 1024 * 1024
comptime PANE_MAX_IMAGES = 64
comptime PANE_MAX_QUEUED = 4
comptime PANE_MAX_OPTION_BYTES = 4096

comptime IMAGE_COUNT_ERROR = "Attach at most 64 images per message."
comptime IMAGE_FORMAT_ERROR = "Choose a PNG, JPEG, or WebP image."
comptime IMAGE_EMPTY_ERROR = "The selected image is empty."
comptime IMAGE_TOTAL_ERROR = "The message and its images must total 6 MiB or less."
comptime QUEUE_FULL_ERROR = "finish or unqueue one of the four queued prompts first"
comptime RESTORE_ERROR = "A restored prompt did not fit the composer."


@fieldwise_init
struct AgentEffect(Copyable, Movable):
    var kind: UInt8
    var text: String
    var images: List[ChatImage]
    var request: UInt64
    var option: Optional[String]
    var answers: List[QuestionAnswer]


def _effect(kind: UInt8, request: UInt64 = 0) -> AgentEffect:
    return AgentEffect(
        kind, String(), List[ChatImage](), request, None, List[QuestionAnswer]()
    )


def validate_images(text: String, images: List[ChatImage]) -> Optional[String]:
    if len(images) > PANE_MAX_IMAGES:
        return Optional[String](String(IMAGE_COUNT_ERROR))
    var total = text.byte_length()
    for image in images:
        if (
            image.format != "image/png"
            and image.format != "image/jpeg"
            and image.format != "image/webp"
        ):
            return Optional[String](String(IMAGE_FORMAT_ERROR))
        if len(image.data) == 0:
            return Optional[String](String(IMAGE_EMPTY_ERROR))
        total += len(image.data)
    if total > PANE_MAX_PROMPT_BYTES:
        return Optional[String](String(IMAGE_TOTAL_ERROR))
    return None


def _digit(name: String) -> Int:
    if name.byte_length() != 1:
        return -1
    var byte = Int(name.as_bytes()[0])
    if byte < 49 or byte > 57:
        return -1
    return byte - 48


struct AgentModel(Movable):
    var pane: UInt64
    var provider: Optional[UInt8]
    var feed: AgentFeed
    var read_only: Bool
    var draft: String
    var cursor: Int
    var attachments: List[ChatImage]
    var draft_error: Optional[String]
    var completions: List[CommandCompletion]
    var completion_selected: Int
    var completion_dismissed: Bool
    var synced: Bool
    var last_value: String
    var last_cursor: Int
    var permission_selected: Int
    var permission_head: UInt64
    var answered: List[UInt64]
    var has_card: Bool
    var card: QuestionCard
    var other_target: Int
    var follow: Bool
    var outbox: List[AgentEffect]
    var refused_effects: Int
    var refused_text: Int
    var refused_restores: Int

    def __init__(out self, pane: UInt64, client: Optional[UInt64] = None):
        self.pane = pane
        self.provider = None
        self.feed = AgentFeed(client)
        self.read_only = False
        self.draft = String()
        self.cursor = 0
        self.attachments = List[ChatImage]()
        self.draft_error = None
        self.completions = List[CommandCompletion]()
        self.completion_selected = -1
        self.completion_dismissed = False
        self.synced = False
        self.last_value = String()
        self.last_cursor = 0
        self.permission_selected = 0
        self.permission_head = 0
        self.answered = List[UInt64]()
        self.has_card = False
        self.card = QuestionCard(0, List[ChatQuestion]())
        self.other_target = -1
        self.follow = True
        self.outbox = List[AgentEffect]()
        self.refused_effects = 0
        self.refused_text = 0
        self.refused_restores = 0

    def take_outbox(mut self) -> List[AgentEffect]:
        var out = self.outbox.copy()
        self.outbox = List[AgentEffect]()
        return out^

    def push(mut self, var effect: AgentEffect) -> Bool:
        if len(self.outbox) >= PANE_MAX_OUTBOX:
            self.refused_effects += 1
            return False
        self.outbox.append(effect^)
        return True

    def writable(self) -> Bool:
        return self.feed.connected and not self.read_only

    def running(self) -> Bool:
        return turn_running(self.feed.state.phase)

    def has_content(self) -> Bool:
        return not text_blank(self.draft) or len(self.attachments) > 0

    def action(self) -> UInt8:
        return composer_action(self.running(), self.has_content())

    def action_enabled(self) -> Bool:
        if self.running():
            return self.writable()
        var phase = self.feed.state.phase
        var ready = self.writable() and (
            phase == CONNECTION_READY or phase == CONNECTION_RUNNING
        )
        return ready and not self.feed.state.parked and self.has_content()

    def error_line(self) -> Optional[String]:
        if self.feed.state.error:
            return Optional[String](rendered_error(self.feed.state.error.value()))
        if self.feed.connected and self.feed.state.phase == CONNECTION_FAILED:
            return Optional[String](rendered_error(self.feed.state.failure))
        if self.draft_error:
            return Optional[String](self.draft_error.value().copy())
        return None

    def waiting(self) -> List[PendingPermission]:
        var found = List[PendingPermission]()
        for request in self.feed.permissions():
            var done = False
            for id in self.answered:
                if id == request.request_id:
                    done = True
            if not done:
                found.append(request.copy())
        return found^

    def current(self) -> Optional[PendingPermission]:
        var waiting = self.waiting()
        if len(waiting) == 0:
            return None
        return Optional[PendingPermission](waiting[0].copy())

    def refresh(mut self):
        var pending = self.feed.permissions()
        var kept = List[UInt64]()
        for id in self.answered:
            for request in pending:
                if request.request_id == id:
                    kept.append(id)
                    break
        self.answered = kept^
        var head = self.current()
        var id: UInt64 = 0
        if head:
            id = head.value().request_id
        if id != self.permission_head:
            self.permission_head = id
            self.permission_selected = 0
        if head and len(head.value().questions) > 0:
            if not self.has_card or self.card.request_id != id:
                self.card = QuestionCard(id, head.value().questions.copy())
                self.has_card = True
                self.other_target = -1
        elif self.has_card:
            self.card = QuestionCard(0, List[ChatQuestion]())
            self.has_card = False
            self.other_target = -1

    def set_state(mut self, state: AgentPaneState) -> UInt8:
        var result = self.feed.set_state(state)
        self.refresh()
        return result

    def disconnect(mut self):
        self.feed.disconnect()
        self.refresh()

    def lagged(mut self) -> Bool:
        return self.push(_effect(EFFECT_REPLAY, self.feed.lagged()))

    def updates(mut self, first_seq: UInt64, items: List[List[UInt8]]) -> FeedResult:
        var result = self.feed.updates(first_seq, items)
        if result.replay:
            _ = self.push(_effect(EFFECT_REPLAY, result.replay.value()))
        if result.switched:
            self.answered.clear()
        self.restore()
        self.refresh()
        return result^

    def restore(mut self):
        for prompt in self.feed.take_restored():
            var text = self.draft.copy()
            if prompt.text.byte_length() > 0:
                if text.byte_length() > 0:
                    text += "\n"
                text += prompt.text
            var images = self.attachments.copy()
            for image in prompt.images:
                images.append(image.copy())
            if validate_images(text, images):
                self.refused_restores += 1
                self.draft_error = Optional[String](String(RESTORE_ERROR))
                continue
            self.draft = text^
            self.attachments = images^
            self.cursor = self.draft.byte_length()
        for id in self.feed.take_acknowledged():
            _ = self.push(_effect(EFFECT_ACKNOWLEDGE, id))

    def sync(mut self):
        if (
            not self.synced
            or self.draft != self.last_value
            or self.cursor != self.last_cursor
        ):
            self.completion_dismissed = False
            self.synced = True
            self.last_value = self.draft.copy()
            self.last_cursor = self.cursor
        if self.completion_dismissed:
            self.completions = List[CommandCompletion]()
        else:
            self.completions = composer_completions(
                self.draft,
                self.cursor,
                pane_commands(
                    self.feed.cursor.commands, self.feed.cursor.verbs_supported
                ),
                self.feed.settings,
            )
        if len(self.completions) == 0:
            self.completion_selected = -1
        else:
            self.completion_selected = min(
                max(self.completion_selected, 0), len(self.completions) - 1
            )

    def clear_completions(mut self):
        self.completions = List[CommandCompletion]()
        self.completion_selected = -1

    def navigate(mut self, down: Bool):
        var count = len(self.completions)
        if count == 0:
            return
        var at = max(self.completion_selected, 0)
        if down:
            self.completion_selected = (at + 1) % count
        else:
            self.completion_selected = count - 1 if at == 0 else at - 1

    def accept_completion(mut self, index: Int) -> UInt8:
        if index < 0 or index >= len(self.completions):
            return PANE_REFUSED_OPTION
        var insertion = self.completions[index].insertion()
        var submits = self.completions[index].submits()
        var start = self.completions[index].start
        var end = self.completions[index].end
        self.completion_dismissed = True
        self.draft = String(
            text_slice(self.draft, 0, start),
            insertion,
            text_slice(self.draft, end, self.draft.byte_length()),
        )
        self.cursor = start + insertion.byte_length()
        self.synced = True
        self.last_value = self.draft.copy()
        self.last_cursor = self.cursor
        self.clear_completions()
        if submits:
            return self.submit()
        return PANE_OK

    def send_prompt(mut self, text: String, images: List[ChatImage]) -> UInt8:
        if not self.feed.connected:
            return PANE_REFUSED_OFFLINE
        if self.read_only:
            return PANE_REFUSED_READ_ONLY
        if not accepts_prompt(self.feed.state.phase, self.feed.state.parked):
            return PANE_REFUSED_PHASE
        if text_blank(text) and len(images) == 0:
            return PANE_REFUSED_EMPTY
        var problem = validate_images(text, images)
        if problem:
            self.draft_error = problem.take()
            return PANE_REFUSED_IMAGES
        if self.running() and self.feed.state.queued_prompts >= PANE_MAX_QUEUED:
            self.draft_error = Optional[String](String(QUEUE_FULL_ERROR))
            return PANE_REFUSED_QUEUE
        var effect = _effect(EFFECT_PROMPT)
        effect.text = text.copy()
        effect.images = images.copy()
        if not self.push(effect^):
            return PANE_REFUSED_OUTBOX
        return PANE_OK

    def submit(mut self) -> UInt8:
        if len(self.attachments) == 0:
            var found = composer_setter(self.draft, self.feed.settings)
            if found:
                var outcome = PANE_OK
                if found.value().kind == SETTER_LIST:
                    self.draft = String("/", found.value().option, " ")
                else:
                    if found.value().kind == SETTER_SET:
                        outcome = self.set_option(
                            found.value().option, found.value().value
                        )
                    self.draft_error = None
                    self.draft = String()
                self.cursor = self.draft.byte_length()
                self.clear_completions()
                self.completion_dismissed = False
                self.synced = False
                return outcome
        var text = self.draft.copy()
        var images = self.attachments.copy()
        var result = self.send_prompt(text, images)
        if result == PANE_OK:
            self.attachments = List[ChatImage]()
            self.draft_error = None
            self.draft = String()
            self.cursor = 0
            self.follow = True
        return result

    def set_option(mut self, option: String, value: String) -> UInt8:
        if not self.feed.connected:
            return PANE_REFUSED_OFFLINE
        if self.read_only:
            return PANE_REFUSED_READ_ONLY
        if (
            option.byte_length() == 0
            or option.byte_length() > PANE_MAX_OPTION_BYTES
            or value.byte_length() > PANE_MAX_OPTION_BYTES
        ):
            return PANE_REFUSED_OPTION
        var effect = _effect(EFFECT_SET_OPTION)
        effect.option = Optional[String](option.copy())
        effect.text = value.copy()
        if not self.push(effect^):
            return PANE_REFUSED_OUTBOX
        return PANE_OK

    def cancel(mut self) -> UInt8:
        if not self.feed.connected:
            return PANE_REFUSED_OFFLINE
        if self.read_only:
            return PANE_REFUSED_READ_ONLY
        if not self.running():
            return PANE_REFUSED_PHASE
        if not self.push(_effect(EFFECT_CANCEL)):
            return PANE_REFUSED_OUTBOX
        return PANE_OK

    def unqueue(mut self) -> UInt8:
        if not self.feed.connected:
            return PANE_REFUSED_OFFLINE
        if self.read_only:
            return PANE_REFUSED_READ_ONLY
        if self.feed.state.queued_prompts <= 0:
            return PANE_REFUSED_EMPTY
        if not self.push(_effect(EFFECT_UNQUEUE)):
            return PANE_REFUSED_OUTBOX
        return PANE_OK

    def press_action(mut self) -> UInt8:
        if self.action() == COMPOSER_STOP:
            return self.cancel()
        return self.submit()

    def can_answer(self, request: UInt64) -> Bool:
        if not self.writable():
            return False
        for id in self.answered:
            if id == request:
                return False
        for pending in self.feed.permissions():
            if pending.request_id == request:
                return True
        return False

    def respond(mut self, request: UInt64, option: Optional[String]) -> UInt8:
        if not self.can_answer(request):
            return PANE_REFUSED_REQUEST
        var effect = _effect(EFFECT_PERMISSION, request)
        effect.option = option.copy()
        if not self.push(effect^):
            return PANE_REFUSED_OUTBOX
        self.answered.append(request)
        self.refresh()
        return PANE_OK

    def choose(mut self, index: Int) -> UInt8:
        var head = self.current()
        if not head or index < 0 or index >= len(head.value().options):
            return PANE_REFUSED_OPTION
        return self.respond(
            head.value().request_id,
            Optional[String](head.value().options[index].id.copy()),
        )

    def answer(mut self, request: UInt64, answers: List[QuestionAnswer]) -> UInt8:
        if not self.can_answer(request):
            return PANE_REFUSED_REQUEST
        var effect = _effect(EFFECT_ANSWER, request)
        effect.answers = answers.copy()
        if not self.push(effect^):
            return PANE_REFUSED_OUTBOX
        self.answered.append(request)
        self.refresh()
        return PANE_OK

    def apply(mut self, step: QuestionStep) -> Bool:
        if step.kind == QUESTION_HANDLED:
            return True
        if step.kind == QUESTION_OTHER:
            self.other_target = step.question
            return True
        if step.kind == QUESTION_SUBMIT:
            var asked = self.card.request_id
            _ = self.answer(asked, step.answers)
            return True
        if step.kind == QUESTION_DISMISS:
            var dismissed = self.card.request_id
            _ = self.respond(dismissed, None)
            return True
        return False

    def pick(mut self, question: Int, option: Int) -> Bool:
        if not self.has_card or question < 0 or question >= len(self.card.questions):
            return False
        if option == len(self.card.questions[question].options):
            return self.apply(self.card.select_other(question))
        return self.apply(self.card.pick(question, option))

    def ask_key(mut self, name: String, shift: Bool) -> Bool:
        if self.has_card:
            if self.other_target >= 0:
                if name == "enter" or _digit(name) > 0:
                    return False
                if name == "escape":
                    self.other_target = -1
                    return True
                if name == "backspace":
                    var value = self.card.other_value(self.other_target)
                    var end = text_floor(value, value.byte_length() - 1)
                    _ = self.card.set_other_text(
                        self.other_target, text_slice(value, 0, end)
                    )
                    return True
                self.other_target = -1
            if not text_blank(self.draft):
                return False
            return self.apply(self.card.key(name, shift))
        if not text_blank(self.draft) or name == "enter":
            return False
        var head = self.current()
        if not head:
            return False
        var count = len(head.value().options)
        if name == "escape":
            _ = self.respond(head.value().request_id, None)
            return True
        if count == 0:
            return False
        if name == "up":
            var at = min(self.permission_selected, count - 1)
            self.permission_selected = count - 1 if at == 0 else at - 1
            return True
        if name == "down":
            self.permission_selected = (self.permission_selected + 1) % count
            return True
        var digit = _digit(name)
        if digit < 1 or digit > count:
            return False
        _ = self.choose(digit - 1)
        return True

    def enter(mut self) -> UInt8:
        if text_blank(self.draft) and len(self.attachments) == 0:
            if self.has_card:
                _ = self.apply(self.card.submit())
                return PANE_OK
            var head = self.current()
            if head and len(head.value().options) > 0:
                return self.choose(
                    min(self.permission_selected, len(head.value().options) - 1)
                )
        self.sync()
        if self.completion_selected >= 0:
            return self.accept_completion(self.completion_selected)
        return self.submit()

    def insert(mut self, text: String) -> UInt8:
        if text.byte_length() == 0:
            return PANE_OK
        if self.has_card and self.other_target >= 0:
            var value = self.card.other_value(self.other_target)
            if not self.card.set_other_text(self.other_target, String(value, text)):
                self.refused_text += 1
                return PANE_REFUSED_TEXT
            return PANE_OK
        if self.draft.byte_length() + text.byte_length() > PANE_MAX_PROMPT_BYTES:
            self.refused_text += 1
            return PANE_REFUSED_TEXT
        var at = text_floor(self.draft, self.cursor)
        self.draft = String(
            text_slice(self.draft, 0, at),
            text,
            text_slice(self.draft, at, self.draft.byte_length()),
        )
        self.cursor = at + text.byte_length()
        return PANE_OK

    def erase(mut self, start: Int, end: Int):
        if start >= end:
            return
        self.draft = String(
            text_slice(self.draft, 0, start),
            text_slice(self.draft, end, self.draft.byte_length()),
        )
        self.cursor = start

    def key(
        mut self,
        name: String,
        shift: Bool = False,
        ctrl: Bool = False,
        alt: Bool = False,
        cmd: Bool = False,
    ) -> Bool:
        self.refresh()
        self.sync()
        if len(self.completions) > 0:
            if (name == "up" or name == "down") and not cmd and not alt:
                self.navigate(name == "down")
                return True
            if name == "escape":
                self.completion_dismissed = True
                self.clear_completions()
                return True
        elif not ctrl and not alt and not cmd and self.ask_key(name, shift):
            return True
        if ctrl or alt or cmd:
            return False
        if name == "enter":
            if shift:
                _ = self.insert("\n")
            else:
                _ = self.enter()
            return True
        if self.other_target >= 0:
            return False
        if name == "backspace":
            self.erase(text_floor(self.draft, self.cursor - 1), self.cursor)
            return True
        if name == "delete":
            self.erase(self.cursor, text_ceil(self.draft, self.cursor + 1))
            return True
        if name == "left":
            self.cursor = text_floor(self.draft, self.cursor - 1)
            return True
        if name == "right":
            self.cursor = text_ceil(self.draft, self.cursor + 1)
            return True
        if name == "home":
            self.cursor = 0
            return True
        if name == "end":
            self.cursor = self.draft.byte_length()
            return True
        return False

    def attach(mut self, images: List[ChatImage]) -> UInt8:
        var combined = self.attachments.copy()
        for image in images:
            combined.append(image.copy())
        var problem = validate_images(self.draft, combined)
        if problem:
            self.draft_error = problem.take()
            return PANE_REFUSED_IMAGES
        self.attachments = combined^
        self.draft_error = None
        return PANE_OK

    def detach(mut self, index: Int) -> Bool:
        if index < 0 or index >= len(self.attachments):
            return False
        _ = self.attachments.pop(index)
        return True
