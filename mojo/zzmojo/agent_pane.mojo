from mjui.box import CENTER, START, TextDeco
from mjui.input import MOD_ALT, MOD_CMD, MOD_CTRL, MOD_SHIFT, KeyEvent
from mjui.keys import Key
from mjui.ui import CURSOR_HAND, Style, TextStyle, Ui
from mjui.widgets.icons import (
    ICON_ALERT_TRIANGLE,
    ICON_CHEVRON_DOWN,
    ICON_CHEVRON_RIGHT,
    ICON_CIRCLE_CHECK,
    ICON_CIRCLE_X,
    ICON_CLOCK,
    ICON_INFO,
    ICON_LOADER,
    ICON_SQUARE,
    ICON_WRENCH,
)
from mjui.widgets.key import sub
from mjui.widgets.penna import PennaTheme, button, row
from .agent_chat import (
    ANSWER_QUESTIONS,
    ASK_QUESTIONS,
    CLOSE_ANSWERED,
    ITEM_ERROR,
    ITEM_REASONING,
    PLAN_STEP_CANCELLED,
    PLAN_STEP_DONE,
    PLAN_STEP_RUNNING,
    STATUS_DECLINED,
    STATUS_FAILED,
    ChatView,
)
from .agent_completion import command_label
from .agent_events import AgentPaneState, FeedResult
from .agent_lens import (
    AGENT_WELCOME_DETAIL,
    PendingPermission,
    agent_empty,
    agent_placeholder,
)
from .agent_pane_model import PANE_OK, AgentEffect, AgentModel
from .agent_question import QuestionCard
from .agent_timeline import COMPOSER_QUEUE, COMPOSER_SEND, context_usage_tooltip
from .chat_rows import (
    AGENT_DONE,
    AGENT_FAILED,
    AGENT_STOPPED,
    AGENT_WAITING,
    AGENT_WORKING,
    FOLD_FAILED,
    LABEL_DONE,
    LABEL_LIVE,
    LABEL_THINKING,
    UNIT_AGENTS,
    UNIT_ASK,
    UNIT_NOTICE,
    UNIT_PLAN,
    UNIT_RECORD,
    UNIT_RUN,
    UNIT_SUBAGENT,
    UNIT_TEXT,
    UNIT_USER,
    RunLabel,
    TurnRows,
    Unit,
    agent_ms,
    agent_state,
    answered,
    background_rows,
    duration,
    fold_label,
    reasoning_body,
    run_shows,
    turn_rows,
    unit_at,
)

comptime AGENT_KEY_PASS: UInt8 = 0
comptime AGENT_KEY_TAKEN: UInt8 = 1
comptime AGENT_KEY_TEXT: UInt8 = 2

comptime AGENT_COMMAND_SUBMIT: UInt8 = 0
comptime AGENT_COMMAND_CANCEL: UInt8 = 1
comptime AGENT_COMMAND_UNQUEUE: UInt8 = 2
comptime AGENT_COMMAND_SET_OPTION: UInt8 = 3
comptime AGENT_COMMAND_FOLLOW: UInt8 = 4
comptime AGENT_COMMAND_DISMISS: UInt8 = 5

comptime AGENT_MAX_OPEN = 64
comptime AGENT_MAX_ROWS = 48
comptime AGENT_MAX_PROVIDER: UInt8 = 1

comptime AGENT_SLOT_SCROLL = 1
comptime AGENT_SLOT_TAIL = 2
comptime AGENT_SLOT_ACTION = 3
comptime AGENT_SLOT_UNQUEUE = 4
comptime AGENT_SLOT_COMPOSER = 5
comptime AGENT_SLOT_JUMP = 6
comptime AGENT_SLOT_SUBMIT = 7
comptime AGENT_SLOT_DISMISS = 8
comptime AGENT_SLOT_DRAFT = 9
comptime AGENT_SLOT_OPTION = 16
comptime AGENT_SLOT_COMPLETION = 64
comptime AGENT_SLOT_ATTACHMENT = 128
comptime AGENT_SLOT_CHOICE = 256
comptime AGENT_SLOT_TEXT = 4096
comptime AGENT_SLOT_FOLD = 20000
comptime AGENT_SLOT_RUN = 40000
comptime AGENT_SLOT_LIVE = 60000


