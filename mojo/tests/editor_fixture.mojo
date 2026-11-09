struct Source(Movable):
    var bytes: List[UInt8]
    var offset: Int

    def __init__(out self, var bytes: List[UInt8]):
        self.bytes = bytes^
        self.offset = 0

    def uint(mut self, count: Int) raises -> UInt64:
        if count > len(self.bytes) - self.offset:
            raise Error("truncated editor source fixture")
        var value: UInt64 = 0
        for i in range(count):
            value |= UInt64(self.bytes[self.offset + i]) << UInt64(i * 8)
        self.offset += count
        return value

    def flag(mut self) raises -> Bool:
        return self.uint(1) != 0

    def text(mut self) raises -> String:
        var count = Int(self.uint(4))
        if count > len(self.bytes) - self.offset:
            raise Error("truncated editor source text")
        var value = String(
            unsafe_from_utf8=Span(self.bytes)[self.offset : self.offset + count]
        )
        self.offset += count
        return value^

    def optional(mut self) raises -> Optional[String]:
        var present = self.flag()
        var value = self.text()
        if present:
            return value^
        return None

    def index(mut self) raises -> Optional[Int]:
        var present = self.flag()
        var value = Int(self.uint(4))
        if present:
            return value
        return None

    def matches(mut self, body: List[UInt8]) raises -> Bool:
        var count = Int(self.uint(4))
        if count > len(self.bytes) - self.offset:
            raise Error("truncated editor source dump")
        var same = count == len(body)
        if same:
            for i in range(count):
                if self.bytes[self.offset + i] != body[i]:
                    same = False
                    break
        self.offset += count
        return same


struct Sink(Movable):
    var bytes: List[UInt8]

    def __init__(out self):
        self.bytes = List[UInt8]()

    def uint(mut self, value: UInt64, count: Int):
        for i in range(count):
            self.bytes.append(UInt8((value >> UInt64(i * 8)) & 0xFF))

    def flag(mut self, value: Bool):
        self.bytes.append(UInt8(1) if value else UInt8(0))

    def text(mut self, value: String):
        self.uint(UInt64(value.byte_length()), 4)
        self.bytes.extend(value.as_bytes())

    def optional(mut self, value: Optional[String]):
        self.flag(Bool(value))
        if value:
            self.text(value.value())
        else:
            self.uint(0, 4)

    def index(mut self, value: Optional[Int]):
        self.flag(Bool(value))
        if value:
            self.uint(UInt64(value.value()), 4)
        else:
            self.uint(0, 4)
