from std.math import ceil, floor, max, min


def _unsigned(value: Float32, limit: Int) -> Int:
    if value != value or value <= 0:
        return 0
    if value >= Float32(limit):
        return limit
    return Int(value)


def _physical(value: Float32) -> Int:
    var integral = floor(value)
    if value - integral >= 0.5:
        integral += 1
    return _unsigned(integral, 4294967295)


@fieldwise_init
struct PointerCell(TrivialRegisterPassable):
    var column: Int
    var row: Int
    var click_count: Int
    var rectangle: Bool
    var x: Int
    var y: Int
    var screen_width: Int
    var screen_height: Int
    var cell_width: Int
    var cell_height: Int


@fieldwise_init
struct PointerGrid(TrivialRegisterPassable):
    var left: Float32
    var top: Float32
    var width: Float32
    var height: Float32
    var cell_width: Float32
    var line_height: Float32
    var content_offset: Float32
    var scale: Float32
    var columns: Int
    var rows: Int

    def contains(self, x: Float32, y: Float32) -> Bool:
        return (
            x >= self.left
            and x <= self.left + self.width
            and y >= self.top
            and y <= self.top + self.height
        )

    def project(
        self, x: Float32, y: Float32, click_count: Int, rectangle: Bool
    ) -> PointerCell:
        var local_x = (x - self.left).clamp(0, self.width)
        var local_y = (y - self.top).clamp(0, self.height)
        var column = _unsigned(local_x / self.cell_width, 65535)
        var row = _unsigned(
            max(local_y - self.content_offset, Float32(0)) / self.line_height,
            65535,
        )
        return PointerCell(
            min(column, max(self.columns - 1, 0)),
            min(row, max(self.rows - 1, 0)),
            min(click_count, 3),
            rectangle,
            _physical(local_x * self.scale),
            _physical(local_y * self.scale),
            _physical(self.width * self.scale),
            _physical(self.height * self.scale),
            max(_physical(self.cell_width * self.scale), 1),
            max(_physical(self.line_height * self.scale), 1),
        )


def selection_autoscroll_lines(
    top: Float32, bottom: Float32, line_height: Float32, y: Float32
) -> Int:
    var distance: Float32
    if y < top:
        distance = -(top - y)
    elif y > bottom:
        distance = y - bottom
    else:
        return 0
    var rows = Int(
        (ceil(abs(distance) / max(line_height, Float32(1)))).clamp(1, 8)
    )
    return -rows if distance < 0 else rows


def scrollbar_strip_hit(
    right: Float32,
    top: Float32,
    bottom: Float32,
    gutter: Float32,
    total: Int,
    visible: Int,
    x: Float32,
    y: Float32,
) -> Bool:
    return (
        total > visible
        and x >= right - gutter
        and x <= right
        and y >= top
        and y <= bottom
    )


def scroll_fraction(top: Float32, height: Float32, y: Float32) -> UInt32:
    var fraction = ((y - top) / max(height, Float32(1))).clamp(0, 1)
    return UInt32(_physical(fraction * Float32(4294967295)))


def accumulate_scroll(mut remainder: Float32, delta: Float32) -> Int:
    remainder += delta
    var lines: Int
    if remainder != remainder:
        lines = 0
    elif remainder >= Float32(2147483647):
        lines = 2147483647
    elif remainder <= Float32(-2147483648):
        lines = -2147483648
    else:
        lines = Int(remainder)
    remainder -= Float32(lines)
    return lines


def cursor_should_blink(
    cursor: Optional[UInt64], policy: Int, focused: Bool
) -> Bool:
    if not focused:
        return False
    if cursor:
        var bits = cursor.value()
        if bits & (UInt64(1) << 58) == 0:
            return False
        if policy == 0:
            return False
        if policy == 1:
            return True
        return bits & (UInt64(1) << 59) != 0
    return False


def presented_uri(uri: String) -> String:
    var presented = String()
    var count = 0
    for codepoint in uri.codepoint_slices():
        if count == 240:
            presented += "…"
            break
        presented += String(codepoint)
        count += 1
    return presented^


def openable_uri(uri: String) -> Bool:
    return (
        uri.startswith("https://")
        or uri.startswith("http://")
        or uri.startswith("mailto:")
    )


def search_result_text(
    current: Int, total: Int, pending: Bool, invalid_pattern: Bool
) -> String:
    if invalid_pattern:
        return "Invalid pattern"
    if pending:
        return "Searching…"
    if total > 0:
        return String(t"{current} / {total}")
    return "0 matches"


def dead_label(dead: Bool, status: Optional[UInt32]) -> Optional[String]:
    if not dead:
        return None
    if status:
        var code = status.value()
        return String(t"Dead · {code}")
    return String("Dead")
