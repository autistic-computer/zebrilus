from .protocol import (
    Decoder,
    append_uint,
    append_text,
    envelope,
    validate_envelope,
)
from .tree_model import TmuxColour
from .tree_wire import (
    read_uint,
    read_enum,
    read_count,
    read_text,
    read_bool,
    read_colour,
    read_optional_id,
)
from .terminal_model import TerminalViewport, TerminalUpdate
from .terminal_wire import decode_viewport_bytes


@fieldwise_init
struct ModePresentation(Copyable, Movable):
    var pane: UInt64
    var view: Bool
    var position: String
    var position_style: String
    var selection_style: String
    var vi_keys: Bool
    var match_style: String
    var current_match_style: String
    var line_numbers: UInt8
    var line_number_style: String
    var current_line_number_style: String
    var current_line_style: String


@fieldwise_init
struct PaneBorderPresentation(Copyable, Movable):
    var pane: UInt64
    var style: String
    var window_style: String
    var window_active_style: String


struct StatusLine(Copyable, Movable):
    var left: String
    var right: String
    var title: String
    var base_style: String
    var rows: List[String]
    var position: UInt8
    var message_line: UInt8
    var customized: Bool
    var theme: List[TmuxColour]
    var message_style: String
    var message_command_style: String
    var modes: List[ModePresentation]
    var pane_borders: List[PaneBorderPresentation]

    def __init__(out self):
        self.left = String()
        self.right = String()
        self.title = String()
        self.base_style = String()
        self.rows = List[String]()
        self.position = 1
        self.message_line = 0
        self.customized = False
        self.theme = List[TmuxColour]()
        var colours: Array[UInt32, 10] = [
            0x0D0D0D,
            0xE5E5E5,
            0xB3B3B3,
            0x262626,
            0x9ACD32,
            0xB8860B,
            0xCD5C5C,
            0x6CA6CD,
            0x5F9EA0,
            0x9370DB,
        ]
        for value in colours:
            var colour = TmuxColour()
            colour.kind = 2
            colour.value = value
            self.theme.append(colour)
        self.message_style = String()
        self.message_command_style = String()
        self.modes = List[ModePresentation]()
        self.pane_borders = List[PaneBorderPresentation]()


@fieldwise_init
struct ClientMessage(Copyable, Movable):
    var pane: Optional[UInt64]
    var kind: UInt8
    var text: String
    var duration_ms: Optional[UInt32]
    var message_id: Optional[UInt64]


@fieldwise_init
struct TerminalNegotiation(Copyable, Movable):
    var features: List[String]
    var user_keys: List[String]
    var application_escape: List[String]


struct ClientEvent(Copyable, Movable):
    var kind: UInt8
    var sequence: UInt64
    var pane: UInt64
    var status: Optional[StatusLine]
    var message: Optional[ClientMessage]
    var message_id: UInt64
    var negotiation: Optional[TerminalNegotiation]
    var output_id: UInt64
    var viewport: Optional[TerminalViewport]

    def __init__(out self):
        self.kind = 0
        self.sequence = 0
        self.pane = 0
        self.status = None
        self.message = None
        self.message_id = 0
        self.negotiation = None
        self.output_id = 0
        self.viewport = None


def _read_strings(
    mut reader: Decoder, maximum: Int = 67108864, size: Int = 67108864
) raises -> List[String]:
    var count = read_count(reader, maximum)
    var output = List[String](capacity=count)
    for _ in range(count):
        output.append(read_text(reader, size))
    return output^


