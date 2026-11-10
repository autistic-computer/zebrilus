from std.testing import assert_equal, assert_false, assert_true
from zzmojo.agent_chat import ChatImage
from zzmojo.agent_events import FEED_OK, FEED_REFUSED_STATE, AgentPaneState
from zzmojo.agent_lens import CONNECTION_READY, CONNECTION_RUNNING
from zzmojo.agent_pane_model import (
    EFFECT_ACKNOWLEDGE,
    EFFECT_ANSWER,
    EFFECT_CANCEL,
    EFFECT_PERMISSION,
    EFFECT_PROMPT,
    EFFECT_REPLAY,
    EFFECT_SET_OPTION,
    EFFECT_UNQUEUE,
    IMAGE_COUNT_ERROR,
    IMAGE_EMPTY_ERROR,
    IMAGE_FORMAT_ERROR,
    PANE_OK,
    PANE_REFUSED_EMPTY,
    PANE_REFUSED_IMAGES,
    PANE_REFUSED_OFFLINE,
    PANE_REFUSED_OPTION,
    PANE_REFUSED_OUTBOX,
    PANE_REFUSED_QUEUE,
    PANE_REFUSED_READ_ONLY,
    PANE_REFUSED_REQUEST,
    QUEUE_FULL_ERROR,
    AgentModel,
)
from zzmojo.agent_timeline import COMPOSER_QUEUE, COMPOSER_SEND, COMPOSER_STOP

comptime SETTINGS = '[{"id":"model","currentValue":"opus","options":[{"value":"opus","name":"Opus"},{"value":"haiku","name":"Haiku"}]},{"id":"effort","currentValue":"xhigh","options":[{"value":"high","name":"High"},{"value":"xhigh","name":"Extra high"}]}]'
comptime CHOICES = '"options":[{"optionId":"allow","name":"Allow","kind":"allow_once"},{"optionId":"deny","name":"Deny","kind":"reject_once"}]'
comptime ASKED = '"questions":[{"id":"q0","header":"Scope","question":"Which q0?","options":[{"label":"one","description":"the safe one"},{"label":"two"}],"multiSelect":false,"allowOther":true,"secret":false}]'


def batch(items: List[String]) -> List[List[UInt8]]:
    var out = List[List[UInt8]]()
    for item in items:
        var bytes = List[UInt8]()
        for byte in item.as_bytes():
            bytes.append(byte)
        out.append(bytes^)
    return out^


def one(item: String) -> List[List[UInt8]]:
    var items: List[String] = [item.copy()]
    return batch(items)


def state(phase: UInt8, queued: Int = 0) -> AgentPaneState:
    var found = AgentPaneState()
    found.phase = phase
    found.queued_prompts = queued
    return found^


def ready(client: Optional[UInt64] = None) -> AgentModel:
    var model = AgentModel(9, client)
    _ = model.set_state(state(CONNECTION_READY))
    return model^


def asked(seq: Int, request: Int) -> String:
    return String(
        '{"seq":',
        seq,
        ',"item":"permissionRequested","request_id":',
        request,
        ',"tool_call":{"toolCallId":"t","title":"Ask"},"options":[],',
        ASKED,
        "}",
    )


def prompts() raises:
    var model = AgentModel(9)
    assert_equal(model.insert("hello"), PANE_OK)
    assert_equal(model.submit(), PANE_REFUSED_OFFLINE)
    assert_equal(model.draft, "hello")
    assert_equal(len(model.outbox), 0)
    assert_equal(model.set_state(state(CONNECTION_READY)), FEED_OK)
    assert_equal(model.action(), COMPOSER_SEND)
    assert_true(model.key("enter"))
    var sent = model.take_outbox()
    assert_equal(len(sent), 1)
    assert_equal(sent[0].kind, EFFECT_PROMPT)
    assert_equal(sent[0].text, "hello")
    assert_equal(len(sent[0].images), 0)
    assert_equal(model.draft, "")
    assert_equal(model.cursor, 0)
    assert_true(model.follow)
    assert_equal(model.submit(), PANE_REFUSED_EMPTY)
    assert_equal(len(model.outbox), 0)


