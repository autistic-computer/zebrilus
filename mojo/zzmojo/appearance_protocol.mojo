from .protocol import (
    Decoder,
    append_uint,
    append_text,
    envelope,
    validate_envelope,
)
from .tree_wire import read_bool, read_count, read_enum, read_text, read_uint
from std.sys.info import CompilationTarget


@fieldwise_init
struct FontFeature(TrivialRegisterPassable):
    var tag: UInt32
    var value: UInt32


def _float(mut reader: Decoder) raises -> Float32:
    var bits: UInt32 = 0
    for index in range(4):
        bits |= UInt32(reader.byte()) << UInt32(index * 8)
    return Pointer(to=bits).unsafe_bitcast[Float32]()[]


def _put_float(mut bytes: List[UInt8], value: Float32):
    var bits = Pointer(to=value).unsafe_bitcast[UInt32]()[]
    for shift in range(0, 32, 8):
        bytes.append(UInt8((bits >> UInt32(shift)) & 255))


def _rgb(mut reader: Decoder) raises -> UInt32:
    return (
        (UInt32(reader.byte()) << 16)
        | (UInt32(reader.byte()) << 8)
        | UInt32(reader.byte())
    )


def _rgba(mut reader: Decoder) raises -> UInt32:
    var rgb = _rgb(reader)
    return (rgb << 8) | UInt32(reader.byte())


def _put_rgb(mut bytes: List[UInt8], color: UInt32):
    for index in range(3):
        bytes.append(UInt8((color >> UInt32(16 - index * 8)) & 255))


def _put_rgba(mut bytes: List[UInt8], color: UInt32):
    _put_rgb(bytes, color >> 8)
    bytes.append(UInt8(color & 255))


def _validate_family(family: String) raises:
    var bytes = family.as_bytes()
    if len(bytes) == 0 or len(bytes) > 256:
        raise Error("invalid terminal font family size")
    for index in range(len(bytes)):
        if (
            bytes[index] < 32
            or bytes[index] == 127
            or (
                bytes[index] == 194
                and index + 1 < len(bytes)
                and bytes[index + 1] >= 128
                and bytes[index + 1] <= 159
            )
        ):
            raise Error("terminal font family contains control characters")


def _validate_families(families: List[String]) raises:
    if len(families) > 32:
        raise Error("too many terminal font families")
    for family in families:
        _validate_family(family)


def _families(mut reader: Decoder) raises -> List[String]:
    var count = read_count(reader, 32)
    var result = List[String](capacity=count)
    for _ in range(count):
        var family = read_text(reader, 256)
        _validate_family(family)
        result.append(family^)
    return result^


def _put_families(mut bytes: List[UInt8], families: List[String]):
    append_uint(bytes, UInt64(len(families)))
    for family in families:
        append_text(bytes, family)


def _range(value: Float32, low: Float32, high: Float32) -> Bool:
    return value >= low and value <= high


def _default_palette() -> List[UInt32]:
    var colors: List[UInt32] = [
        0x000000,
        0xCD0000,
        0x00CD00,
        0xCDCD00,
        0x0000EE,
        0xCD00CD,
        0x00CDCD,
        0xE5E5E5,
        0x7F7F7F,
        0xFF0000,
        0x00FF00,
        0xFFFF00,
        0x5C5CFF,
        0xFF00FF,
        0x00FFFF,
        0xFFFFFF,
    ]
    var levels: List[UInt32] = [0, 95, 135, 175, 215, 255]
    for red in levels:
        for green in levels:
            for blue in levels:
                colors.append((red << 16) | (green << 8) | blue)
    for index in range(24):
        var level = UInt32(8 + index * 10)
        colors.append((level << 16) | (level << 8) | level)
    return colors^


