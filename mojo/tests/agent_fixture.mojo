from browser_fixture import Source


@fieldwise_init
struct AgentCase(Movable):
    var op: String
    var fields: List[String]
    var expected: String


def read_case(mut source: Source) raises -> AgentCase:
    var op = source.text()
    var count = Int(source.uint(1))
    var fields = List[String]()
    for _ in range(count):
        fields.append(source.text())
    return AgentCase(op^, fields^, source.text())


def parsed(text: String) -> Optional[Int]:
    var count = text.byte_length()
    var index = 0
    var negative = False
    if count > 0 and text.as_bytes()[0] == 45:
        negative = True
        index = 1
    if index >= count:
        return None
    var value = 0
    while index < count:
        var byte = Int(text.as_bytes()[index])
        if byte < 48 or byte > 57:
            return None
        value = value * 10 + byte - 48
        index += 1
    return Optional[Int](-value if negative else value)


def unsigned(text: String) -> UInt64:
    var value: UInt64 = 0
    for byte in text.as_bytes():
        if byte < 48 or byte > 57:
            return 0
        value = value * 10 + UInt64(byte - 48)
    return value


def number(text: String) -> Int:
    var value = parsed(text)
    return value.value() if value else 0


def some(value: Optional[String]) -> String:
    if value:
        return String("S", value.value())
    return "N"


def bit(flag: Bool) -> String:
    return "1" if flag else "0"


def numbers(values: List[Int]) -> String:
    var joined = String()
    for index in range(len(values)):
        if index > 0:
            joined += ","
        joined += String(values[index])
    return joined^
