from std.ffi import external_call
from std.os.path import isfile
from .cli_native import environment_value
from .client_files import _read_bytes, _last_error
from .desktop_key_config import (
    DesktopKeyConfig,
    desktop_key_config,
    MAX_DESKTOP_CONFIG_BYTES,
)


@fieldwise_init
struct DesktopConfigEnvironment(Copyable, Movable):
    var xdg: Optional[String]
    var home: Optional[String]
    var appdata: Optional[String]
    var local_appdata: Optional[String]
    var user_profile: Optional[String]


def _same_path(left: String, right: String) -> Bool:
    var first = List[String]()
    var second = List[String]()
    for part in left.split("/"):
        var value = String(part)
        if value.byte_length() != 0 and value != ".":
            first.append(value^)
    for part in right.split("/"):
        var value = String(part)
        if value.byte_length() != 0 and value != ".":
            second.append(value^)
    if len(first) != len(second):
        return False
    for index in range(len(first)):
        if first[index] != second[index]:
            return False
    return True


def _candidate(
    mut result: List[String], base: Optional[String], identity: String
):
    if not base or not base.value().startswith("/"):
        return
    var path = base.value().copy()
    if not path.endswith("/"):
        path += "/"
    path += identity + "/config"
    for current in result:
        if _same_path(current, path):
            return
    result.append(path^)


def _join(base: String, child: String) -> String:
    return (
        child.copy() if base.byte_length()
        == 0 else base + child if base.endswith("/") else base + "/" + child
    )


def _home_candidate(
    mut result: List[String], home: Optional[String], identity: String
):
    if home:
        var base = _join(home.value(), ".config")
        _candidate(result, base^, identity)


def desktop_config_candidates(
    platform: UInt8,
    environment: DesktopConfigEnvironment,
    identity: String = String("zz"),
) raises -> List[String]:
    if platform > 2 or identity != "zz" and identity != "zz-dev":
        raise Error("desktop configuration platform or identity is invalid")
    var total = 0
    for value in [
        environment.xdg,
        environment.home,
        environment.appdata,
        environment.local_appdata,
        environment.user_profile,
    ]:
        if value:
            if value.value().byte_length() > 16384 - total:
                raise Error(
                    "desktop configuration environment exceeds path budget"
                )
            total += value.value().byte_length()
            for byte in value.value().as_bytes():
                if byte == 0:
                    raise Error(
                        "desktop configuration environment contains NUL"
                    )
    var result = List[String]()
    _candidate(result, environment.xdg, identity)
    if platform == 2:
        _candidate(result, environment.appdata, identity)
        _candidate(result, environment.local_appdata, identity)
        _home_candidate(result, environment.user_profile, identity)
        _home_candidate(result, environment.home, identity)
    else:
        _home_candidate(result, environment.home, identity)
        if platform == 1 and environment.home:
            var base = _join(
                environment.home.value(), "Library/Application Support"
            )
            _candidate(result, base^, identity)
    return result^


def _environment(name: String) raises -> Optional[String]:
    var bytes = environment_value(name)
    if not bytes or len(bytes.value()) == 0:
        return None
    return String(from_utf8=Span(bytes.value()))


def local_desktop_config_environment() raises -> DesktopConfigEnvironment:
    return DesktopConfigEnvironment(
        _environment("XDG_CONFIG_HOME"),
        _environment("HOME"),
        _environment("APPDATA"),
        _environment("LOCALAPPDATA"),
        _environment("USERPROFILE"),
    )


def read_desktop_config(var path: String) raises -> String:
    if path.byte_length() > 16384:
        raise Error("desktop configuration path exceeds byte limit")
    for byte in path.as_bytes():
        if byte == 0:
            raise Error("desktop configuration path contains NUL")
    var fd = external_call["open", Int32, num_fixed_args=2](
        path.as_c_string_span().ptr(), Int32(0)
    )
    if fd < 0:
        raise Error(_last_error())
    try:
        var bytes = _read_bytes(fd, MAX_DESKTOP_CONFIG_BYTES, False)
        if len(bytes) > MAX_DESKTOP_CONFIG_BYTES:
            raise Error("configuration exceeds the 65536-byte limit")
        return String(from_utf8=Span(bytes))
    finally:
        _ = external_call["close", Int32](fd)
        _ = path


@fieldwise_init
struct DesktopLoadedConfig(Movable):
    var path: Optional[String]
    var source: String
    var value: DesktopKeyConfig


def load_desktop_key_config(
    environment: DesktopConfigEnvironment,
    macos: Bool,
    identity: String = String("zz"),
) raises -> DesktopLoadedConfig:
    var candidates = desktop_config_candidates(
        UInt8(1 if macos else 0), environment, identity
    )
    for path in candidates:
        if isfile(path):
            var text = read_desktop_config(path)
            var value = desktop_key_config(text, macos)
            return DesktopLoadedConfig(path.copy(), text^, value^)
    return DesktopLoadedConfig(None, String(), DesktopKeyConfig(macos))
