from std.sys import argv
from std.ffi import external_call
from std.testing import assert_true, assert_false, assert_equal
from zzmojo.backend import AsyncBackend
from zzmojo.state import AppModel
from zzmojo.exec_protocol import Invocation
from zzmojo.input_protocol import KeyInput, input_key


def pause():
    _ = external_call["usleep", Int32](UInt32(10_000))


def poll(mut backend: AsyncBackend, mut model: AppModel) raises:
    backend.submit(model, 0)
    _ = backend.collect(model, 0, True)


def command(name: String, args: List[String]) -> Invocation:
    var result = Invocation()
    result.name = name
    for arg in args:
        result.args.append(List[UInt8](arg.as_bytes()))
    return result^


def send(mut backend: AsyncBackend, model: AppModel, value: Invocation) raises:
    backend.palette_command(
        value, 0, (model.source_server, model.source_client)
    )


def main() raises:
    var args = argv()
    var backend = AsyncBackend(args[1])
    var model = AppModel()
    for _ in range(200):
        poll(backend, model)
        if model.pane and model.pane.value() in model.terminals:
            break
        pause()
    assert_true(Bool(model.pane))
    var source = (model.source_server, model.source_client)
    var pane = model.pane.value()
    var split: List[String] = ["-h", "-t", "%" + String(pane)]
    send(backend, model, command("split-window", split^))
    for _ in range(200):
        poll(backend, model)
        if len(model.panes) == 2:
            break
        pause()
    assert_equal(len(model.panes), 2)
    assert_false(Bool(model.command_output.viewport))
    var rename: List[String] = [
        "-t",
        "@" + String(model.window.value()),
        "queued ' λ name",
    ]
    send(backend, model, command("rename-window", rename^))
    for _ in range(200):
        poll(backend, model)
        if model.windows[0].name == "queued ' λ name":
            break
        pause()
    assert_equal(model.windows[0].name, "queued ' λ name")
    var raw: List[String] = ["-F", "1", "rename-window 'native daemon parser'"]
    send(backend, model, command("if-shell", raw^))
    for _ in range(200):
        poll(backend, model)
        if model.windows[0].name == "native daemon parser":
            break
        pause()
    assert_equal(model.windows[0].name, "native daemon parser")
    var stale: List[String] = ["STALE_MUST_NOT_EXECUTE"]
    backend.palette_command(
        command("rename-window", stale^), 0, (source[0], source[1] + 1)
    )
    poll(backend, model)
    assert_equal(model.windows[0].name, "native daemon parser")
    var key = KeyInput()
    key.key = 0
    key.codepoint = 98
    key.modifiers = 2
    key.unshifted = UInt32(98)
    backend.input(model.pane.value(), input_key(model.pane.value(), key), 0)
    for _ in range(200):
        poll(backend, model)
        if model.routing.armed:
            break
        pause()
    assert_true(model.routing.armed)
    backend.palette_open(0, source)
    for _ in range(200):
        poll(backend, model)
        if not model.routing.armed and model.routing.cancelled_request > 0:
            break
        pause()
    assert_false(model.routing.armed)
    assert_true(model.routing.cancelled_request > 0)
    var prompt: List[String] = ["-p", "palette proof", "display-message %%"]
    send(backend, model, command("command-prompt", prompt^))
    for _ in range(200):
        poll(backend, model)
        if model.dialogs.prompt:
            break
        pause()
    assert_true(Bool(model.dialogs.prompt))
    backend.palette_open(0, source)
    for _ in range(200):
        poll(backend, model)
        if not model.dialogs.prompt:
            break
        pause()
    assert_false(Bool(model.dialogs.prompt))
    send(backend, model, command("choose-tree", List[String]()))
    for _ in range(200):
        poll(backend, model)
        if model.dialogs.chooser:
            break
        pause()
    assert_true(Bool(model.dialogs.chooser))
    backend.palette_open(0, source)
    for _ in range(200):
        poll(backend, model)
        if not model.dialogs.chooser:
            break
        pause()
    assert_false(Bool(model.dialogs.chooser))
    assert_true(Bool(backend.job[].connection))
    assert_equal(model.source_client, source[1])
    assert_equal(model.error, "")
    var session = model.session.value()
    send(backend, model, command("detach-client", List[String]()))
    for _ in range(200):
        poll(backend, model)
        if not model.session and model.attachment_blocked:
            break
        pause()
    assert_false(Bool(model.session))
    assert_false(Bool(model.pane))
    assert_true(model.attachment_blocked)
    assert_true(Bool(model.detached))
    assert_equal(model.detached.value().session, session)
    assert_equal(len(model.sessions), 1)
    for _ in range(20):
        poll(backend, model)
        assert_false(Bool(model.session))
        assert_false(Bool(backend.job[].connection.value().model.view.session))
    model.select_session(session)
    for _ in range(200):
        poll(backend, model)
        if (
            model.session == Optional[UInt64](session)
            and not model.attachment_blocked
            and model.pane
            and model.pane.value() in model.terminals
        ):
            break
        pause()
    assert_equal(model.session, Optional[UInt64](session))
    assert_false(model.attachment_blocked)
    assert_true(Bool(model.pane))
    assert_true(model.pane.value() in model.terminals)
    assert_equal(model.source_client, source[1])
    send(backend, model, command("palette-no-such-command", List[String]()))
    poll(backend, model)
    assert_true(model.error.byte_length() > 0)
    assert_true(Bool(backend.job[].connection))
    assert_equal(model.source_client, source[1])
    print(
        "palette worker: typed split/rename/raw execution, stale-source"
        " refusal, error liveness and native prefix/dialog controls passed"
    )