def read_status(mut reader: Decoder) raises -> StatusLine:
    var status = StatusLine()
    status.left = read_text(reader, 4096)
    status.right = read_text(reader, 4096)
    status.title = read_text(reader, 4096)
    status.base_style = read_text(reader, 4096)
    status.rows = _read_strings(reader, 5, 4096)
    status.position = read_enum(reader, 1)
    status.message_line = reader.byte()
    status.customized = read_bool(reader)
    status.theme.clear()
    for _ in range(10):
        status.theme.append(read_colour(reader))
    status.message_style = read_text(reader, 4096)
    status.message_command_style = read_text(reader, 4096)
    var modes = read_count(reader, 2)
    for _ in range(modes):
        status.modes.append(
            ModePresentation(
                reader.uint(),
                read_bool(reader),
                read_text(reader, 4096),
                read_text(reader, 4096),
                read_text(reader, 4096),
                read_bool(reader),
                read_text(reader, 4096),
                read_text(reader, 4096),
                reader.byte(),
                read_text(reader, 4096),
                read_text(reader, 4096),
                read_text(reader, 4096),
            )
        )
    var borders = read_count(reader, 256)
    for _ in range(borders):
        status.pane_borders.append(
            PaneBorderPresentation(
                reader.uint(),
                read_text(reader, 4096),
                read_text(reader, 4096),
                read_text(reader, 4096),
            )
        )
    validate_status(status)
    return status^


def decode_client_event(var bytes: List[UInt8]) raises -> Optional[ClientEvent]:
    if validate_envelope(Span(bytes)) != 0:
        return None
    var reader = Decoder(bytes^)
    if read_enum(reader, 75) != 10:
        return None
    var event = ClientEvent()
    event.sequence = reader.uint()
    event.kind = read_enum(reader, 65)
    if event.kind == 3:
        event.status = read_status(reader)
    elif event.kind == 10:
        event.pane = reader.uint()
        event.output_id = reader.uint()
        if read_bool(reader):
            if event.output_id == 0:
                raise Error("command output viewport has a zero output ID")
            event.viewport = decode_viewport_bytes(reader.bytes())
    elif event.kind == 16 or event.kind == 33:
        var pane = read_optional_id(reader)
        var kind = read_enum(reader, 3)
        var text = read_text(reader)
        var duration: Optional[UInt32] = None
        var id: Optional[UInt64] = None
        if event.kind == 33:
            duration = UInt32(read_uint(reader, 4294967295))
            id = reader.uint()
        event.message = ClientMessage(pane, kind, text^, duration, id)
    elif event.kind == 17:
        event.pane = reader.uint()
    elif event.kind == 45:
        event.message_id = reader.uint()
    elif event.kind == 64:
        event.negotiation = TerminalNegotiation(
            _read_strings(reader), _read_strings(reader), _read_strings(reader)
        )
    else:
        return None
    return event^


def _append_bool(mut output: List[UInt8], value: Bool):
    output.append(UInt8(1 if value else 0))


def _append_colour(mut output: List[UInt8], colour: TmuxColour):
    append_uint(output, UInt64(colour.kind))
    if colour.kind == 0 or colour.kind == 1 or colour.kind == 5:
        output.append(UInt8(colour.value))
    elif colour.kind == 2:
        append_uint(output, UInt64(colour.value))


def _append_strings(mut output: List[UInt8], strings: List[String]):
    append_uint(output, UInt64(len(strings)))
    for string in strings:
        append_text(output, string)


def append_status(mut output: List[UInt8], status: StatusLine) raises:
    validate_status(status)
    append_text(output, status.left)
    append_text(output, status.right)
    append_text(output, status.title)
    append_text(output, status.base_style)
    _append_strings(output, status.rows)
    append_uint(output, UInt64(status.position))
    output.append(status.message_line)
    _append_bool(output, status.customized)
    for colour in status.theme:
        _append_colour(output, colour)
    append_text(output, status.message_style)
    append_text(output, status.message_command_style)
    append_uint(output, UInt64(len(status.modes)))
    for mode in status.modes:
        append_uint(output, mode.pane)
        _append_bool(output, mode.view)
        append_text(output, mode.position)
        append_text(output, mode.position_style)
        append_text(output, mode.selection_style)
        _append_bool(output, mode.vi_keys)
        append_text(output, mode.match_style)
        append_text(output, mode.current_match_style)
        output.append(mode.line_numbers)
        append_text(output, mode.line_number_style)
        append_text(output, mode.current_line_number_style)
        append_text(output, mode.current_line_style)
    append_uint(output, UInt64(len(status.pane_borders)))
    for border in status.pane_borders:
        append_uint(output, border.pane)
        append_text(output, border.style)
        append_text(output, border.window_style)
        append_text(output, border.window_active_style)


