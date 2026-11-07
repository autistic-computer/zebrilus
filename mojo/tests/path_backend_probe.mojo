from std.ffi import external_call
from std.sys import argv
from zzmojo.backend import AsyncBackend, RECONNECT
from zzmojo.state import AppModel
from zzmojo.path_protocol import PATH_OPEN
from zzmojo.path_mailbox import PATH_CLOSE
from zzmojo.path_service import PathService, PathSource, PathCommand


def queue(mut backend: AsyncBackend, commands: Span[PathCommand, _]) raises:
    for command in commands:
        if not backend.path(command):
            raise Error("live path request lost its captured backend source")


def poll(mut backend: AsyncBackend, mut model: AppModel, mut service: PathService) raises -> Int:
    backend.submit(model, 0)
    _ = backend.collect(model, 0, True)
    if model.error != "":
        raise Error(model.error)
    var inbox = backend.paths.take()
    var notifications = 0
    for delivery in inbox.events:
        if delivery.message.kind == PATH_CLOSE:
            notifications |= 1
            if service.source.matches(delivery.source):
                var cancel = service.close()
                if cancel:
                    _ = backend.path(cancel.value())
        elif delivery.message.kind == PATH_OPEN:
            notifications |= 2
            var cancel = service.opened(delivery.source, delivery.message.pane.value(), delivery.message.dir.copy())
            if cancel:
                _ = backend.path(cancel.value())
        else:
            var effect = service.apply(delivery.source, delivery.message.copy())
            if effect.cancel:
                _ = backend.path(effect.cancel.value())
    return notifications


def complete(mut backend: AsyncBackend, mut model: AppModel, mut service: PathService) raises:
    for _ in range(256):
        _ = poll(backend, model, service)
        if not service.pending:
            return
        _ = external_call["usleep", Int32](UInt32(10_000))
    raise Error("live backend path mailbox did not complete")


def main() raises:
    var args = argv()
    var backend = AsyncBackend(args[1])
    var model = AppModel()
    var service = PathService()
    _ = poll(backend, model, service)
    if not backend.path_source or not model.pane:
        raise Error("live path backend did not attach to a terminal")
    var source = backend.path_source.value()
    var commands = service.set_directory(source, "here", String(args[2], "/"), model.pane)
    queue(backend, Span(commands))
    complete(backend, model, service)
    var count = 0
    for chunk in service.listing.value().chunks:
        count += len(chunk[].entries)
    if count != 5 or service.listing.value().error or len(backend.job[].connection.value().model.paths.registry.pending) != 0:
        raise Error("live backend did not publish the complete native listing exactly once")
    _ = poll(backend, model, service)
    if len(backend.paths.events) != 0:
        raise Error("path deliveries repeated across backend snapshots")
    commands = service.set_directory(source, "here", String(args[2], "-missing/"), model.pane)
    queue(backend, Span(commands))
    complete(backend, model, service)
    if not service.listing.value().error or not service.listing.value().is_empty():
        raise Error("live backend error reused a previous listing")
    var command_args: List[String] = ["-c", args[2].copy()]
    var response = backend.job[].connection.value().command("choose-path", Span(command_args))
    if response.exit_code != 0:
        raise Error("native choose-path failed: " + response.error)
    var opened = False
    for _ in range(32):
        opened = opened or poll(backend, model, service) & 2 != 0
        if opened:
            break
        _ = external_call["usleep", Int32](UInt32(10_000))
    if not opened or service.insert_pane != model.pane or service.start_dir.value() != args[2]:
        raise Error("native path-picker notification lost its insertion context")
    var insert = service.insert_pane
    commands = service.set_directory(source, "here", String(args[2], "/"), insert)
    queue(backend, Span(commands))
    complete(backend, model, service)
    command_args = ["-d", "-s", "path-second", "sleep 120"]
    response = backend.job[].connection.value().command("new-session", Span(command_args))
    if response.exit_code != 0:
        raise Error("private path context session creation failed")
    command_args = ["-t", "path-second"]
    response = backend.job[].connection.value().command("switch-client", Span(command_args))
    if response.exit_code != 0:
        raise Error("private path context switch failed")
    var closed = False
    for _ in range(32):
        closed = closed or poll(backend, model, service) & 1 != 0
        if closed:
            break
        _ = external_call["usleep", Int32](UInt32(10_000))
    if not closed or service.listing or service.insert_pane:
        raise Error("native attachment reset did not close retained path state")
    commands = service.set_directory(source, "here", String(args[2], "/"), model.pane)
    backend.enqueue(RECONNECT)
    queue(backend, Span(commands))
    _ = poll(backend, model, service)
    if not backend.path_source or backend.path_source.value().matches(source) or backend.path_source.value().epoch == source.epoch or service.pending or len(backend.job[].connection.value().model.paths.registry.pending) != 0:
        raise Error("path request survived reconnect provenance checks")
    if backend.path(commands[0]):
        raise Error("stale path command was accepted after reconnect")
    print("Private path backend: typed mailbox, exact-once delivery, picker notifications, attachment reset and reconnect ownership passed")