struct TerminalAppearance(Copyable, Movable):
    var color_scheme: UInt8
    var font_families: List[String]
    var font_families_bold: List[String]
    var font_families_italic: List[String]
    var font_families_bold_italic: List[String]
    var font_size_points: Float32
    var font_weight: UInt16
    var font_features: List[FontFeature]
    var font_synthetic_style: UInt8
    var font_thicken: Bool
    var font_thicken_strength: UInt8
    var cell_height_kind: UInt8
    var cell_height_adjustment: Float32
    var padding_left: Float32
    var padding_right: Float32
    var padding_top: Float32
    var padding_bottom: Float32
    var foreground: UInt32
    var background: UInt32
    var cursor_color: UInt32
    var cursor_style: UInt8
    var selection_foreground: UInt32
    var selection_background: UInt32
    var search_match_color: UInt32
    var search_current_color: UInt32
    var link_color: UInt32
    var copy_cursor_color: UInt32
    var palette: List[UInt32]
    var minimum_contrast: Float32
    var cursor_blink_policy: UInt8
    var cursor_blink_interval_ms: UInt32
    var rounded_selection: Bool
    var background_opacity: Float32

    def __init__(out self):
        self.color_scheme = 1
        self.font_families = List[String]()
        comptime if CompilationTarget.is_linux():
            self.font_families.append("Noto Sans Mono")
        else:
            self.font_families.append("Menlo")
        self.font_families_bold = List[String]()
        self.font_families_italic = List[String]()
        self.font_families_bold_italic = List[String]()
        self.font_size_points = 13
        self.font_weight = 400
        self.font_features = List[FontFeature]()
        self.font_synthetic_style = 7
        self.font_thicken = False
        self.font_thicken_strength = 255
        self.cell_height_kind = 0
        self.cell_height_adjustment = 0
        self.padding_left = 10
        self.padding_right = 10
        self.padding_top = 10
        self.padding_bottom = 10
        self.foreground = 0xD8DEE9
        self.background = 0x101318
        self.cursor_color = 0xE5C07B
        self.cursor_style = 1
        self.selection_foreground = 0xD8DEE9
        self.selection_background = 0x507DB894
        self.search_match_color = 0xE0AB386B
        self.search_current_color = 0xFAC247AD
        self.link_color = 0x4DA3EB
        self.copy_cursor_color = 0xB8C7E06B
        self.palette = _default_palette()
        self.minimum_contrast = 4.5
        self.cursor_blink_policy = 2
        self.cursor_blink_interval_ms = 500
        self.rounded_selection = True
        self.background_opacity = 1

    def validate(self) raises:
        if (
            self.color_scheme > 1
            or self.cell_height_kind > 2
            or self.cursor_style > 3
            or self.cursor_blink_policy > 2
            or self.font_synthetic_style > 7
        ):
            raise Error("unknown terminal appearance enum value")
        _validate_families(self.font_families)
        _validate_families(self.font_families_bold)
        _validate_families(self.font_families_italic)
        _validate_families(self.font_families_bold_italic)
        if len(self.font_features) > 64:
            raise Error("too many terminal font features")
        for feature in self.font_features:
            for index in range(4):
                var byte = UInt8((feature.tag >> UInt32(index * 8)) & 255)
                if not (
                    (byte >= 48 and byte <= 57)
                    or (byte >= 65 and byte <= 90)
                    or (byte >= 97 and byte <= 122)
                    or byte == 32
                ):
                    raise Error("invalid terminal font feature tag")
        if len(self.palette) != 256:
            raise Error("terminal palette must contain 256 colors")
        if (
            len(self.font_families) == 0
            or not _range(self.font_size_points, 1, 256)
            or self.font_weight < 1
            or self.font_weight > 1000
        ):
            raise Error("invalid terminal font settings")
        if self.cell_height_kind != 0 and not _range(
            self.cell_height_adjustment, -1024, 1024
        ):
            raise Error("invalid terminal cell height adjustment")
        if (
            not _range(self.padding_left, 0, 1024)
            or not _range(self.padding_right, 0, 1024)
            or not _range(self.padding_top, 0, 1024)
            or not _range(self.padding_bottom, 0, 1024)
        ):
            raise Error("invalid terminal padding")
        if (
            not _range(self.minimum_contrast, 1, 21)
            or self.cursor_blink_interval_ms < 50
            or self.cursor_blink_interval_ms > 60000
            or not _range(self.background_opacity, 0, 1)
        ):
            raise Error("invalid terminal contrast, blink interval or opacity")


def read_appearance(mut reader: Decoder) raises -> TerminalAppearance:
    var value = TerminalAppearance()
    value.color_scheme = read_enum(reader, 1)
    value.font_families = _families(reader)
    value.font_families_bold = _families(reader)
    value.font_families_italic = _families(reader)
    value.font_families_bold_italic = _families(reader)
    value.font_size_points = _float(reader)
    value.font_weight = UInt16(read_uint(reader, 65535))
    var features = read_count(reader, 64)
    for _ in range(features):
        var tag: UInt32 = 0
        for index in range(4):
            var byte = reader.byte()
            if not (
                (byte >= 48 and byte <= 57)
                or (byte >= 65 and byte <= 90)
                or (byte >= 97 and byte <= 122)
                or byte == 32
            ):
                raise Error("invalid terminal font feature tag")
            tag |= UInt32(byte) << UInt32(index * 8)
        value.font_features.append(
            FontFeature(tag, UInt32(read_uint(reader, 4294967295)))
        )
    value.font_synthetic_style = 0
    for index in range(3):
        if read_bool(reader):
            value.font_synthetic_style |= UInt8(1 << index)
    value.font_thicken = read_bool(reader)
    value.font_thicken_strength = reader.byte()
    value.cell_height_kind = read_enum(reader, 2)
    if value.cell_height_kind != 0:
        value.cell_height_adjustment = _float(reader)
    value.padding_left = _float(reader)
    value.padding_right = _float(reader)
    value.padding_top = _float(reader)
    value.padding_bottom = _float(reader)
    value.foreground = _rgb(reader)
    value.background = _rgb(reader)
    value.cursor_color = _rgb(reader)
    value.cursor_style = read_enum(reader, 3)
    value.selection_foreground = _rgb(reader)
    value.selection_background = _rgba(reader)
    value.search_match_color = _rgba(reader)
    value.search_current_color = _rgba(reader)
    value.link_color = _rgb(reader)
    value.copy_cursor_color = _rgba(reader)
    value.palette.clear()
    for _ in range(256):
        value.palette.append(_rgb(reader))
    value.minimum_contrast = _float(reader)
    value.cursor_blink_policy = read_enum(reader, 2)
    value.cursor_blink_interval_ms = UInt32(read_uint(reader, 4294967295))
    value.rounded_selection = read_bool(reader)
    value.background_opacity = _float(reader)
    value.validate()
    return value^


