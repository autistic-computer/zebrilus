from .exec_protocol import Invocation
from .palette_protocol import validate_palette_command, PALETTE_COMMAND_BYTES


@fieldwise_init
struct NavTarget(TrivialRegisterPassable, Equatable):
    var kind: UInt8
    var id: UInt64

    def __eq__(self, other: Self) -> Bool:
        return self.kind == other.kind and self.id == other.id


@fieldwise_init
struct NavActivation(Copyable, Movable):
    var kind: UInt8
    var host: UInt32
    var session: Optional[UInt64]
    var target: String
    var commands: List[Invocation]


def nav_command(name: String, arguments: List[String]) raises -> Invocation:
    if name.byte_length() == 0 or name.byte_length() > PALETTE_COMMAND_BYTES or len(arguments) > 4096:
        raise Error("desktop navigation command exceeds its limits")
    var result = Invocation()
    result.name = name.copy()
    var size = name.byte_length()
    for argument in arguments:
        if argument.byte_length() > PALETTE_COMMAND_BYTES - size:
            raise Error("desktop navigation command exceeds byte limit")
        size += argument.byte_length()
        result.args.append(List[UInt8](argument.as_bytes()))
    validate_palette_command(result)
    return result^


def new_window_command(session: UInt64) raises -> Invocation:
    return nav_command("new-window", [String("-t"), String("$", session)])


def select_window_command(window: UInt64) raises -> Invocation:
    return nav_command("select-window", [String("-t"), String("@", window)])


def select_pane_commands(pane: UInt64) raises -> List[Invocation]:
    var arguments: List[String] = [String("-t"), String("%", pane)]
    var result = List[Invocation]()
    result.append(nav_command("select-window", arguments))
    result.append(nav_command("select-pane", arguments))
    return result^


def kill_target_command(target: NavTarget) raises -> Invocation:
    if target.kind > 2:
        raise Error("desktop navigation target kind is invalid")
    var name = String("kill-session" if target.kind == 0 else "kill-window" if target.kind == 1 else "kill-pane")
    var label = String("$" if target.kind == 0 else "@" if target.kind == 1 else "%", target.id)
    return nav_command(name, [String("-t"), label^])


def activation_for_target(host: UInt32, target: NavTarget, owner_session: Optional[UInt64], owner_window: Optional[UInt64], attached_host: UInt32, attached_session: Optional[UInt64], connected: Bool) raises -> Optional[NavActivation]:
    if target.kind > 2:
        raise Error("desktop navigation target kind is invalid")
    if not connected:
        return None
    if target.kind != 0 and owner_session and (host != attached_host or owner_session != attached_session):
        if not owner_window:
            return None
        var value = String("$", owner_session.value(), ":@", owner_window.value())
        if target.kind == 2:
            value += String(".%", target.id)
        return NavActivation(2, host, None, value^, List[Invocation]())
    if target.kind == 0:
        return NavActivation(1, host, target.id, String(), List[Invocation]())
    var commands = List[Invocation]()
    if target.kind == 1:
        commands.append(select_window_command(target.id))
    else:
        commands = select_pane_commands(target.id)
    return NavActivation(3, host, None, String(), commands^)


def new_session_commands(local: Bool, capabilities: List[String], working_directory: Optional[String]) raises -> List[Invocation]:
    var result = List[Invocation]()
    if not local:
        result.append(nav_command("new-session", [String("-d")]))
        return result^
    if len(capabilities) > 4096:
        raise Error("desktop navigation capability list exceeds count limit")
    var capability_bytes = 0
    for capability in capabilities:
        if capability.byte_length() > 65536 - capability_bytes:
            raise Error("desktop navigation capability list exceeds byte limit")
        capability_bytes += capability.byte_length()
    if working_directory and working_directory.value().byte_length() > PALETTE_COMMAND_BYTES - 13:
        raise Error("desktop navigation directory exceeds command byte limit")
    var arguments = List[String]()
    if working_directory:
        arguments.append(String("-c"))
        arguments.append(working_directory.value().copy())
    result.append(nav_command("new-session", arguments))
    var attached = False
    for capability in capabilities:
        attached = attached or capability == "new-session-attach-v1"
    if not attached:
        result.append(nav_command("attach-session", List[String]()))
    return result^


def detach_command() raises -> Invocation:
    return nav_command("detach-client", List[String]())


def picker_split_command(pane: UInt64, axis: UInt8) raises -> Invocation:
    if axis > 1:
        raise Error("desktop split axis is invalid")
    var arguments: List[String] = [String("--kind"), String("picker"), String("-h" if axis == 0 else "-v"), String("-t"), String("%", pane)]
    return nav_command("split-window", arguments)
