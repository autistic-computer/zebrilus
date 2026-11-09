comptime JSON_NULL: UInt8 = 0
comptime JSON_FALSE: UInt8 = 1
comptime JSON_TRUE: UInt8 = 2
comptime JSON_UNSIGNED: UInt8 = 3
comptime JSON_SIGNED: UInt8 = 4
comptime JSON_FLOAT: UInt8 = 5
comptime JSON_STRING: UInt8 = 6
comptime JSON_ARRAY: UInt8 = 7
comptime JSON_OBJECT: UInt8 = 8

comptime JSON_OK = 0
comptime JSON_REFUSED_BYTES = 1
comptime JSON_REFUSED_DEPTH = 2
comptime JSON_REFUSED_NODES = 3
comptime JSON_REFUSED_SYNTAX = 4

comptime JSON_MAX_DEPTH = 128
comptime JSON_MAX_BYTES = 9 * 1024 * 1024
comptime JSON_MAX_NODES = 262144
comptime JSON_ABSENT = -1
comptime JSON_DUPLICATE = -2


@fieldwise_init
struct JsonNode(Copyable, Movable):
    var kind: UInt8
    var key: String
    var text: String
    var magnitude: UInt64
    var first: Int
    var next: Int
    var count: Int


struct Json(Movable):
    var nodes: List[JsonNode]
    var refusal: Int
    var failed: Bool
    var limit: Int
    var at: Int

    def __init__(out self, limit: Int = JSON_MAX_NODES):
        self.nodes = List[JsonNode]()
        self.refusal = JSON_OK
        self.failed = False
        self.limit = limit
        self.at = 0

    def ok(self) -> Bool:
        return self.refusal == JSON_OK

    def kind(self, node: Int) -> UInt8:
        return self.nodes[node].kind

    def text(self, node: Int) -> String:
        return self.nodes[node].text

    def count(self, node: Int) -> Int:
        return self.nodes[node].count

    def children(self, node: Int) -> List[Int]:
        var found = List[Int]()
        var child = self.nodes[node].first
        while child >= 0:
            found.append(child)
            child = self.nodes[child].next
        return found^

    def member(self, node: Int, key: String) -> Int:
        if node < 0 or self.nodes[node].kind != JSON_OBJECT:
            return JSON_ABSENT
        var found = JSON_ABSENT
        var child = self.nodes[node].first
        while child >= 0:
            if self.nodes[child].key == key:
                if found != JSON_ABSENT:
                    return JSON_DUPLICATE
                found = child
            child = self.nodes[child].next
        return found

    def last(self, node: Int, key: String) -> Int:
        if node < 0 or self.nodes[node].kind != JSON_OBJECT:
            return JSON_ABSENT
        var found = JSON_ABSENT
        var child = self.nodes[node].first
        while child >= 0:
            if self.nodes[child].key == key:
                found = child
            child = self.nodes[child].next
        return found

    def last_text(self, node: Int, key: String) -> Optional[String]:
        var found = self.last(node, key)
        if found < 0 or self.nodes[found].kind != JSON_STRING:
            return None
        return Optional[String](self.nodes[found].text.copy())

    def fail(mut self) -> Int:
        self.failed = True
        return JSON_ABSENT

    def field(mut self, node: Int, key: String, required: Bool) -> Int:
        if node < 0 or self.nodes[node].kind != JSON_OBJECT:
            return self.fail()
        var found = self.member(node, key)
        if found == JSON_DUPLICATE or (found == JSON_ABSENT and required):
            return self.fail()
        return found

    def need(mut self, node: Int, key: String, kind: UInt8) -> Int:
        var found = self.field(node, key, True)
        if found >= 0 and self.nodes[found].kind != kind:
            return self.fail()
        return found

    def maybe(mut self, node: Int, key: String, kind: UInt8) -> Int:
        var found = self.field(node, key, False)
        if found < 0 or self.nodes[found].kind == JSON_NULL:
            return JSON_ABSENT
        if self.nodes[found].kind != kind:
            return self.fail()
        return found

    def defaulted(mut self, node: Int, key: String, kind: UInt8) -> Int:
        var found = self.field(node, key, False)
        if found >= 0 and self.nodes[found].kind != kind:
            return self.fail()
        return found

    def req_text(mut self, node: Int, key: String) -> String:
        var found = self.need(node, key, JSON_STRING)
        return self.nodes[found].text if found >= 0 else String()

    def def_text(mut self, node: Int, key: String) -> String:
        var found = self.defaulted(node, key, JSON_STRING)
        return self.nodes[found].text if found >= 0 else String()

    def opt_text(mut self, node: Int, key: String) -> Optional[String]:
        var found = self.maybe(node, key, JSON_STRING)
        if found < 0:
            return None
        return Optional[String](self.nodes[found].text)

    def req_unsigned(mut self, node: Int, key: String) -> UInt64:
        var found = self.need(node, key, JSON_UNSIGNED)
        return self.nodes[found].magnitude if found >= 0 else 0

    def opt_unsigned(mut self, node: Int, key: String) -> Optional[UInt64]:
        var found = self.maybe(node, key, JSON_UNSIGNED)
        if found < 0:
            return None
        return Optional[UInt64](self.nodes[found].magnitude)

    def integer(mut self, node: Int, low: Int, high: Int) -> Int:
        if node < 0:
            return 0
        var kind = self.nodes[node].kind
        var magnitude = self.nodes[node].magnitude
        if kind == JSON_UNSIGNED and magnitude <= UInt64(high):
            return Int(magnitude)
        if kind == JSON_SIGNED and magnitude <= UInt64(-low):
            return -Int(magnitude)
        self.failed = True
        return 0

    def opt_integer(
        mut self, node: Int, key: String, low: Int, high: Int
    ) -> Optional[Int]:
        var found = self.field(node, key, False)
        if found < 0 or self.nodes[found].kind == JSON_NULL:
            return None
        var value = self.integer(found, low, high)
        if self.failed:
            return None
        return Optional[Int](value)

    def boolean(mut self, found: Int) -> Bool:
        if found < 0:
            return False
        var kind = self.nodes[found].kind
        if kind != JSON_TRUE and kind != JSON_FALSE:
            self.failed = True
        return kind == JSON_TRUE

    def req_bool(mut self, node: Int, key: String) -> Bool:
        return self.boolean(self.field(node, key, True))

    def def_bool(mut self, node: Int, key: String) -> Bool:
        return self.boolean(self.field(node, key, False))

    def req_object(mut self, node: Int, key: String) -> Int:
        return self.need(node, key, JSON_OBJECT)

    def req_list(mut self, node: Int, key: String) -> List[Int]:
        var found = self.need(node, key, JSON_ARRAY)
        return self.children(found) if found >= 0 else List[Int]()

    def def_list(mut self, node: Int, key: String) -> List[Int]:
        var found = self.defaulted(node, key, JSON_ARRAY)
        return self.children(found) if found >= 0 else List[Int]()

    def string(mut self, node: Int) -> String:
        if node < 0 or self.nodes[node].kind != JSON_STRING:
            self.failed = True
            return String()
        return self.nodes[node].text

    def refuse(mut self, refusal: Int) -> Int:
        if self.refusal == JSON_OK:
            self.refusal = refusal
        return JSON_ABSENT

    def push(mut self, kind: UInt8, var key: String) -> Int:
        if len(self.nodes) >= self.limit:
            return self.refuse(JSON_REFUSED_NODES)
        self.nodes.append(JsonNode(kind, key^, String(), 0, -1, -1, 0))
        return len(self.nodes) - 1

    def space(mut self, bytes: List[UInt8]):
        while self.at < len(bytes):
            var byte = bytes[self.at]
            if byte != 32 and byte != 9 and byte != 10 and byte != 13:
                return
            self.at += 1

    def word(mut self, bytes: List[UInt8], word: String) -> Bool:
        var count = word.byte_length()
        if self.at + count > len(bytes):
            return False
        for offset in range(count):
            if bytes[self.at + offset] != word.as_bytes()[offset]:
                return False
        self.at += count
        return True

    def hex(mut self, bytes: List[UInt8]) -> Int:
        if self.at + 4 > len(bytes):
            return -1
        var value = 0
        for _ in range(4):
            var byte = Int(bytes[self.at])
            var digit: Int
            if byte >= 48 and byte <= 57:
                digit = byte - 48
            elif byte >= 65 and byte <= 70:
                digit = byte - 55
            elif byte >= 97 and byte <= 102:
                digit = byte - 87
            else:
                return -1
            value = value * 16 + digit
            self.at += 1
        return value

    def quoted(mut self, bytes: List[UInt8]) -> Optional[String]:
        var count = len(bytes)
        if self.at >= count or bytes[self.at] != 34:
            return None
        self.at += 1
        var out = List[UInt8]()
        while True:
            if self.at >= count:
                return None
            var byte = bytes[self.at]
            if byte == 34:
                self.at += 1
                break
            if byte < 32:
                return None
            if byte != 92:
                var width = 1
                if byte >= 128:
                    width = _utf8_width(bytes, self.at)
                    if width == 0:
                        return None
                for offset in range(width):
                    out.append(bytes[self.at + offset])
                self.at += width
                continue
            self.at += 1
            if self.at >= count:
                return None
            var escape = bytes[self.at]
            self.at += 1
            if escape == 34 or escape == 92 or escape == 47:
                out.append(escape)
            elif escape == 98:
                out.append(8)
            elif escape == 102:
                out.append(12)
            elif escape == 110:
                out.append(10)
            elif escape == 114:
                out.append(13)
            elif escape == 116:
                out.append(9)
            elif escape == 117:
                var scalar = self.hex(bytes)
                if scalar < 0 or (scalar >= 0xDC00 and scalar <= 0xDFFF):
                    return None
                if scalar >= 0xD800 and scalar <= 0xDBFF:
                    if not self.word(bytes, "\\u"):
                        return None
                    var low = self.hex(bytes)
                    if low < 0xDC00 or low > 0xDFFF:
                        return None
                    scalar = 0x10000 + ((scalar - 0xD800) << 10) + (low - 0xDC00)
                _push_scalar(out, scalar)
            else:
                return None
        return Optional[String](String(unsafe_from_utf8=Span(out)))

    def number(mut self, bytes: List[UInt8], node: Int) -> Bool:
        var count = len(bytes)
        var start = self.at
        var negative = False
        if bytes[self.at] == 45:
            negative = True
            self.at += 1
        if self.at >= count or bytes[self.at] < 48 or bytes[self.at] > 57:
            return False
        var magnitude: UInt64 = 0
        var fits = True
        var lead = List[UInt8]()
        var seen = False
        var scale = 0
        if bytes[self.at] == 48:
            self.at += 1
            if self.at < count and bytes[self.at] >= 48 and bytes[self.at] <= 57:
                return False
        else:
            var first = self.at
            while self.at < count and bytes[self.at] >= 48 and bytes[self.at] <= 57:
                var digit = UInt64(bytes[self.at] - 48)
                if magnitude > (UInt64.MAX - digit) // 10:
                    fits = False
                else:
                    magnitude = magnitude * 10 + digit
                if len(lead) < 17:
                    lead.append(bytes[self.at] - 48)
                self.at += 1
            seen = True
            scale = self.at - first - 1
        var whole = True
        if self.at < count and bytes[self.at] == 46:
            whole = False
            self.at += 1
            if self.at >= count or bytes[self.at] < 48 or bytes[self.at] > 57:
                return False
            var place = 0
            while self.at < count and bytes[self.at] >= 48 and bytes[self.at] <= 57:
                place += 1
                if not seen and bytes[self.at] != 48:
                    seen = True
                    scale = -place
                if seen and len(lead) < 17:
                    lead.append(bytes[self.at] - 48)
                self.at += 1
        var exponent = 0
        if self.at < count and (bytes[self.at] == 101 or bytes[self.at] == 69):
            whole = False
            self.at += 1
            var down = False
            if self.at < count and (bytes[self.at] == 43 or bytes[self.at] == 45):
                down = bytes[self.at] == 45
                self.at += 1
            if self.at >= count or bytes[self.at] < 48 or bytes[self.at] > 57:
                return False
            while self.at < count and bytes[self.at] >= 48 and bytes[self.at] <= 57:
                if exponent < 100000000:
                    exponent = exponent * 10 + Int(bytes[self.at] - 48)
                self.at += 1
            if down:
                exponent = -exponent
        var kind = JSON_FLOAT
        if whole and fits:
            if not negative:
                kind = JSON_UNSIGNED
            elif magnitude > 0 and magnitude <= UInt64(1) << 63:
                kind = JSON_SIGNED
        if kind == JSON_FLOAT and seen:
            var power = scale + exponent
            if power > 308 or (power == 308 and _beyond(lead)):
                return False
        self.nodes[node].kind = kind
        self.nodes[node].magnitude = magnitude if kind != JSON_FLOAT else 0
        self.nodes[node].text = String(
            unsafe_from_utf8=Span(bytes)[start : self.at]
        )
        return True

    def value(mut self, bytes: List[UInt8], var key: String, depth: Int) -> Int:
        self.space(bytes)
        if self.at >= len(bytes):
            return self.refuse(JSON_REFUSED_SYNTAX)
        var byte = bytes[self.at]
        var node = self.push(JSON_NULL, key^)
        if node < 0:
            return node
        if byte == 34:
            var text = self.quoted(bytes)
            if not text:
                return self.refuse(JSON_REFUSED_SYNTAX)
            self.nodes[node].kind = JSON_STRING
            self.nodes[node].text = text.value()
            return node
        if byte == 45 or (byte >= 48 and byte <= 57):
            if not self.number(bytes, node):
                return self.refuse(JSON_REFUSED_SYNTAX)
            return node
        if byte == 110:
            if not self.word(bytes, "null"):
                return self.refuse(JSON_REFUSED_SYNTAX)
            return node
        if byte == 116:
            if not self.word(bytes, "true"):
                return self.refuse(JSON_REFUSED_SYNTAX)
            self.nodes[node].kind = JSON_TRUE
            return node
        if byte == 102:
            if not self.word(bytes, "false"):
                return self.refuse(JSON_REFUSED_SYNTAX)
            self.nodes[node].kind = JSON_FALSE
            return node
        if byte != 91 and byte != 123:
            return self.refuse(JSON_REFUSED_SYNTAX)
        if depth + 1 >= JSON_MAX_DEPTH:
            return self.refuse(JSON_REFUSED_DEPTH)
        var object = byte == 123
        var close: UInt8 = 125 if object else 93
        self.nodes[node].kind = JSON_OBJECT if object else JSON_ARRAY
        self.at += 1
        self.space(bytes)
        if self.at < len(bytes) and bytes[self.at] == close:
            self.at += 1
            return node
        var last = -1
        while True:
            var name = String()
            if object:
                self.space(bytes)
                var parsed = self.quoted(bytes)
                if not parsed:
                    return self.refuse(JSON_REFUSED_SYNTAX)
                name = parsed.value()
                self.space(bytes)
                if self.at >= len(bytes) or bytes[self.at] != 58:
                    return self.refuse(JSON_REFUSED_SYNTAX)
                self.at += 1
            var child = self.value(bytes, name^, depth + 1)
            if child < 0:
                return child
            if last < 0:
                self.nodes[node].first = child
            else:
                self.nodes[last].next = child
            last = child
            self.nodes[node].count += 1
            self.space(bytes)
            if self.at >= len(bytes):
                return self.refuse(JSON_REFUSED_SYNTAX)
            if bytes[self.at] == close:
                self.at += 1
                return node
            if bytes[self.at] != 44:
                return self.refuse(JSON_REFUSED_SYNTAX)
            self.at += 1