def encode_client_event(event: ClientEvent) raises -> List[UInt8]:
    var output: List[UInt8] = [10]
    append_uint(output, event.sequence)
    append_uint(output, UInt64(event.kind))
    if event.kind == 3 and event.status:
        append_status(output, event.status.value())
    elif event.kind == 10 and not event.viewport:
        append_uint(output, event.pane)
        append_uint(output, event.output_id)
        output.append(0)
    elif (event.kind == 16 or event.kind == 33) and event.message:
        ref message = event.message.value()
        _append_bool(output, Bool(message.pane))
        if message.pane:
            append_uint(output, message.pane.value())
        if message.kind > 3:
            raise Error("client message has an unknown kind")
        append_uint(output, UInt64(message.kind))
        append_text(output, message.text)
        if event.kind == 33:
            if not message.duration_ms or not message.message_id:
                raise Error(
                    "timed client message has no duration or identifier"
                )
            append_uint(output, UInt64(message.duration_ms.value()))
            append_uint(output, message.message_id.value())
    elif event.kind == 17:
        append_uint(output, event.pane)
    elif event.kind == 45:
        append_uint(output, event.message_id)
    elif event.kind == 64 and event.negotiation:
        ref negotiation = event.negotiation.value()
        _append_strings(output, negotiation.features)
        _append_strings(output, negotiation.user_keys)
        _append_strings(output, negotiation.application_escape)
    else:
        raise Error("unsupported client event encoder")
    return envelope(Span(output))


struct CommandOutputState(Copyable, Movable):
    var watermark: UInt64
    var id: Optional[UInt64]
    var pane: Optional[UInt64]
    var viewport: Optional[TerminalViewport]
    var revision: UInt64

    def __init__(out self):
        self.watermark = 0
        self.id = None
        self.pane = None
        self.viewport = None
        self.revision = 0

    def clear(mut self):
        self.id = None
        self.pane = None
        self.viewport = None
        self.revision += 1

    def apply(
        mut self, pane: UInt64, id: UInt64, viewport: Optional[TerminalViewport]
    ):
        if id == 0:
            if not viewport:
                self.clear()
            return
        if id < self.watermark:
            return
        if id > self.watermark or self.id == Optional[UInt64](id):
            self.watermark = id
            if viewport:
                self.id = id
                self.pane = pane
                self.viewport = viewport.copy()
                self.revision += 1
            else:
                self.clear()

    def apply_terminal(mut self, update: TerminalUpdate):
        if update.kind == 2 and update.output_id and update.viewport:
            self.apply(update.pane, update.output_id.value(), update.viewport)

    def remove_pane(mut self, pane: UInt64):
        if self.pane == Optional[UInt64](pane):
            self.clear()


@fieldwise_init
struct ClientNotice(Copyable, Movable):
    var message: Optional[ClientMessage]
    var cleared: Optional[UInt64]


struct ClientEventState(Copyable, Movable):
    var status: StatusLine
    var negotiation: Optional[TerminalNegotiation]
    var output: CommandOutputState

    def __init__(out self):
        self.status = StatusLine()
        self.negotiation = None
        self.output = CommandOutputState()

    def apply(mut self, event: ClientEvent) -> Optional[ClientNotice]:
        if event.kind == 3 and event.status:
            self.status = event.status.value().copy()
        elif event.kind == 64 and event.negotiation:
            self.negotiation = event.negotiation.copy()
        elif event.kind == 10:
            self.output.apply(event.pane, event.output_id, event.viewport)
        elif event.kind == 17:
            self.output.remove_pane(event.pane)
        elif event.kind == 16 or event.kind == 33:
            return ClientNotice(event.message.copy(), None)
        elif event.kind == 45:
            return ClientNotice(None, event.message_id)
        return None