struct AgentClicks(Movable):
    var turn: Int
    var has_run: Bool
    var run: String
    var action: Bool
    var unqueue: Bool
    var jump: Bool
    var option: Int
    var dismiss: Bool
    var question: Int
    var choice: Int
    var submit: Bool
    var completion: Int
    var detach: Int

    def __init__(out self):
        self.turn = -1
        self.has_run = False
        self.run = String()
        self.action = False
        self.unqueue = False
        self.jump = False
        self.option = -1
        self.dismiss = False
        self.question = -1
        self.choice = -1
        self.submit = False
        self.completion = -1
        self.detach = -1


def _say(mut ui: Ui, th: PennaTheme, text: String, color: UInt32, wrap: Bool = True):
    ui.label(text, TextStyle(th.flat.font, th.flat.size, color, wrap))


def _press(mut ui: Ui, key: UInt64) -> Bool:
    var response = ui.interact(key, activate_on_release=True)
    if response.hovered:
        ui.cursor = CURSOR_HAND
    return response.clicked


def _listed(ids: List[String], id: String) -> Bool:
    for known in ids:
        if known == id:
            return True
    return False


def _opened(turns: List[UInt64], id: UInt64) -> Bool:
    for known in turns:
        if known == id:
            return True
    return False


def _chevron(mut ui: Ui, th: PennaTheme, expanded: Bool):
    if expanded:
        ui.icon(ICON_CHEVRON_DOWN, th.flat.icons, 12, th.flat.dim)
    else:
        ui.icon(ICON_CHEVRON_RIGHT, th.flat.icons, 12, th.flat.dim)


def agent_run_text(label: RunLabel) -> String:
    if label.kind == LABEL_LIVE:
        if label.text:
            return String(label.action, " ", label.text.value())
        return String(label.action, " ")
    if label.kind == LABEL_THINKING:
        if label.text:
            return label.text.value().copy()
        return "Thinking"
    if label.text:
        return label.text.value().copy()
    return String()


def agent_footer(kind: UInt8, took: Optional[String]) -> Optional[String]:
    if kind == AGENT_WORKING:
        if took:
            return Optional[String](String("Working for ", took.value()))
        return None
    if kind == AGENT_WAITING:
        if took:
            return Optional[String](String("Waiting for you after ", took.value()))
        return None
    if kind == AGENT_DONE:
        if took:
            return Optional[String](String("Done in ", took.value()))
        return Optional[String](String("Done"))
    if kind == AGENT_FAILED:
        if took:
            return Optional[String](String("Failed after ", took.value()))
        return Optional[String](String("Failed"))
    if took:
        return Optional[String](String("Stopped after ", took.value()))
    return Optional[String](String("Stopped"))


def agent_record_text(view: ChatView, at: Int) -> String:
    var count = 1
    if view.requests[at].ask.kind == ASK_QUESTIONS:
        count = max(len(view.requests[at].ask.questions), 1)
    var noun = "questions"
    if count == 1:
        noun = "question"
    var answers = String()
    if view.requests[at].closed:
        if view.requests[at].close.kind != CLOSE_ANSWERED:
            answers = "no answer"
        elif view.requests[at].close.answer != ANSWER_QUESTIONS:
            answers = "answered"
        else:
            for answer in view.requests[at].close.answers:
                for value in answer.answers:
                    if answers.byte_length() > 0:
                        answers += ", "
                    answers += value
    return String("Asked ", count, " ", noun, " · ", answers)


def _step(mut ui: Ui, th: PennaTheme, view: ChatView, id: String):
    var at = view.item(id)
    if at < 0:
        return
    if view.items[at].kind == ITEM_REASONING:
        var body = reasoning_body(view.items[at].body.text)
        if body:
            _say(ui, th, body.value(), th.flat.dim)
        return
    var color = th.flat.dim
    if view.items[at].status == STATUS_FAILED:
        color = th.flat.danger
    elif view.items[at].status == STATUS_DECLINED:
        color = th.flat.warning
    var text = view.items[at].body.title.copy()
    if view.items[at].body.target:
        if view.items[at].body.target.value().byte_length() > 0:
            text = view.items[at].body.target.value().copy()
    ui.open(Style.row().grow().spacing(6).align(START, CENTER))
    if view.items[at].is_open():
        ui.icon(ICON_LOADER, th.flat.icons, 13, th.flat.accent)
    else:
        ui.icon(ICON_WRENCH, th.flat.icons, 13, color)
    _say(ui, th, text, color, False)
    ui.close()


def _steps(mut ui: Ui, th: PennaTheme, view: ChatView, items: List[String]):
    ui.open(Style.col().grow().spacing(2).pad_sides(12, 0, 0, 0))
    for id in items:
        _step(ui, th, view, id)
    ui.close()