def running() raises:
    var model = AgentModel(9)
    _ = model.set_state(state(CONNECTION_RUNNING))
    assert_equal(model.action(), COMPOSER_STOP)
    assert_true(model.action_enabled())
    assert_equal(model.press_action(), PANE_OK)
    _ = model.insert("next")
    assert_equal(model.action(), COMPOSER_QUEUE)
    assert_equal(model.press_action(), PANE_OK)
    var sent = model.take_outbox()
    assert_equal(len(sent), 2)
    assert_equal(sent[0].kind, EFFECT_CANCEL)
    assert_equal(sent[1].kind, EFFECT_PROMPT)
    assert_equal(sent[1].text, "next")
    _ = model.set_state(state(CONNECTION_RUNNING, 4))
    _ = model.insert("fifth")
    assert_equal(model.submit(), PANE_REFUSED_QUEUE)
    assert_equal(model.error_line().value(), QUEUE_FULL_ERROR)
    assert_equal(model.draft, "fifth")
    assert_equal(model.unqueue(), PANE_OK)
    sent = model.take_outbox()
    assert_equal(len(sent), 1)
    assert_equal(sent[0].kind, EFFECT_UNQUEUE)
    _ = model.set_state(state(CONNECTION_READY))
    assert_equal(model.unqueue(), PANE_REFUSED_EMPTY)
    assert_equal(model.set_state(state(CONNECTION_READY, 5)), FEED_REFUSED_STATE)
    assert_equal(model.feed.refused_states, 1)


def editing() raises:
    var model = ready()
    _ = model.insert("hé")
    assert_equal(model.cursor, 3)
    assert_true(model.key("backspace"))
    assert_equal(model.draft, "h")
    assert_true(model.key("enter", shift=True))
    assert_equal(model.draft, "h\n")
    assert_true(model.key("home"))
    _ = model.insert(">")
    assert_equal(model.draft, ">h\n")
    assert_true(model.key("delete"))
    assert_equal(model.draft, ">\n")
    assert_true(model.key("end"))
    assert_true(model.key("left"))
    assert_equal(model.cursor, 1)
    assert_false(model.key("a", ctrl=True))
    assert_equal(len(model.outbox), 0)


def settings() raises:
    var model = ready()
    var result = model.updates(
        1,
        one(String('{"seq":1,"item":"sessionReady","config_options":', SETTINGS, "}")),
    )
    assert_equal(result.accepted, 1)
    assert_true(Bool(model.feed.settings))
    _ = model.insert("/model haiku")
    assert_true(model.key("enter"))
    var sent = model.take_outbox()
    assert_equal(len(sent), 1)
    assert_equal(sent[0].kind, EFFECT_SET_OPTION)
    assert_equal(sent[0].option.value(), "model")
    assert_equal(sent[0].text, "haiku")
    assert_equal(model.draft, "")


def completions() raises:
    var model = ready()
    _ = model.updates(
        1,
        one(
            '{"seq":1,"item":"update","update":{"sessionUpdate":"available_commands_update","availableCommands":[{"name":"again"}]}}'
        ),
    )
    _ = model.insert("/ag")
    model.sync()
    assert_equal(len(model.completions), 1)
    assert_equal(model.completions[0].command.name, "again")
    assert_true(model.key("enter"))
    assert_equal(model.draft, "/again ")
    assert_equal(model.cursor, 7)
    assert_equal(len(model.outbox), 0)