def _unsigned(text: String, maximum: UInt64) -> Bool:
    var bytes = text.as_bytes()
    var start = 1 if len(bytes) > 0 and bytes[0] == 43 else 0
    if start == len(bytes):
        return False
    var value: UInt64 = 0
    for index in range(start, len(bytes)):
        var byte = bytes[index]
        if byte < 48 or byte > 57:
            return False
        var digit = UInt64(byte - 48)
        if value > (maximum - digit) // 10:
            return False
        value = value * 10 + digit
    return True


def _tail(value: String, start: Int, end: Int = -1) raises -> String:
    return String(
        from_utf8=value.as_bytes()[
            start : (value.byte_length() if end < 0 else end)
        ]
    )


def _percentage(value: String) raises -> Bool:
    var text = _tail(value, 0, value.byte_length() - 1) if value.endswith(
        "%"
    ) else value.copy()
    return _unsigned(text, 100)


def _valid_colour(value: String) raises -> Bool:
    if value.byte_length() == 0:
        return False
    if value.startswith("#"):
        if value.byte_length() != 7:
            return False
        for byte in value.as_bytes()[1:]:
            if not (
                (byte >= 48 and byte <= 57) or (byte >= 97 and byte <= 102)
            ):
                return False
        return True
    if value.startswith("colour") and _unsigned(_tail(value, 6), 255):
        return True
    if value.startswith("color") and _unsigned(_tail(value, 5), 255):
        return True
    var builtins = String(
        "|default|terminal|themeblack|themewhite|themelightgrey|themedarkgrey|themegreen|themeyellow|themered|themeblue|themecyan|thememagenta|black|red|green|yellow|blue|magenta|cyan|white|brightblack|brightred|brightgreen|brightyellow|brightblue|brightmagenta|brightcyan|brightwhite|0|1|2|3|4|5|6|7|90|91|92|93|94|95|96|97|"
    )
    if _listed(value, builtins):
        return True
    return String("\n", value, "\n") in String(_X11_NAMES)


def _listed(value: String, choices: String) -> Bool:
    if value.byte_length() == 0:
        return False
    for choice in choices.split("|"):
        if value == choice:
            return True
    return False


def _attributes(value: String) -> Bool:
    if value == "default" or value == "none":
        return True
    var accepted = String(
        "|acs|bright|bold|dim|underscore|blink|reverse|hidden|italics|strikethrough|double-underscore|curly-underscore|dotted-underscore|dashed-underscore|overline|"
    )
    for attribute in value.split("|"):
        if not _listed(String(attribute), accepted):
            return False
    return True


def _range(value: String) raises -> Bool:
    var bytes = value.as_bytes()
    var divider = -1
    for index in range(len(bytes)):
        if bytes[index] == 124:
            divider = index
            break
    var kind = _tail(value, 0, divider) if divider >= 0 else value.copy()
    if kind.byte_length() == 0:
        return False
    if divider < 0:
        return (
            kind != "control"
            and kind != "pane"
            and kind != "window"
            and kind != "session"
            and kind != "user"
        )
    var argument = _tail(value, divider + 1)
    if argument.byte_length() == 0 or kind == "left" or kind == "right":
        return False
    if kind == "control":
        return _unsigned(argument, 9)
    if kind == "pane" or kind == "session":
        var prefix = "%" if kind == "pane" else "$"
        return argument.startswith(prefix) and _unsigned(
            _tail(argument, 1), UInt64.MAX
        )
    if kind == "window":
        return _unsigned(argument, UInt64.MAX)
    return True