def write_appearance(mut bytes: List[UInt8], value: TerminalAppearance) raises:
    value.validate()
    append_uint(bytes, UInt64(value.color_scheme))
    _put_families(bytes, value.font_families)
    _put_families(bytes, value.font_families_bold)
    _put_families(bytes, value.font_families_italic)
    _put_families(bytes, value.font_families_bold_italic)
    _put_float(bytes, value.font_size_points)
    append_uint(bytes, UInt64(value.font_weight))
    append_uint(bytes, UInt64(len(value.font_features)))
    for feature in value.font_features:
        for index in range(4):
            bytes.append(UInt8((feature.tag >> UInt32(index * 8)) & 255))
        append_uint(bytes, UInt64(feature.value))
    for index in range(3):
        bytes.append(UInt8((value.font_synthetic_style >> UInt8(index)) & 1))
    bytes.append(UInt8(value.font_thicken))
    bytes.append(value.font_thicken_strength)
    append_uint(bytes, UInt64(value.cell_height_kind))
    if value.cell_height_kind != 0:
        _put_float(bytes, value.cell_height_adjustment)
    _put_float(bytes, value.padding_left)
    _put_float(bytes, value.padding_right)
    _put_float(bytes, value.padding_top)
    _put_float(bytes, value.padding_bottom)
    _put_rgb(bytes, value.foreground)
    _put_rgb(bytes, value.background)
    _put_rgb(bytes, value.cursor_color)
    append_uint(bytes, UInt64(value.cursor_style))
    _put_rgb(bytes, value.selection_foreground)
    _put_rgba(bytes, value.selection_background)
    _put_rgba(bytes, value.search_match_color)
    _put_rgba(bytes, value.search_current_color)
    _put_rgb(bytes, value.link_color)
    _put_rgba(bytes, value.copy_cursor_color)
    if len(value.palette) != 256:
        raise Error("terminal palette must contain 256 colors")
    for color in value.palette:
        _put_rgb(bytes, color)
    _put_float(bytes, value.minimum_contrast)
    append_uint(bytes, UInt64(value.cursor_blink_policy))
    append_uint(bytes, UInt64(value.cursor_blink_interval_ms))
    bytes.append(UInt8(value.rounded_selection))
    _put_float(bytes, value.background_opacity)


@fieldwise_init
struct AppearanceUpdate(Copyable, Movable):
    var sequence: UInt64
    var appearance: TerminalAppearance
    var provenance: Dict[UInt8, UInt8]


def decode_appearance(
    var bytes: List[UInt8],
) raises -> Optional[AppearanceUpdate]:
    if validate_envelope(Span(bytes)) != 0:
        return None
    var reader = Decoder(bytes^)
    if read_uint(reader, 4294967295) != 10:
        return None
    var sequence = reader.uint()
    if read_uint(reader, 4294967295) != 1:
        return None
    var appearance = read_appearance(reader)
    var count = read_count(reader)
    var provenance = Dict[UInt8, UInt8]()
    for _ in range(count):
        var key = read_enum(reader, 29)
        provenance[key] = read_enum(reader, 3)
    if len(provenance) != 30:
        raise Error("appearance provenance must contain every supported key")
    return AppearanceUpdate(sequence, appearance^, provenance^)


def encode_appearance(value: AppearanceUpdate) raises -> List[UInt8]:
    var bytes: List[UInt8] = [10]
    append_uint(bytes, value.sequence)
    append_uint(bytes, 1)
    write_appearance(bytes, value.appearance)
    if len(value.provenance) != 30:
        raise Error("appearance provenance must contain every supported key")
    var keys = List[UInt8]()
    for key in value.provenance:
        keys.append(key)
    sort(Span(keys))
    append_uint(bytes, UInt64(len(keys)))
    for key in keys:
        if key > 29 or value.provenance[key] > 3:
            raise Error("unknown appearance provenance key or source")
        append_uint(bytes, UInt64(key))
        append_uint(bytes, UInt64(value.provenance[key]))
    return envelope(Span(bytes))
