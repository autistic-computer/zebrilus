from std.testing import assert_equal, assert_false, assert_raises, assert_true
from mjui.input import MOD_CTRL, KeyEvent
from mjui.keys import Key
from mjui.resources import resource_path
from mjui.sem import ROLE_BUTTON, sem_find
from mjui.ui import MOTION_REDUCED, Style, Ui
from mjui.widgets.key import sub
from mjui.widgets.penna import PennaTheme
from zzmojo.agent_events import AgentPaneState
from zzmojo.agent_lens import (
    AGENT_WELCOME_DETAIL,
    CONNECTION_READY,
    CONNECTION_RUNNING,
)
from zzmojo.agent_pane import (
    AGENT_COMMAND_FOLLOW,
    AGENT_KEY_PASS,
    AGENT_KEY_TAKEN,
    AGENT_KEY_TEXT,
    AGENT_SLOT_ACTION,
    AGENT_SLOT_CHOICE,
    AGENT_SLOT_FOLD,
    AGENT_SLOT_JUMP,
    AGENT_SLOT_OPTION,
    AGENT_SLOT_RUN,
    AGENT_SLOT_UNQUEUE,
    AgentPane,
)
from zzmojo.agent_pane_model import (
    EFFECT_ANSWER,
    EFFECT_CANCEL,
    EFFECT_PERMISSION,
    EFFECT_PROMPT,
    EFFECT_UNQUEUE,
    PANE_OK,
)

comptime BASE: UInt64 = 0x7000000000000000
comptime WIDTH: Float32 = 900
comptime HEIGHT: Float32 = 700
comptime CHAT = '"item":"chat","event":'
comptime DONE = '"status":{"status":"completed"}'


def batch(items: List[String]) -> List[List[UInt8]]:
    var out = List[List[UInt8]]()
    for item in items:
        var bytes = List[UInt8]()
        for byte in item.as_bytes():
            bytes.append(byte)
        out.append(bytes^)
    return out^


def state(phase: UInt8, queued: Int = 0) -> AgentPaneState:
    var found = AgentPaneState()
    found.phase = phase
    found.queued_prompts = queued
    return found^


def press(key: UInt16, mods: UInt32 = 0, text: Bool = False) -> KeyEvent:
    return KeyEvent(0, key, mods, 0, 0, 0, 0, False, False, text, False)


def opened(id: UInt64, provider: UInt8, phase: UInt8) raises -> AgentPane:
    var pane = AgentPane(id)
    pane.descriptor(provider, None, Optional[String](String("opus")), None)
    _ = pane.set_state(state(phase))
    pane.set_visible(True)
    pane.set_focused(True)
    return pane^


def frames(mut pane: AgentPane, mut ui: Ui, th: PennaTheme, count: Int) raises:
    for _ in range(count):
        ui.set_time(ui.time + 0.02)
        ui.begin_frame(WIDTH, HEIGHT)
        ui.open(Style.col().width(WIDTH).height(HEIGHT))
        pane.build(ui, th, BASE + pane.pane)
        ui.close()
        ui.end_frame()


def click(mut pane: AgentPane, mut ui: Ui, th: PennaTheme, slot: Int) raises:
    var rect = ui.rect_of(sub(BASE + pane.pane, slot))
    assert_true(rect.key != 0)
    var x = (rect.x0 + rect.x1) / 2
    var y = (rect.y0 + rect.y1) / 2
    ui.pointer_move(x, y)
    frames(pane, ui, th, 1)
    ui.pointer_down(x, y, 1)
    frames(pane, ui, th, 1)
    ui.pointer_up(x, y)
    frames(pane, ui, th, 2)


def shown(ui: Ui, text: String) -> Bool:
    return sem_find(ui, text) >= 0


def welcome(mut ui: Ui, th: PennaTheme) raises:
    var pane = opened(11, 1, CONNECTION_READY)
    frames(pane, ui, th, 2)
    assert_true(shown(ui, "Claude Code is ready"))
    assert_true(shown(ui, AGENT_WELCOME_DETAIL))
    assert_true(shown(ui, "Message Claude Code…"))
    assert_true(shown(ui, "opus"))
    assert_true(sem_find(ui, "Send", ROLE_BUTTON) >= 0)
    assert_equal(pane.width, WIDTH)
    assert_equal(pane.height, HEIGHT)
    with assert_raises(contains="unknown agent provider 2"):
        pane.descriptor(UInt8(2), None, None, None)
    with assert_raises(contains="unsupported agent command 9"):
        _ = pane.command(UInt8(9))
    pane.disconnect()
    frames(pane, ui, th, 1)
    assert_true(shown(ui, "The agent is offline."))
    assert_false(shown(ui, "Claude Code is ready"))


