struct Source(Movable):
    var bytes: List[UInt8]
    var offset: Int

    def __init__(out self, var bytes: List[UInt8]):
        self.bytes = bytes^
        self.offset = 0

    def uint(mut self, count: Int) raises -> UInt64:
        if count > len(self.bytes) - self.offset:
            raise Error("truncated browser source fixture")
        var value: UInt64 = 0
        for i in range(count):
            value |= UInt64(self.bytes[self.offset + i]) << UInt64(i * 8)
        self.offset += count
        return value

    def text(mut self) raises -> String:
        var count = Int(self.uint(4))
        if count > len(self.bytes) - self.offset:
            raise Error("truncated browser source text")
        var value = String(
            unsafe_from_utf8=Span(self.bytes)[self.offset : self.offset + count]
        )
        self.offset += count
        return value^