def _beyond(lead: List[UInt8]) -> Bool:
    var limit = "17976931348623158"
    for index in range(17):
        var digit = lead[index] if index < len(lead) else UInt8(0)
        var bound = limit.as_bytes()[index] - 48
        if digit != bound:
            return digit > bound
    return True


def _utf8_width(bytes: List[UInt8], at: Int) -> Int:
    var first = Int(bytes[at])
    var width: Int
    var low = 0x80
    var high = 0xBF
    if first >= 0xC2 and first <= 0xDF:
        width = 2
    elif first >= 0xE0 and first <= 0xEF:
        width = 3
        if first == 0xE0:
            low = 0xA0
        elif first == 0xED:
            high = 0x9F
    elif first >= 0xF0 and first <= 0xF4:
        width = 4
        if first == 0xF0:
            low = 0x90
        elif first == 0xF4:
            high = 0x8F
    else:
        return 0
    if at + width > len(bytes):
        return 0
    var second = Int(bytes[at + 1])
    if second < low or second > high:
        return 0
    for offset in range(2, width):
        var next = Int(bytes[at + offset])
        if next < 0x80 or next > 0xBF:
            return 0
    return width


def _push_scalar(mut out: List[UInt8], scalar: Int):
    if scalar < 0x80:
        out.append(UInt8(scalar))
    elif scalar < 0x800:
        out.append(UInt8(0xC0 | (scalar >> 6)))
        out.append(UInt8(0x80 | (scalar & 0x3F)))
    elif scalar < 0x10000:
        out.append(UInt8(0xE0 | (scalar >> 12)))
        out.append(UInt8(0x80 | ((scalar >> 6) & 0x3F)))
        out.append(UInt8(0x80 | (scalar & 0x3F)))
    else:
        out.append(UInt8(0xF0 | (scalar >> 18)))
        out.append(UInt8(0x80 | ((scalar >> 12) & 0x3F)))
        out.append(UInt8(0x80 | ((scalar >> 6) & 0x3F)))
        out.append(UInt8(0x80 | (scalar & 0x3F)))