def _run(
    mut ui: Ui,
    th: PennaTheme,
    key: UInt64,
    view: ChatView,
    unit: Unit,
    open_runs: List[String],
    mut clicks: AgentClicks,
):
    if len(unit.items) == 0:
        return
    var shows = run_shows(view, unit.items)
    var expanded = shows and _listed(open_runs, unit.items[0])
    var slot = sub(key, AGENT_SLOT_RUN + max(view.item(unit.items[0]), 0))
    if shows and _press(ui, slot):
        clicks.has_run = True
        clicks.run = unit.items[0].copy()
    ui.open(Style.row().grow().spacing(6).align(START, CENTER).id(slot))
    _say(ui, th, agent_run_text(unit.label), th.flat.dim, False)
    if shows:
        _chevron(ui, th, expanded)
    ui.close()
    if expanded:
        _steps(ui, th, view, unit.items)


def _agent(mut ui: Ui, th: PennaTheme, view: ChatView, id: String, now: UInt64):
    var at = view.item(id)
    if at < 0:
        return
    var title = view.items[at].body.title.copy()
    if title.byte_length() == 0:
        title = "Subagent"
    var kind = AGENT_STOPPED
    var state = agent_state(view, id)
    if state:
        kind = state.value().kind
    var took: Optional[String] = None
    var ms = agent_ms(view, id, now)
    if ms:
        if view.items[at].is_open() or ms.value() >= 1000:
            took = Optional[String](duration(ms.value()))
    ui.open(Style.col().grow().pad(10, 6).spacing(2).stroke(1, th.flat.border))
    ui.open(Style.row().grow().spacing(6).align(START, CENTER))
    if kind == AGENT_WORKING:
        ui.icon(ICON_LOADER, th.flat.icons, 13, th.flat.accent)
    elif kind == AGENT_WAITING:
        ui.icon(ICON_ALERT_TRIANGLE, th.flat.icons, 13, th.flat.warning)
    elif kind == AGENT_DONE:
        ui.icon(ICON_CIRCLE_CHECK, th.flat.icons, 13, th.flat.ok)
    elif kind == AGENT_FAILED:
        ui.icon(ICON_CIRCLE_X, th.flat.icons, 13, th.flat.danger)
    else:
        ui.icon(ICON_SQUARE, th.flat.icons, 13, th.flat.dim)
    _say(ui, th, title, th.flat.fg, False)
    if kind == AGENT_WAITING:
        _say(ui, th, "Needs you", th.flat.warning, False)
    ui.close()
    var footer = agent_footer(kind, took)
    if footer:
        _say(ui, th, footer.value(), th.flat.dim, False)
    ui.close()


def _plan(mut ui: Ui, th: PennaTheme, view: ChatView, id: String):
    var at = view.item(id)
    if at < 0:
        return
    var done = 0
    for step in view.items[at].body.steps:
        if step.status == PLAN_STEP_DONE:
            done += 1
    ui.open(Style.col().grow().pad(10, 6).spacing(4).stroke(1, th.flat.border))
    ui.open(Style.row().grow().spacing(8).align(START, CENTER))
    ui.label("Plan", TextStyle(th.flat.font_bold, th.flat.size, th.flat.fg, False))
    _say(
        ui,
        th,
        String(done, " of ", len(view.items[at].body.steps)),
        th.flat.dim,
        False,
    )
    ui.close()
    for step in view.items[at].body.steps:
        ui.open(Style.row().grow().spacing(6).align(START, CENTER))
        if step.status == PLAN_STEP_DONE:
            ui.icon(ICON_CIRCLE_CHECK, th.flat.icons, 13, th.flat.ok)
        elif step.status == PLAN_STEP_RUNNING:
            ui.icon(ICON_LOADER, th.flat.icons, 13, th.flat.accent)
        elif step.status == PLAN_STEP_CANCELLED:
            ui.icon(ICON_CIRCLE_X, th.flat.icons, 13, th.flat.dim)
        else:
            ui.icon(ICON_CLOCK, th.flat.icons, 13, th.flat.dim)
        _say(ui, th, step.text, th.flat.fg)
        ui.close()
    ui.close()


def _record(mut ui: Ui, th: PennaTheme, view: ChatView, request: UInt64):
    var at = view.request(request)
    if at < 0:
        return
    var pairs = answered(view.requests[at])
    if len(pairs) == 0:
        _say(ui, th, agent_record_text(view, at), th.flat.dim)
        return
    ui.open(Style.col().grow().spacing(2))
    for pair in pairs:
        _say(ui, th, pair.label, th.flat.fg)
        _say(ui, th, pair.value, th.flat.dim)
    ui.close()