def transcript(mut ui: Ui, th: PennaTheme) raises:
    var pane = opened(12, 0, CONNECTION_READY)
    var items: List[String] = [
        String(
            '{"seq":1,',
            CHAT,
            '{"event":"turnStarted","turn":1,"by":{"by":"user","prompt":4},"at":1000}}',
        ),
        String(
            '{"seq":2,',
            CHAT,
            '{"event":"itemStarted","item":"u1","turn":1,"kind":"user","body":{"text":"port the pane"},"at":1000}}',
        ),
        String('{"seq":3,', CHAT, '{"event":"itemEnded","item":"u1",', DONE, ',"at":1000}}'),
        String(
            '{"seq":4,',
            CHAT,
            '{"event":"itemStarted","item":"x1","turn":1,"kind":"execute","body":{"title":"Bash","target":"cargo check"},"at":1100}}',
        ),
        String('{"seq":5,', CHAT, '{"event":"itemEnded","item":"x1",', DONE, ',"at":2100}}'),
        String(
            '{"seq":6,',
            CHAT,
            '{"event":"itemStarted","item":"a1","turn":1,"kind":"agent","body":{"text":"Working on it"},"at":2200}}',
        ),
        String('{"seq":7,', CHAT, '{"event":"itemAppended","item":"a1","text":" now"}}'),
        String('{"seq":8,', CHAT, '{"event":"itemEnded","item":"a1",', DONE, ',"at":2900}}'),
        String(
            '{"seq":9,',
            CHAT,
            '{"event":"turnEnded","turn":1,"outcome":{"outcome":"completed","stop_reason":"end_turn"},"at":4000}}',
        ),
    ]
    var result = pane.updates(1, batch(items))
    assert_equal(result.accepted, 9)
    assert_false(result.invalid)
    assert_equal(len(pane.model.feed.chat.items), 3)
    assert_equal(Int(pane.model.feed.chat.skipped), 0)
    pane.tick(9000)
    frames(pane, ui, th, 2)
    assert_true(shown(ui, "port the pane"))
    assert_true(shown(ui, "Working on it now"))
    assert_true(shown(ui, "Worked for 3s"))
    assert_false(shown(ui, "Ran cargo check"))
    assert_false(shown(ui, "Codex is ready"))
    click(pane, ui, th, AGENT_SLOT_FOLD)
    assert_equal(len(pane.open_turns), 1)
    assert_true(shown(ui, "Ran cargo check"))
    assert_false(shown(ui, "cargo check"))
    click(pane, ui, th, AGENT_SLOT_RUN + 1)
    assert_equal(len(pane.open_runs), 1)
    assert_equal(pane.open_runs[0], "x1")
    assert_true(shown(ui, "cargo check"))
    click(pane, ui, th, AGENT_SLOT_FOLD)
    assert_equal(len(pane.open_turns), 0)
    assert_false(shown(ui, "Ran cargo check"))
    assert_equal(len(pane.take_outbox()), 0)


def streaming(mut ui: Ui, th: PennaTheme) raises:
    var pane = opened(13, 1, CONNECTION_RUNNING)
    var items: List[String] = [
        String(
            '{"seq":1,',
            CHAT,
            '{"event":"turnStarted","turn":2,"by":{"by":"user","prompt":5},"at":1000}}',
        ),
        String(
            '{"seq":2,',
            CHAT,
            '{"event":"itemStarted","item":"a2","turn":2,"kind":"agent","body":{"text":"Hel"},"at":1100}}',
        ),
    ]
    assert_equal(pane.updates(1, batch(items)).accepted, 2)
    pane.tick(6000)
    frames(pane, ui, th, 2)
    assert_true(shown(ui, "Hel"))
    assert_true(shown(ui, "Thinking"))
    assert_true(shown(ui, "5s"))
    var more: List[String] = [
        String('{"seq":3,', CHAT, '{"event":"itemAppended","item":"a2","text":"lo"}}')
    ]
    assert_equal(pane.updates(3, batch(more)).accepted, 1)
    pane.tick(63000)
    frames(pane, ui, th, 2)
    assert_true(shown(ui, "Hello"))
    assert_false(shown(ui, "Hel"))
    assert_true(shown(ui, "1m 2s"))
    assert_true(sem_find(ui, "Stop", ROLE_BUTTON) >= 0)
    click(pane, ui, th, AGENT_SLOT_ACTION)
    var sent = pane.take_outbox()
    assert_equal(len(sent), 1)
    assert_equal(sent[0].kind, EFFECT_CANCEL)


def composer(mut ui: Ui, th: PennaTheme) raises:
    var pane = opened(14, 1, CONNECTION_READY)
    frames(pane, ui, th, 2)
    assert_equal(pane.key(press(UInt16(104), 0, True)), AGENT_KEY_TEXT)
    assert_equal(pane.key(press(UInt16(104), MOD_CTRL, True)), AGENT_KEY_PASS)
    pane.text("hi")
    frames(pane, ui, th, 2)
    assert_false(shown(ui, "Message Claude Code…"))
    click(pane, ui, th, AGENT_SLOT_ACTION)
    var sent = pane.take_outbox()
    assert_equal(len(sent), 1)
    assert_equal(sent[0].kind, EFFECT_PROMPT)
    assert_equal(sent[0].text, "hi")
    assert_equal(pane.model.draft, "")
    pane.text("yo")
    assert_equal(pane.key(press(Key.BACKSPACE)), AGENT_KEY_TAKEN)
    assert_equal(pane.model.draft, "y")
    assert_equal(pane.key(press(Key.ENTER)), AGENT_KEY_TAKEN)
    sent = pane.take_outbox()
    assert_equal(len(sent), 1)
    assert_equal(sent[0].text, "y")
    pane.set_focused(False)
    assert_equal(pane.key(press(Key.ENTER)), AGENT_KEY_PASS)
    assert_equal(pane.key(press(UInt16(104), 0, True)), AGENT_KEY_PASS)