def permissions() raises:
    var model = ready()
    var items: List[String] = [
        String(
            '{"seq":1,"item":"permissionRequested","request_id":1,"tool_call":{"toolCallId":"t1","title":"Run cargo check"},',
            CHOICES,
            "}",
        ),
        '{"seq":2,"item":"permissionRequested","request_id":2,"tool_call":{"toolCallId":"t2","title":"Edit src/lib.rs"},"options":[]}',
    ]
    _ = model.updates(1, batch(items))
    assert_equal(len(model.waiting()), 2)
    var head = model.current()
    assert_equal(head.value().title, "Run cargo check")
    assert_true(model.key("2"))
    head = model.current()
    assert_equal(Int(head.value().request_id), 2)
    assert_true(model.key("escape"))
    assert_equal(len(model.waiting()), 0)
    var sent = model.take_outbox()
    assert_equal(len(sent), 2)
    assert_equal(sent[0].kind, EFFECT_PERMISSION)
    assert_equal(Int(sent[0].request), 1)
    assert_equal(sent[0].option.value(), "deny")
    assert_equal(sent[1].kind, EFFECT_PERMISSION)
    assert_equal(Int(sent[1].request), 2)
    assert_false(Bool(sent[1].option))
    assert_equal(model.respond(1, None), PANE_REFUSED_REQUEST)
    _ = model.updates(
        3,
        one(
            String(
                '{"seq":3,"item":"permissionRequested","request_id":3,"tool_call":{"toolCallId":"t3"},',
                CHOICES,
                "}",
            )
        ),
    )
    head = model.current()
    assert_equal(head.value().title, "Tool approval")
    assert_true(model.key("down"))
    assert_true(model.key("enter"))
    sent = model.take_outbox()
    assert_equal(len(sent), 1)
    assert_equal(Int(sent[0].request), 3)
    assert_equal(sent[0].option.value(), "deny")
    _ = model.updates(4, one('{"seq":4,"item":"permissionResolved","request_id":3}'))
    assert_equal(len(model.answered), 2)
    var rest: List[String] = [
        '{"seq":5,"item":"permissionResolved","request_id":1}',
        '{"seq":6,"item":"permissionResolved","request_id":2}',
    ]
    _ = model.updates(5, batch(rest))
    assert_equal(len(model.answered), 0)
    assert_equal(len(model.feed.permissions()), 0)


def questions() raises:
    var model = ready()
    _ = model.updates(1, one(asked(1, 5)))
    assert_true(model.has_card)
    assert_true(model.pick(0, 1))
    assert_false(model.has_card)
    var sent = model.take_outbox()
    assert_equal(len(sent), 1)
    assert_equal(sent[0].kind, EFFECT_ANSWER)
    assert_equal(Int(sent[0].request), 5)
    assert_equal(len(sent[0].answers), 1)
    assert_equal(sent[0].answers[0].question, "q0")
    assert_equal(len(sent[0].answers[0].answers), 1)
    assert_equal(sent[0].answers[0].answers[0], "two")
    _ = model.updates(2, one(asked(2, 6)))
    assert_true(model.pick(0, 2))
    assert_equal(model.other_target, 0)
    assert_equal(model.insert("three"), PANE_OK)
    assert_equal(model.draft, "")
    assert_true(model.key("enter"))
    sent = model.take_outbox()
    assert_equal(len(sent), 1)
    assert_equal(sent[0].kind, EFFECT_ANSWER)
    assert_equal(Int(sent[0].request), 6)
    assert_equal(sent[0].answers[0].answers[0], "three")
    _ = model.updates(3, one(asked(3, 7)))
    assert_true(model.key("escape"))
    sent = model.take_outbox()
    assert_equal(len(sent), 1)
    assert_equal(sent[0].kind, EFFECT_PERMISSION)
    assert_equal(Int(sent[0].request), 7)
    assert_false(Bool(sent[0].option))