def _valid_token(value: String) raises -> Bool:
    if value.byte_length() > 255:
        return False
    var lower = _ascii_lower(value)
    var accepted = String(
        "|default|none|ignore|noignore|push-default|pop-default|set-default|nolist|norange|noalign|nolink|noattr|"
    )
    if _listed(lower, accepted):
        return True
    for prefix in ["fg=", "bg=", "us=", "fill="]:
        if lower.startswith(prefix):
            return _valid_colour(_tail(lower, prefix.byte_length()))
    if lower.startswith("align="):
        return _listed(_tail(lower, 6), "|left|centre|right|absolute-centre|")
    if lower.startswith("list="):
        return _listed(_tail(lower, 5), "|on|focus|left-marker|right-marker|")
    if lower.startswith("range="):
        return _range(_tail(lower, 6))
    if lower.startswith("dim="):
        return _percentage(_tail(lower, 4))
    if lower.startswith("width="):
        var width = _tail(lower, 6)
        return _percentage(
            _tail(width, 0, width.byte_length() - 1)
        ) if width.endswith("%") else _unsigned(width, 4294967295)
    if lower.startswith("pad="):
        return _unsigned(_tail(lower, 4), 4294967295)
    if lower.startswith("link="):
        return True
    return _attributes(_tail(lower, 2)) if lower.startswith(
        "no"
    ) else _attributes(lower)


def _ascii_lower(value: String) raises -> String:
    var bytes = List[UInt8](value.as_bytes())
    for index in range(len(bytes)):
        if bytes[index] >= 65 and bytes[index] <= 90:
            bytes[index] += 32
    return String(from_utf8=Span(bytes))


def valid_style(value: String) raises -> Bool:
    var bytes = value.as_bytes()
    var start = 0
    for index in range(len(bytes) + 1):
        if (
            index == len(bytes)
            or bytes[index] == 32
            or bytes[index] == 44
            or bytes[index] == 10
        ):
            if index > start and not _valid_token(_tail(value, start, index)):
                return False
            start = index + 1
    return True


def validate_status(status: StatusLine) raises:
    if (
        len(status.rows) > 5
        or len(status.modes) > 2
        or len(status.pane_borders) > 256
        or len(status.theme) != 10
        or status.position > 1
    ):
        raise Error("status collections or position exceed their wire bounds")
    if (len(status.rows) == 0 and status.message_line != 0) or (
        len(status.rows) > 0 and Int(status.message_line) >= len(status.rows)
    ):
        raise Error("status message line is outside its rows")
    for value in [
        status.left,
        status.right,
        status.title,
        status.base_style,
        status.message_style,
        status.message_command_style,
    ]:
        if value.byte_length() > 4096:
            raise Error("status text exceeds its wire byte limit")
    for row in status.rows:
        if row.byte_length() > 4096:
            raise Error("status row exceeds its wire byte limit")
    for style in [
        status.base_style,
        status.message_style,
        status.message_command_style,
    ]:
        if not valid_style(style):
            raise Error("status style does not parse")
    for colour in status.theme:
        if (
            colour.kind == 5
            or colour.kind > 5
            or (colour.kind == 2 and colour.value > 16777215)
            or ((colour.kind == 0 or colour.kind == 1) and colour.value > 255)
        ):
            raise Error("status theme contains an invalid or circular colour")
    for mode in status.modes:
        for text in [
            mode.position,
            mode.position_style,
            mode.selection_style,
            mode.match_style,
            mode.current_match_style,
            mode.line_number_style,
            mode.current_line_number_style,
            mode.current_line_style,
        ]:
            if text.byte_length() > 4096:
                raise Error("mode presentation exceeds its wire byte limit")
        for style in [
            mode.position_style,
            mode.selection_style,
            mode.match_style,
            mode.current_match_style,
        ]:
            if not valid_style(style):
                raise Error("mode presentation style does not parse")
    for border in status.pane_borders:
        for style in [
            border.style,
            border.window_style,
            border.window_active_style,
        ]:
            if style.byte_length() > 4096 or not valid_style(style):
                raise Error("pane border presentation style does not parse")