def json_parse(bytes: List[UInt8], limit: Int = JSON_MAX_NODES) -> Json:
    var json = Json(limit)
    if len(bytes) > JSON_MAX_BYTES:
        _ = json.refuse(JSON_REFUSED_BYTES)
        return json^
    var root = json.value(bytes, String(), 0)
    if root >= 0:
        json.space(bytes)
        if json.at != len(bytes):
            _ = json.refuse(JSON_REFUSED_SYNTAX)
    return json^


def json_text(text: String, limit: Int = JSON_MAX_NODES) -> Json:
    var bytes = List[UInt8]()
    for byte in text.as_bytes():
        bytes.append(byte)
    return json_parse(bytes, limit)


def base64_bytes(text: String) -> Optional[List[UInt8]]:
    var count = text.byte_length()
    if count % 4 != 0:
        return None
    var out = List[UInt8]()
    var index = 0
    while index < count:
        var block = 0
        var pad = 0
        for offset in range(4):
            var byte = Int(text.as_bytes()[index + offset])
            var digit: Int
            if byte >= 65 and byte <= 90:
                digit = byte - 65
            elif byte >= 97 and byte <= 122:
                digit = byte - 71
            elif byte >= 48 and byte <= 57:
                digit = byte + 4
            elif byte == 43:
                digit = 62
            elif byte == 47:
                digit = 63
            elif byte == 61 and index + 4 == count and offset >= 2:
                digit = 0
                pad += 1
            else:
                return None
            if pad > 0 and byte != 61:
                return None
            block = (block << 6) | digit
        if pad == 1 and (block & 0xFF) != 0:
            return None
        if pad == 2 and (block & 0xFFFF) != 0:
            return None
        out.append(UInt8((block >> 16) & 0xFF))
        if pad < 2:
            out.append(UInt8((block >> 8) & 0xFF))
        if pad < 1:
            out.append(UInt8(block & 0xFF))
        index += 4
    return Optional[List[UInt8]](out^)