def restores() raises:
    var model = ready(Optional[UInt64](7))
    _ = model.updates(
        1,
        one(
            '{"seq":1,"item":"promptsRestored","reclaim_id":3,"prompts":[{"owner":7,"text":"Zml4IHRoZSBidWlsZA=="},{"owner":8,"text":"b3RoZXI="}]}'
        ),
    )
    assert_equal(model.draft, "fix the build")
    assert_equal(model.cursor, 13)
    var sent = model.take_outbox()
    assert_equal(len(sent), 1)
    assert_equal(sent[0].kind, EFFECT_ACKNOWLEDGE)
    assert_equal(Int(sent[0].request), 3)


def replays() raises:
    var model = ready()
    var items: List[String] = [
        '{"seq":1,"item":"sessionReady"}',
        '{"seq":2,"item":"promptFinished","outcome":{"outcome":"completed","stop_reason":"end_turn"}}',
    ]
    var first = model.updates(1, batch(items))
    assert_equal(first.accepted, 2)
    var gap = model.updates(
        9, one('{"seq":9,"item":"permissionResolved","request_id":1}')
    )
    assert_equal(gap.accepted, 0)
    assert_equal(Int(gap.replay.value()), 2)
    var again = model.updates(
        10, one('{"seq":10,"item":"permissionResolved","request_id":1}')
    )
    assert_false(Bool(again.replay))
    var sent = model.take_outbox()
    assert_equal(len(sent), 1)
    assert_equal(sent[0].kind, EFFECT_REPLAY)
    assert_equal(Int(sent[0].request), 2)
    assert_true(model.lagged())
    sent = model.take_outbox()
    assert_equal(len(sent), 1)
    assert_equal(sent[0].kind, EFFECT_REPLAY)
    assert_equal(Int(sent[0].request), 2)


def bounds() raises:
    var model = ready()
    for _ in range(64):
        assert_equal(model.set_option("model", "x"), PANE_OK)
    assert_equal(model.set_option("model", "x"), PANE_REFUSED_OUTBOX)
    assert_equal(model.refused_effects, 1)
    assert_equal(len(model.take_outbox()), 64)
    var long = String()
    for _ in range(4097):
        long += "a"
    assert_equal(model.set_option("model", long), PANE_REFUSED_OPTION)
    var many = List[ChatImage]()
    for _ in range(65):
        var data: List[UInt8] = [1]
        many.append(ChatImage("image/png", data^))
    assert_equal(model.attach(many), PANE_REFUSED_IMAGES)
    assert_equal(model.error_line().value(), IMAGE_COUNT_ERROR)
    var gif = List[ChatImage]()
    var pixels: List[UInt8] = [1, 2, 3]
    gif.append(ChatImage("image/gif", pixels.copy()))
    assert_equal(model.attach(gif), PANE_REFUSED_IMAGES)
    assert_equal(model.error_line().value(), IMAGE_FORMAT_ERROR)
    var hollow = List[ChatImage]()
    hollow.append(ChatImage("image/png", List[UInt8]()))
    assert_equal(model.attach(hollow), PANE_REFUSED_IMAGES)
    assert_equal(model.error_line().value(), IMAGE_EMPTY_ERROR)
    var good = List[ChatImage]()
    good.append(ChatImage("image/png", pixels.copy()))
    assert_equal(model.attach(good), PANE_OK)
    assert_false(Bool(model.error_line()))
    assert_equal(model.submit(), PANE_OK)
    var sent = model.take_outbox()
    assert_equal(len(sent), 1)
    assert_equal(sent[0].kind, EFFECT_PROMPT)
    assert_equal(sent[0].text, "")
    assert_equal(len(sent[0].images), 1)
    assert_equal(len(sent[0].images[0].data), 3)
    assert_equal(len(model.attachments), 0)
    model.read_only = True
    _ = model.insert("x")
    assert_equal(model.submit(), PANE_REFUSED_READ_ONLY)
    assert_equal(len(model.outbox), 0)


def main() raises:
    prompts()
    running()
    editing()
    settings()
    completions()
    permissions()
    questions()
    restores()
    replays()
    bounds()
    print("agent model scenarios 10")