comptime _X11_NAMES = "\naliceblue\nantiquewhite\nantiquewhite1\nantiquewhite2\nantiquewhite3\nantiquewhite4\naqua\naquamarine\naquamarine1\naquamarine2\naquamarine3\naquamarine4\nazure\nazure1\nazure2\nazure3\nazure4\nbeige\nbisque\nbisque1\nbisque2\nbisque3\nbisque4\nblack\nblanchedalmond\nblue\nblue1\nblue2\nblue3\nblue4\nblueviolet\nbrown\nbrown1\nbrown2\nbrown3\nbrown4\nburlywood\nburlywood1\nburlywood2\nburlywood3\nburlywood4\ncadetblue\ncadetblue1\ncadetblue2\ncadetblue3\ncadetblue4\nchartreuse\nchartreuse1\nchartreuse2\nchartreuse3\nchartreuse4\nchocolate\nchocolate1\nchocolate2\nchocolate3\nchocolate4\ncoral\ncoral1\ncoral2\ncoral3\ncoral4\ncornflowerblue\ncornsilk\ncornsilk1\ncornsilk2\ncornsilk3\ncornsilk4\ncrimson\ncyan\ncyan1\ncyan2\ncyan3\ncyan4\ndarkblue\ndarkcyan\ndarkgoldenrod\ndarkgoldenrod1\ndarkgoldenrod2\ndarkgoldenrod3\ndarkgoldenrod4\ndarkgray\ndarkgreen\ndarkgrey\ndarkkhaki\ndarkmagenta\ndarkolivegreen\ndarkolivegreen1\ndarkolivegreen2\ndarkolivegreen3\ndarkolivegreen4\ndarkorange\ndarkorange1\ndarkorange2\ndarkorange3\ndarkorange4\ndarkorchid\ndarkorchid1\ndarkorchid2\ndarkorchid3\ndarkorchid4\ndarkred\ndarksalmon\ndarkseagreen\ndarkseagreen1\ndarkseagreen2\ndarkseagreen3\ndarkseagreen4\ndarkslateblue\ndarkslategray\ndarkslategray1\ndarkslategray2\ndarkslategray3\ndarkslategray4\ndarkslategrey\ndarkturquoise\ndarkviolet\ndeeppink\ndeeppink1\ndeeppink2\ndeeppink3\ndeeppink4\ndeepskyblue\ndeepskyblue1\ndeepskyblue2\ndeepskyblue3\ndeepskyblue4\ndimgray\ndimgrey\ndodgerblue\ndodgerblue1\ndodgerblue2\ndodgerblue3\ndodgerblue4\nfirebrick\nfirebrick1\nfirebrick2\nfirebrick3\nfirebrick4\nfloralwhite\nforestgreen\nfuchsia\ngainsboro\nghostwhite\ngold\ngold1\ngold2\ngold3\ngold4\ngoldenrod\ngoldenrod1\ngoldenrod2\ngoldenrod3\ngoldenrod4\ngray\ngray0\ngray1\ngray10\ngray100\ngray11\ngray12\ngray13\ngray14\ngray15\ngray16\ngray17\ngray18\ngray19\ngray2\ngray20\ngray21\ngray22\ngray23\ngray24\ngray25\ngray26\ngray27\ngray28\ngray29\ngray3\ngray30\ngray31\ngray32\ngray33\ngray34\ngray35\ngray36\ngray37\ngray38\ngray39\ngray4\ngray40\ngray41\ngray42\ngray43\ngray44\ngray45\ngray46\ngray47\ngray48\ngray49\ngray5\ngray50\ngray51\ngray52\ngray53\ngray54\ngray55\ngray56\ngray57\ngray58\ngray59\ngray6\ngray60\ngray61\ngray62\ngray63\ngray64\ngray65\ngray66\ngray67\ngray68\ngray69\ngray7\ngray70\ngray71\ngray72\ngray73\ngray74\ngray75\ngray76\ngray77\ngray78\ngray79\ngray8\ngray80\ngray81\ngray82\ngray83\ngray84\ngray85\ngray86\ngray87\ngray88\ngray89\ngray9\ngray90\ngray91\ngray92\ngray93\ngray94\ngray95\ngray96\ngray97\ngray98\ngray99\ngreen\ngreen1\ngreen2\ngreen3\ngreen4\ngreenyellow\ngrey\ngrey0\ngrey1\ngrey10\ngrey100\ngrey11\ngrey12\ngrey13\ngrey14\ngrey15\ngrey16\ngrey17\ngrey18\ngrey19\ngrey2\ngrey20\ngrey21\ngrey22\ngrey23\ngrey24\ngrey25\ngrey26\ngrey27\ngrey28\ngrey29\ngrey3\ngrey30\ngrey31\ngrey32\ngrey33\ngrey34\ngrey35\ngrey36\ngrey37\ngrey38\ngrey39\ngrey4\ngrey40\ngrey41\ngrey42\ngrey43\ngrey44\ngrey45\ngrey46\ngrey47\ngrey48\ngrey49\ngrey5\ngrey50\ngrey51\ngrey52\ngrey53\ngrey54\ngrey55\ngrey56\ngrey57\ngrey58\ngrey59\ngrey6\ngrey60\ngrey61\ngrey62\ngrey63\ngrey64\ngrey65\ngrey66\ngrey67\ngrey68\ngrey69\ngrey7\ngrey70\ngrey71\ngrey72\ngrey73\ngrey74\ngrey75\ngrey76\ngrey77\ngrey78\ngrey79\ngrey8\ngrey80\ngrey81\ngrey82\ngrey83\ngrey84\ngrey85\ngrey86\ngrey87\ngrey88\ngrey89\ngrey9\ngrey90\ngrey91\ngrey92\ngrey93\ngrey94\ngrey95\ngrey96\ngrey97\ngrey98\ngrey99\nhoneydew\nhoneydew1\nhoneydew2\nhoneydew3\nhoneydew4\nhotpink\nhotpink1\nhotpink2\nhotpink3\nhotpink4\nindianred\nindianred1\nindianred2\nindianred3\nindianred4\nindigo\nivory\nivory1\nivory2\nivory3\nivory4\nkhaki\nkhaki1\nkhaki2\nkhaki3\nkhaki4\nlavender\nlavenderblush\nlavenderblush1\nlavenderblush2\nlavenderblush3\nlavenderblush4\nlawngreen\nlemonchiffon\nlemonchiffon1\nlemonchiffon2\nlemonchiffon3\nlemonchiffon4\nlightblue\nlightblue1\nlightblue2\nlightblue3\nlightblue4\nlightcoral\nlightcyan\nlightcyan1\nlightcyan2\nlightcyan3\nlightcyan4\nlightgoldenrod\nlightgoldenrod1\nlightgoldenrod2\nlightgoldenrod3\nlightgoldenrod4\nlightgoldenrodyellow\nlightgray\nlightgreen\nlightgrey\nlightpink\nlightpink1\nlightpink2\nlightpink3\nlightpink4\nlightsalmon\nlightsalmon1\nlightsalmon2\nlightsalmon3\nlightsalmon4\nlightseagreen\nlightskyblue\nlightskyblue1\nlightskyblue2\nlightskyblue3\nlightskyblue4\nlightslateblue\nlightslategray\nlightslategrey\nlightsteelblue\nlightsteelblue1\nlightsteelblue2\nlightsteelblue3\nlightsteelblue4\nlightyellow\nlightyellow1\nlightyellow2\nlightyellow3\nlightyellow4\nlime\nlimegreen\nlinen\nmagenta\nmagenta1\nmagenta2\nmagenta3\nmagenta4\nmaroon\nmaroon1\nmaroon2\nmaroon3\nmaroon4\nmediumaquamarine\nmediumblue\nmediumorchid\nmediumorchid1\nmediumorchid2\nmediumorchid3\nmediumorchid4\nmediumpurple\nmediumpurple1\nmediumpurple2\nmediumpurple3\nmediumpurple4\nmediumseagreen\nmediumslateblue\nmediumspringgreen\nmediumturquoise\nmediumvioletred\nmidnightblue\nmintcream\nmistyrose\nmistyrose1\nmistyrose2\nmistyrose3\nmistyrose4\nmoccasin\nnavajowhite\nnavajowhite1\nnavajowhite2\nnavajowhite3\nnavajowhite4\nnavy\nnavyblue\noldlace\nolive\nolivedrab\nolivedrab1\nolivedrab2\nolivedrab3\nolivedrab4\norange\norange1\norange2\norange3\norange4\norangered\norangered1\norangered2\norangered3\norangered4\norchid\norchid1\norchid2\norchid3\norchid4\npalegoldenrod\npalegreen\npalegreen1\npalegreen2\npalegreen3\npalegreen4\npaleturquoise\npaleturquoise1\npaleturquoise2\npaleturquoise3\npaleturquoise4\npalevioletred\npalevioletred1\npalevioletred2\npalevioletred3\npalevioletred4\npapayawhip\npeachpuff\npeachpuff1\npeachpuff2\npeachpuff3\npeachpuff4\nperu\npink\npink1\npink2\npink3\npink4\nplum\nplum1\nplum2\nplum3\nplum4\npowderblue\npurple\npurple1\npurple2\npurple3\npurple4\nrebeccapurple\nred\nred1\nred2\nred3\nred4\nrosybrown\nrosybrown1\nrosybrown2\nrosybrown3\nrosybrown4\nroyalblue\nroyalblue1\nroyalblue2\nroyalblue3\nroyalblue4\nsaddlebrown\nsalmon\nsalmon1\nsalmon2\nsalmon3\nsalmon4\nsandybrown\nseagreen\nseagreen1\nseagreen2\nseagreen3\nseagreen4\nseashell\nseashell1\nseashell2\nseashell3\nseashell4\nsienna\nsienna1\nsienna2\nsienna3\nsienna4\nsilver\nskyblue\nskyblue1\nskyblue2\nskyblue3\nskyblue4\nslateblue\nslateblue1\nslateblue2\nslateblue3\nslateblue4\nslategray\nslategray1\nslategray2\nslategray3\nslategray4\nslategrey\nsnow\nsnow1\nsnow2\nsnow3\nsnow4\nspringgreen\nspringgreen1\nspringgreen2\nspringgreen3\nspringgreen4\nsteelblue\nsteelblue1\nsteelblue2\nsteelblue3\nsteelblue4\ntan\ntan1\ntan2\ntan3\ntan4\nteal\nthistle\nthistle1\nthistle2\nthistle3\nthistle4\ntomato\ntomato1\ntomato2\ntomato3\ntomato4\nturquoise\nturquoise1\nturquoise2\nturquoise3\nturquoise4\nviolet\nvioletred\nvioletred1\nvioletred2\nvioletred3\nvioletred4\nwebgray\nwebgreen\nwebgrey\nwebmaroon\nwebpurple\nwheat\nwheat1\nwheat2\nwheat3\nwheat4\nwhite\nwhitesmoke\nx11gray\nx11green\nx11grey\nx11maroon\nx11purple\nyellow\nyellow1\nyellow2\nyellow3\nyellow4\nyellowgreen\n"