def _unit(
    mut ui: Ui,
    th: PennaTheme,
    key: UInt64,
    view: ChatView,
    unit: Unit,
    open_runs: List[String],
    now: UInt64,
    mut clicks: AgentClicks,
):
    if unit.kind == UNIT_USER:
        var at = view.item(unit.id)
        if at >= 0:
            ui.open(
                Style.col().grow().pad(10, 6).fill(th.flat.surface, th.flat.radius)
            )
            _say(ui, th, view.items[at].body.text, th.flat.fg)
            ui.close()
    elif unit.kind == UNIT_TEXT:
        var at = view.item(unit.id)
        if at >= 0 and view.items[at].body.text.byte_length() > 0:
            ui.text_growing(
                sub(key, AGENT_SLOT_TEXT + at),
                view.items[at].body.text.unsafe_ptr(),
                view.items[at].body.text.byte_length(),
                TextStyle(th.flat.font, th.flat.size, th.flat.fg, True),
            )
    elif unit.kind == UNIT_RUN:
        _run(ui, th, key, view, unit, open_runs, clicks)
    elif unit.kind == UNIT_SUBAGENT:
        _agent(ui, th, view, unit.id, now)
    elif unit.kind == UNIT_AGENTS:
        for id in unit.items:
            _agent(ui, th, view, id, now)
    elif unit.kind == UNIT_PLAN:
        _plan(ui, th, view, unit.id)
    elif unit.kind == UNIT_NOTICE:
        var at = view.item(unit.id)
        if at >= 0:
            ui.open(Style.row().grow().spacing(6).align(START, CENTER))
            if view.items[at].kind == ITEM_ERROR:
                ui.icon(ICON_ALERT_TRIANGLE, th.flat.icons, 13, th.flat.danger)
            else:
                ui.icon(ICON_INFO, th.flat.icons, 13, th.flat.dim)
            _say(ui, th, view.items[at].body.text, th.flat.fg)
            ui.close()
    elif unit.kind == UNIT_ASK:
        var at = view.request(unit.request)
        if at >= 0:
            ui.open(Style.row().grow().spacing(6).align(START, CENTER))
            ui.icon(ICON_ALERT_TRIANGLE, th.flat.icons, 13, th.flat.warning)
            _say(ui, th, view.requests[at].ask.title, th.flat.fg)
            ui.close()
    elif unit.kind == UNIT_RECORD:
        _record(ui, th, view, unit.request)


def _turn(
    mut ui: Ui,
    th: PennaTheme,
    key: UInt64,
    view: ChatView,
    turn: Int,
    open_turns: List[UInt64],
    open_runs: List[String],
    now: UInt64,
    mut clicks: AgentClicks,
):
    var rows = turn_rows(view, turn, now)
    var live = view.turns[turn].is_open()
    var lead = 0
    while lead < len(rows.shown) and rows.shown[lead].kind == UNIT_USER:
        lead += 1
    var moving = live and not view.waiting()
    var settled = len(rows.shown)
    if moving and settled > lead:
        if (
            rows.shown[settled - 1].kind == UNIT_RUN
            and rows.shown[settled - 1].label.kind != LABEL_DONE
        ):
            settled -= 1
    ui.open(Style.col().grow().spacing(3))
    for index in range(lead):
        _unit(ui, th, key, view, rows.shown[index], open_runs, now, clicks)
    ui.open(Style.col().grow().spacing(3))
    if rows.has_fold and len(rows.folded) > 0:
        var expanded = _opened(open_turns, view.turns[turn].id)
        var slot = sub(key, AGENT_SLOT_FOLD + turn)
        if _press(ui, slot):
            clicks.turn = turn
        ui.open(Style.row().grow().spacing(6).align(START, CENTER).id(slot))
        if rows.fold == FOLD_FAILED:
            ui.icon(ICON_ALERT_TRIANGLE, th.flat.icons, 13, th.flat.danger)
        _say(ui, th, fold_label(rows.fold, rows.ms), th.flat.dim, False)
        _chevron(ui, th, expanded)
        ui.close()
        if expanded:
            for unit in rows.folded:
                _unit(ui, th, key, view, unit, open_runs, now, clicks)
    for index in range(lead, settled):
        _unit(ui, th, key, view, rows.shown[index], open_runs, now, clicks)
    if moving:
        var started = view.turns[turn].started_at
        var seconds: UInt64 = 0
        if now > started:
            seconds = (now - started) / 1000
        var shows = False
        var expanded = False
        var slot = sub(key, AGENT_SLOT_LIVE + turn)
        if settled < len(rows.shown):
            shows = run_shows(view, rows.shown[settled].items)
            if shows and len(rows.shown[settled].items) > 0:
                expanded = _listed(open_runs, rows.shown[settled].items[0])
                if _press(ui, slot):
                    clicks.has_run = True
                    clicks.run = rows.shown[settled].items[0].copy()
        ui.open(Style.row().grow().spacing(6).align(START, CENTER).id(slot))
        ui.icon(ICON_LOADER, th.flat.icons, 13, th.flat.accent)
        if rows.activity:
            _say(ui, th, rows.activity.value(), th.flat.dim, False)
        else:
            _say(ui, th, "Thinking", th.flat.dim, False)
        if seconds > 0 and seconds < 86400:
            _say(ui, th, duration(seconds * 1000), th.flat.dim, False)
        if shows:
            _chevron(ui, th, expanded)
        ui.close()
        if expanded:
            _steps(ui, th, view, rows.shown[settled].items)
    ui.close()
    ui.close()