def permission(mut ui: Ui, th: PennaTheme) raises:
    var pane = opened(15, 1, CONNECTION_RUNNING)
    var items: List[String] = [
        '{"seq":1,"item":"permissionRequested","request_id":7,"tool_call":{"toolCallId":"t1","title":"Run cargo check"},"options":[{"optionId":"allow","name":"Allow","kind":"allow_once"},{"optionId":"deny","name":"Deny","kind":"reject_once"}]}'
    ]
    assert_equal(pane.updates(1, batch(items)).accepted, 1)
    frames(pane, ui, th, 2)
    assert_true(shown(ui, "Run cargo check"))
    assert_true(sem_find(ui, "Allow", ROLE_BUTTON) >= 0)
    assert_true(sem_find(ui, "Deny", ROLE_BUTTON) >= 0)
    click(pane, ui, th, AGENT_SLOT_OPTION + 1)
    var sent = pane.take_outbox()
    assert_equal(len(sent), 1)
    assert_equal(sent[0].kind, EFFECT_PERMISSION)
    assert_equal(Int(sent[0].request), 7)
    assert_equal(sent[0].option.value(), "deny")
    assert_false(shown(ui, "Run cargo check"))


def questions(mut ui: Ui, th: PennaTheme) raises:
    var pane = opened(16, 1, CONNECTION_RUNNING)
    var items: List[String] = [
        '{"seq":1,"item":"permissionRequested","request_id":5,"tool_call":{"toolCallId":"t","title":"Ask"},"options":[],"questions":[{"id":"q0","header":"Scope","question":"Which q0?","options":[{"label":"one","description":"the safe one"},{"label":"two"}],"multiSelect":false,"allowOther":true,"secret":false}]}'
    ]
    assert_equal(pane.updates(1, batch(items)).accepted, 1)
    frames(pane, ui, th, 2)
    assert_true(shown(ui, "Scope"))
    assert_true(shown(ui, "Which q0?"))
    click(pane, ui, th, AGENT_SLOT_CHOICE + 1)
    var sent = pane.take_outbox()
    assert_equal(len(sent), 1)
    assert_equal(sent[0].kind, EFFECT_ANSWER)
    assert_equal(Int(sent[0].request), 5)
    assert_equal(sent[0].answers[0].question, "q0")
    assert_equal(sent[0].answers[0].answers[0], "two")
    assert_false(shown(ui, "Which q0?"))


def queue(mut ui: Ui, th: PennaTheme) raises:
    var pane = opened(17, 0, CONNECTION_RUNNING)
    _ = pane.set_state(state(CONNECTION_RUNNING, 2))
    frames(pane, ui, th, 2)
    assert_true(shown(ui, "2 prompts queued"))
    click(pane, ui, th, AGENT_SLOT_UNQUEUE)
    var sent = pane.take_outbox()
    assert_equal(len(sent), 1)
    assert_equal(sent[0].kind, EFFECT_UNQUEUE)
    assert_false(pane.wheel(WIDTH + 10, 10, 0, -3, 0))
    assert_true(pane.model.follow)
    assert_true(pane.pointer(0, 1, 1, 10, 10, 0))
    assert_false(pane.pointer(0, 1, 1, 10, HEIGHT + 10, 0))
    assert_true(pane.wheel(10, 10, 0, -3, 0))
    assert_false(pane.model.follow)
    frames(pane, ui, th, 2)
    assert_true(sem_find(ui, "Jump to latest", ROLE_BUTTON) >= 0)
    click(pane, ui, th, AGENT_SLOT_JUMP)
    assert_true(pane.model.follow)
    assert_false(sem_find(ui, "Jump to latest", ROLE_BUTTON) >= 0)
    _ = pane.wheel(10, 10, 0, -3, 0)
    assert_equal(pane.command(AGENT_COMMAND_FOLLOW), PANE_OK)
    assert_true(pane.model.follow)
    pane.set_visible(False)
    assert_false(pane.wheel(10, 10, 0, -3, 0))


def main() raises:
    var ui = Ui(1)
    ui.sem_on = True
    ui.motion = MOTION_REDUCED
    _ = ui.add_font(resource_path("fonts/Inter-Regular.ttf"))
    _ = ui.add_font(resource_path("fonts/Inter-SemiBold.ttf"))
    _ = ui.add_font(resource_path("fonts/JetBrainsMono-Regular.ttf"))
    _ = ui.add_font(resource_path("fonts/lucide.ttf"))
    var th = PennaTheme(4, 0, 1, 2, 3)
    welcome(ui, th)
    transcript(ui, th)
    streaming(ui, th)
    composer(ui, th)
    permission(ui, th)
    questions(ui, th)
    queue(ui, th)
    print("agent pane scenarios 7")
