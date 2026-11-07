from std.sys import argv
from std.ffi import external_call
from std.os import getenv
from zzmojo.exec_protocol import decode_exec_exit
from zzmojo.stream_protocol import interactive_hello
from zzmojo.app_resume import ResumeStartup


def main() raises:
    var args = argv()
    if args[1] == "codec":
        var bytes: List[UInt8]
        with open(args[2], "r") as source:
            bytes = source.read_bytes()
        var decoded = decode_exec_exit(bytes^)
        var resume = decoded.take()
        var frame = interactive_hello(
            UInt64.MAX,
            UInt32.MAX,
            "ignored",
            options=(UInt64(1) << 21) - 1,
            viewport=False,
            native_search=args[3] == "1",
            prepared=resume.commands.copy(),
        )
        with open(args[4], "w") as target:
            target.write_all(Span(frame))
        return
    if args[1] != "--socket" or args[3] != "--resume-fds" or len(args) != 7:
        raise Error("invalid application resume probe invocation")
    var socket = Int32(Int(args[4]))
    var payload = Int32(Int(args[5]))
    var startup = ResumeStartup(socket, payload, Int32(Int(args[6])))
    if external_call["fcntl", Int32, num_fixed_args=2](payload, Int32(1)) != -1:
        raise Error("application startup retained its payload descriptor")
    if (
        external_call["fcntl", Int32, num_fixed_args=2](socket, Int32(1)) & 1
        == 0
    ):
        raise Error("application socket inheritance was not protected")
    var stream = startup^.connect()
    for _ in range(4096):
        if stream.model.view.session:
            break
        stream.consume(stream.transport.read_frame())
    if not stream.model.view.session:
        raise Error("application resume did not attach a session")
    var text = (
        String(stream.server_id)
        + " "
        + String(stream.client_id)
        + " "
        + String(stream.model.view.session.value())
        + " "
        + String(len(stream.model.snapshot.sessions))
    )
    var message = String()
    if stream.model.message:
        message = stream.model.message.value().text.copy()
    var view = String(Int(stream.model.view.read_only)) + "\n" + stream.model.view.client_flags
    with open(getenv("APP_VIEW_REPORT", ""), "w") as report:
        report.write_all(view.as_bytes())
    with open(getenv("APP_MESSAGE_REPORT", ""), "w") as report:
        report.write_all(message.as_bytes())
    var error_path = getenv("APP_NORMAL_ERROR_REPORT", "")
    if error_path.byte_length() > 0:
        var invalid: List[String] = ["-Q"]
        var response = stream.command("select-pane", Span(invalid))
        with open(error_path, "w") as report:
            report.write_all(response.error.as_bytes())
    with open(getenv("APP_RESUME_REPORT", ""), "w") as report:
        report.write_all(text.as_bytes())