def _thread(
    mut ui: Ui,
    th: PennaTheme,
    key: UInt64,
    view: ChatView,
    open_turns: List[UInt64],
    open_runs: List[String],
    now: UInt64,
    mut clicks: AgentClicks,
) -> Int:
    var pending = background_rows(view)
    var entries = List[Int]()
    var next = 0
    for turn in range(len(view.turns)):
        while next < len(pending):
            var at = unit_at(view, pending[next])
            if not at:
                break
            if at.value() >= view.turns[turn].started_at:
                break
            entries.append(-(next + 1))
            next += 1
        entries.append(turn)
    while next < len(pending):
        entries.append(-(next + 1))
        next += 1
    var first = max(len(entries) - AGENT_MAX_ROWS, 0)
    if first > 0:
        _say(ui, th, String(first, " earlier rows"), th.flat.dim, False)
    for index in range(first, len(entries)):
        if entries[index] >= 0:
            _turn(
                ui, th, key, view, entries[index], open_turns, open_runs, now, clicks
            )
        else:
            _unit(
                ui,
                th,
                key,
                view,
                pending[-entries[index] - 1],
                open_runs,
                now,
                clicks,
            )
    return first


def _permission(
    mut ui: Ui,
    th: PennaTheme,
    key: UInt64,
    head: PendingPermission,
    selected: Int,
    mut clicks: AgentClicks,
) raises:
    ui.open(
        Style.col().grow().pad(10, 8).spacing(6).fill(th.flat.surface, th.flat.radius)
    )
    _say(ui, th, head.title, th.flat.fg)
    ui.open(Style.row().grow().spacing(6).wrap())
    for index in range(min(len(head.options), AGENT_SLOT_COMPLETION - AGENT_SLOT_OPTION)):
        if button(
            ui,
            th,
            sub(key, AGENT_SLOT_OPTION + index),
            head.options[index].name,
            prominent=index == selected,
        ):
            clicks.option = index
    if button(ui, th, sub(key, AGENT_SLOT_DISMISS), "Dismiss"):
        clicks.dismiss = True
    ui.close()
    ui.close()


def _questions(
    mut ui: Ui,
    th: PennaTheme,
    key: UInt64,
    card: QuestionCard,
    other_target: Int,
    mut clicks: AgentClicks,
) raises:
    ui.open(
        Style.col().grow().pad(10, 8).spacing(6).fill(th.flat.surface, th.flat.radius)
    )
    for question in range(min(len(card.questions), 32)):
        if card.questions[question].header:
            _say(ui, th, card.questions[question].header.value(), th.flat.dim, False)
        _say(ui, th, card.questions[question].question, th.flat.fg)
        var count = min(len(card.questions[question].options), 62)
        for option in range(count):
            if row(
                ui,
                th,
                sub(key, AGENT_SLOT_CHOICE + question * 64 + option),
                card.questions[question].options[option].label,
                selected=card.is_picked(question, option),
            ):
                clicks.question = question
                clicks.choice = option
        if card.questions[question].allow_other:
            if row(
                ui,
                th,
                sub(key, AGENT_SLOT_CHOICE + question * 64 + count),
                "Other",
                selected=card.other_selected(question),
            ):
                clicks.question = question
                clicks.choice = len(card.questions[question].options)
            if card.other_selected(question):
                var typed = card.other_value(question)
                if card.questions[question].secret:
                    var masked = String()
                    for _ in range(typed.byte_length()):
                        masked += "•"
                    typed = masked^
                if typed.byte_length() == 0 and other_target == question:
                    typed = "Type an answer"
                _say(ui, th, typed, th.flat.dim)
    var problem = card.problem()
    if problem:
        _say(ui, th, problem.value(), th.flat.warning)
    ui.open(Style.row().grow().spacing(6))
    if button(
        ui,
        th,
        sub(key, AGENT_SLOT_SUBMIT),
        "Submit",
        disabled=not card.is_complete(),
        prominent=True,
    ):
        clicks.submit = True
    if button(ui, th, sub(key, AGENT_SLOT_DISMISS), "Dismiss"):
        clicks.dismiss = True
    ui.close()
    ui.close()


struct AgentPane(Movable):
    var pane: UInt64
    var model: AgentModel
    var left: Float32
    var top: Float32
    var width: Float32
    var height: Float32
    var scale: Float32
    var visible: Bool
    var focused: Bool
    var now: UInt64
    var open_turns: List[UInt64]
    var open_runs: List[String]
    var session_id: Optional[String]
    var model_name: Optional[String]
    var effort: Optional[String]
    var hidden_rows: Int

    def __init__(out self, pane: UInt64, client: Optional[UInt64] = None):
        self.pane = pane
        self.model = AgentModel(pane, client)
        self.left = 0
        self.top = 0
        self.width = 1
        self.height = 1
        self.scale = 1
        self.visible = False
        self.focused = False
        self.now = 0
        self.open_turns = List[UInt64]()
        self.open_runs = List[String]()
        self.session_id = None
        self.model_name = None
        self.effort = None
        self.hidden_rows = 0

    def descriptor(
        mut self,
        provider: UInt8,
        session_id: Optional[String],
        model: Optional[String],
        effort: Optional[String],
    ) raises:
        if provider > AGENT_MAX_PROVIDER:
            raise Error(String("unknown agent provider ", provider))
        self.model.provider = Optional[UInt8](provider)
        self.session_id = session_id.copy()
        self.model_name = model.copy()
        self.effort = effort.copy()

    def take_outbox(mut self) -> List[AgentEffect]:
        return self.model.take_outbox()

    def updates(mut self, first_seq: UInt64, items: List[List[UInt8]]) -> FeedResult:
        return self.model.updates(first_seq, items)

    def set_state(mut self, state: AgentPaneState) -> UInt8:
        return self.model.set_state(state)

    def lagged(mut self) -> Bool:
        return self.model.lagged()

    def disconnect(mut self):
        self.model.disconnect()

    def tick(mut self, now: UInt64):
        self.now = now

    def layout(
        mut self,
        left: Float32,
        top: Float32,
        width: Float32,
        height: Float32,
        scale: Float32,
    ):
        self.left = left
        self.top = top
        self.width = max(width, Float32(1))
        self.height = max(height, Float32(1))
        self.scale = scale

    def set_visible(mut self, visible: Bool):
        self.visible = visible

    def set_focused(mut self, focused: Bool):
        self.focused = focused

    def inside(self, x: Float32, y: Float32) -> Bool:
        return (
            self.visible
            and x >= self.left
            and y >= self.top
            and x < self.left + self.width
            and y < self.top + self.height
        )

    def pointer(
        mut self,
        phase: UInt8,
        button: UInt8,
        pressed: UInt8,
        x: Float32,
        y: Float32,
        mods: UInt32,
        clicks: Int32 = 1,
    ) raises -> Bool:
        return self.inside(x, y)

    def wheel(
        mut self, x: Float32, y: Float32, dx: Float32, dy: Float32, mods: UInt32
    ) -> Bool:
        if not self.inside(x, y):
            return False
        if dy != 0:
            self.model.follow = False
        return True

    def key(mut self, event: KeyEvent) raises -> UInt8:
        if event.release or event.consumed or not self.focused:
            return AGENT_KEY_PASS
        var name = String()
        if event.key == Key.ENTER or event.key == Key.KP_ENTER:
            name = "enter"
        elif event.key == Key.ESCAPE:
            name = "escape"
        elif event.key == Key.UP:
            name = "up"
        elif event.key == Key.DOWN:
            name = "down"
        elif event.key == Key.LEFT:
            name = "left"
        elif event.key == Key.RIGHT:
            name = "right"
        elif event.key == Key.TAB:
            name = "tab"
        elif event.key == Key.BACKSPACE:
            name = "backspace"
        elif event.key == Key.DELETE:
            name = "delete"
        elif event.key == Key.HOME:
            name = "home"
        elif event.key == Key.END:
            name = "end"
        elif event.key >= Key.DIGIT_1 and event.key <= Key.DIGIT_9:
            name = String(Int(event.key) - Int(Key.DIGIT_1) + 1)
        var held = event.mods & (MOD_CTRL | MOD_ALT | MOD_CMD)
        if name.byte_length() > 0:
            if self.model.key(
                name,
                event.mods & MOD_SHIFT != 0,
                event.mods & MOD_CTRL != 0,
                event.mods & MOD_ALT != 0,
                event.mods & MOD_CMD != 0,
            ):
                return AGENT_KEY_TAKEN
        if event.text and held == 0:
            return AGENT_KEY_TEXT
        return AGENT_KEY_PASS

    def text(mut self, text: String):
        _ = self.model.insert(text)

    def command(
        mut self, command: UInt8, option: String = "", value: String = ""
    ) raises -> UInt8:
        if command == AGENT_COMMAND_SUBMIT:
            return self.model.submit()
        if command == AGENT_COMMAND_CANCEL:
            return self.model.cancel()
        if command == AGENT_COMMAND_UNQUEUE:
            return self.model.unqueue()
        if command == AGENT_COMMAND_SET_OPTION:
            return self.model.set_option(option, value)
        if command == AGENT_COMMAND_FOLLOW:
            self.model.follow = True
            return PANE_OK
        if command == AGENT_COMMAND_DISMISS:
            var head = self.model.current()
            if not head:
                return PANE_OK
            return self.model.respond(head.value().request_id, None)
        raise Error(String("unsupported agent command ", command))

    def toggle_turn(mut self, id: UInt64):
        var kept = List[UInt64]()
        var found = False
        for known in self.open_turns:
            if known == id:
                found = True
            else:
                kept.append(known)
        if not found:
            if len(kept) >= AGENT_MAX_OPEN:
                _ = kept.pop(0)
            kept.append(id)
        self.open_turns = kept^

    def toggle_run(mut self, id: String):
        var kept = List[String]()
        var found = False
        for known in self.open_runs:
            if known == id:
                found = True
            else:
                kept.append(known.copy())
        if not found:
            if len(kept) >= AGENT_MAX_OPEN:
                _ = kept.pop(0)
            kept.append(id.copy())
        self.open_runs = kept^

    def settings_line(self) -> String:
        if self.model.feed.settings:
            return self.model.feed.settings.value().summary()
        var out = String()
        if self.model_name:
            out += self.model_name.value()
        if self.effort:
            if out.byte_length() > 0:
                out += " · "
            out += self.effort.value()
        return out^

    def build(mut self, mut ui: Ui, th: PennaTheme, key: UInt64) raises:
        var rect = ui.rect_of(key)
        if rect.key != 0:
            self.layout(
                rect.x0, rect.y0, rect.x1 - rect.x0, rect.y1 - rect.y0, ui.scale
            )
        self.model.sync()
        var clicks = AgentClicks()
        ui.open(Style.col().grow().grow_h().clipped().id(key))
        ui.scroll_begin(
            sub(key, AGENT_SLOT_SCROLL),
            Style.col().grow().grow_h().pad(12, 8).spacing(12),
        )
        if len(self.model.feed.chat.turns) == 0 and len(self.model.feed.chat.items) == 0:
            var empty = agent_empty(
                self.model.feed.connected,
                self.model.feed.state.phase,
                self.model.provider,
            )
            if empty.welcome:
                ui.label(
                    empty.text,
                    TextStyle(th.flat.font_bold, th.flat.size, th.flat.fg, True),
                )
                _say(ui, th, AGENT_WELCOME_DETAIL, th.flat.dim)
            else:
                _say(ui, th, empty.text, th.flat.dim)
            self.hidden_rows = 0
        else:
            self.hidden_rows = _thread(
                ui,
                th,
                key,
                self.model.feed.chat,
                self.open_turns,
                self.open_runs,
                self.now,
                clicks,
            )
        ui.open(Style.row().grow().height(1).id(sub(key, AGENT_SLOT_TAIL)))
        ui.close()
        ui.scroll_end()
        if self.model.follow:
            ui.scroll_into_view(
                sub(key, AGENT_SLOT_SCROLL), sub(key, AGENT_SLOT_TAIL), False
            )
        ui.open(Style.col().grow().pad(12, 8).spacing(6))
        if not self.model.follow:
            if button(ui, th, sub(key, AGENT_SLOT_JUMP), "Jump to latest"):
                clicks.jump = True
        var head = self.model.current()
        if self.model.has_card:
            _questions(
                ui, th, key, self.model.card, self.model.other_target, clicks
            )
        elif head:
            _permission(
                ui, th, key, head.value(), self.model.permission_selected, clicks
            )
        var queued = self.model.feed.state.queued_prompts
        if queued > 0:
            ui.open(Style.row().grow().spacing(8).align(START, CENTER))
            if queued == 1:
                _say(ui, th, "1 prompt queued", th.flat.dim, False)
            else:
                _say(ui, th, String(queued, " prompts queued"), th.flat.dim, False)
            if button(ui, th, sub(key, AGENT_SLOT_UNQUEUE), "Unqueue"):
                clicks.unqueue = True
            ui.close()
        var problem = self.model.error_line()
        if problem:
            _say(ui, th, problem.value(), th.flat.danger)
        if len(self.model.attachments) > 0:
            ui.open(Style.row().grow().spacing(6).wrap())
            for index in range(len(self.model.attachments)):
                if button(
                    ui,
                    th,
                    sub(key, AGENT_SLOT_ATTACHMENT + index),
                    String("Image ", index + 1, " ×"),
                ):
                    clicks.detach = index
            ui.close()
        for index in range(min(len(self.model.completions), AGENT_SLOT_ATTACHMENT - AGENT_SLOT_COMPLETION)):
            if row(
                ui,
                th,
                sub(key, AGENT_SLOT_COMPLETION + index),
                command_label(self.model.completions[index].command.name),
                selected=index == self.model.completion_selected,
            ):
                clicks.completion = index
        ui.open(
            Style.row()
            .grow()
            .pad(10, 8)
            .spacing(8)
            .stroke(1, th.flat.border)
            .id(sub(key, AGENT_SLOT_COMPOSER))
        )
        ui.open(Style.col().grow())
        if self.model.draft.byte_length() == 0:
            _say(ui, th, agent_placeholder(self.model.provider), th.flat.dim)
        else:
            var caret = Int32(-1)
            if self.focused and self.model.other_target < 0:
                caret = Int32(self.model.cursor)
            ui.edit_text(
                sub(key, AGENT_SLOT_DRAFT),
                self.model.draft.unsafe_ptr(),
                self.model.draft.byte_length(),
                TextStyle(th.flat.font, th.flat.size, th.flat.fg, True),
                TextDeco(0, 0, 0, caret, False, True, th.flat.caret, 0, 0, 0),
            )
        ui.close()
        var action = self.model.action()
        var title = "Stop"
        if action == COMPOSER_SEND:
            title = "Send"
        elif action == COMPOSER_QUEUE:
            title = "Queue"
        if button(
            ui,
            th,
            sub(key, AGENT_SLOT_ACTION),
            title,
            disabled=not self.model.action_enabled(),
            prominent=True,
        ):
            clicks.action = True
        ui.close()
        ui.open(Style.row().grow().spacing(8))
        var settings = self.settings_line()
        if settings.byte_length() > 0:
            _say(ui, th, settings, th.flat.dim, False)
        ui.spacer()
        if self.model.feed.usage.known:
            _say(
                ui,
                th,
                context_usage_tooltip(
                    self.model.feed.usage.used, self.model.feed.usage.size
                ),
                th.flat.dim,
                False,
            )
        ui.close()
        ui.close()
        ui.close()
        self.apply(clicks)

    def apply(mut self, clicks: AgentClicks):
        if clicks.turn >= 0 and clicks.turn < len(self.model.feed.chat.turns):
            var id = self.model.feed.chat.turns[clicks.turn].id
            self.toggle_turn(id)
        if clicks.has_run:
            self.toggle_run(clicks.run)
        if clicks.jump:
            self.model.follow = True
        if clicks.option >= 0:
            _ = self.model.choose(clicks.option)
        if clicks.question >= 0:
            _ = self.model.pick(clicks.question, clicks.choice)
        if clicks.submit and self.model.has_card:
            var step = self.model.card.submit()
            _ = self.model.apply(step)
        if clicks.dismiss:
            var head = self.model.current()
            if head:
                _ = self.model.respond(head.value().request_id, None)
        if clicks.unqueue:
            _ = self.model.unqueue()
        if clicks.detach >= 0:
            _ = self.model.detach(clicks.detach)
        if clicks.completion >= 0:
            _ = self.model.accept_completion(clicks.completion)
        if clicks.action:
            _ = self.model.press_action()
