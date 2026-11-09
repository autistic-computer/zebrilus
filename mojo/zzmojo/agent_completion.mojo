from .agent_settings import LensSettings, setter
from .agent_text import (
    text_blank,
    text_boundary,
    text_contains,
    text_slice,
    text_trim,
    text_trim_start,
)
from .palette_text import palette_space, scalar_width

comptime MAX_COMPLETION_RESULTS = 64
comptime MAX_RENDERED_ERROR_BYTES = 1024


@fieldwise_init
struct AgentCommand(Copyable, Movable):
    var name: String
    var description: String
    var input_hint: Optional[String]
    var skill: Bool


@fieldwise_init
struct CommandCompletion(Copyable, Movable):
    var command: AgentCommand
    var start: Int
    var end: Int

    def insertion(self) -> String:
        return String(command_label(self.command.name), " ")

    def submits(self) -> Bool:
        var found = setter(self.insertion())
        return Bool(found) and Bool(found.value().value)


@fieldwise_init
struct CompletionQuery(Copyable, Movable):
    var needle: String
    var start: Int
    var end: Int


def rendered_error(error: String) -> String:
    var source = text_trim(error)
    var out = List[UInt8]()
    var at = 0
    var truncated = False
    for character in source.codepoints():
        var scalar = UInt32(Int(character))
        var width = scalar_width(scalar)
        var control = (scalar < 32 or scalar >= 127 and scalar <= 159) and (
            scalar != 10 and scalar != 9
        )
        var size = 3 if control else width
        if len(out) + size > MAX_RENDERED_ERROR_BYTES - 3:
            truncated = True
            break
        if control:
            out.append(0xEF)
            out.append(0xBF)
            out.append(0xBD)
        else:
            for offset in range(width):
                out.append(source.as_bytes()[at + offset])
        at += width
    if truncated:
        out.append(0xE2)
        out.append(0x80)
        out.append(0xA6)
    return String(from_utf8_lossy=Span(out))


def agent_verb(name: String) -> Bool:
    return (
        name == "btw"
        or name == "side"
        or name == "steer"
        or name == "fork"
        or name == "rewind"
    )


def bare_command_name(name: String) -> String:
    var start = 0
    var count = name.byte_length()
    while start < count and name.as_bytes()[start] == 47:
        start += 1
    return text_slice(name, start, count)


def command_label(name: String) -> String:
    var bare = bare_command_name(name)
    if bare.startswith("$"):
        return bare^
    return String("/", bare)


def pane_commands(vendor: List[AgentCommand], verbs: Bool) -> List[AgentCommand]:
    if not verbs:
        return vendor.copy()
    var commands = List[AgentCommand]()
    for command in vendor:
        if not agent_verb(bare_command_name(command.name)):
            commands.append(command.copy())
    commands.append(
        AgentCommand(
            "btw",
            "Ask on the side without adding to the conversation",
            Optional[String]("question"),
            False,
        )
    )
    commands.append(
        AgentCommand("side", "Same as /btw", Optional[String]("question"), False)
    )
    commands.append(
        AgentCommand(
            "steer", "Redirect the running turn", Optional[String]("text"), False
        )
    )
    commands.append(
        AgentCommand(
            "fork", "Continue in a copy of this conversation", None, False
        )
    )
    commands.append(
        AgentCommand(
            "rewind",
            "Continue from before your last n prompts; files stay as they are",
            Optional[String]("n or prompt id"),
            False,
        )
    )
    return commands^


def ascii_lower(text: String) -> String:
    var out = List[UInt8]()
    for byte in text.as_bytes():
        out.append(byte + 32 if byte >= 65 and byte <= 90 else byte)
    return String(from_utf8_lossy=Span(out))


def _spaced(text: String) -> Bool:
    for character in text.codepoints():
        if palette_space(UInt32(Int(character))):
            return True
    return False


def completion_score(candidate: String, needle: String) -> Int:
    if needle.byte_length() == 0:
        return 3
    if candidate == needle:
        return 0
    if candidate.startswith(needle):
        return 1
    if text_contains(candidate, needle):
        return 2
    var scalars = List[Int]()
    for character in candidate.codepoints():
        scalars.append(Int(character))
    var at = 0
    for character in needle.codepoints():
        var wanted = Int(character)
        var found = False
        while at < len(scalars):
            at += 1
            if scalars[at - 1] == wanted:
                found = True
                break
        if not found:
            return -1
    return 3


def _less(left: String, right: String) -> Bool:
    var shared = min(left.byte_length(), right.byte_length())
    for index in range(shared):
        var first = left.as_bytes()[index]
        var second = right.as_bytes()[index]
        if first != second:
            return first < second
    return left.byte_length() < right.byte_length()


def _ordered(ranks: List[Int], keys: List[String]) -> List[Int]:
    var order = List[Int]()
    for index in range(len(ranks)):
        order.append(index)
    var width = 1
    var count = len(order)
    while width < count:
        var merged = List[Int]()
        var start = 0
        while start < count:
            var middle = min(start + width, count)
            var stop = min(start + 2 * width, count)
            var left = start
            var right = middle
            while left < middle or right < stop:
                var take_right = False
                if left >= middle:
                    take_right = True
                elif right < stop:
                    var first = order[left]
                    var second = order[right]
                    if ranks[second] != ranks[first]:
                        take_right = ranks[second] < ranks[first]
                    else:
                        take_right = _less(keys[second], keys[first])
                if take_right:
                    merged.append(order[right])
                    right += 1
                else:
                    merged.append(order[left])
                    left += 1
            start = stop
        order = merged^
        width *= 2
    return order^


def completion_query(value: String, cursor: Int) -> Optional[CompletionQuery]:
    if cursor < 0 or cursor > value.byte_length():
        return None
    if not text_boundary(value, cursor):
        return None
    var line_start = 0
    var sigil = -1
    var index = cursor - 1
    while index >= 0:
        var byte = value.as_bytes()[index]
        if byte == 10:
            line_start = index + 1
            break
        if byte == 47 and sigil < 0:
            sigil = index
        index -= 1
    if sigil < 0:
        return None
    if sigil > line_start:
        var before = sigil - 1
        while before > 0 and not text_boundary(value, before):
            before -= 1
        var spaced = False
        for character in text_slice(value, before, sigil).codepoints():
            spaced = palette_space(UInt32(Int(character)))
        if not spaced:
            return None
    var tail = text_slice(value, sigil + 1, cursor)
    if _spaced(tail):
        return None
    return Optional[CompletionQuery](CompletionQuery(tail^, sigil, cursor))


def ranked_completions(
    commands: List[AgentCommand], query: CompletionQuery
) -> List[CommandCompletion]:
    var needle = ascii_lower(query.needle)
    var kept = List[Int]()
    var ranks = List[Int]()
    var keys = List[String]()
    for index in range(len(commands)):
        var searchable = ascii_lower(bare_command_name(commands[index].name))
        var score = completion_score(searchable, needle)
        if score < 0:
            continue
        kept.append(index)
        ranks.append(score)
        keys.append(searchable^)
    var found = List[CommandCompletion]()
    for position in _ordered(ranks, keys):
        if len(found) == MAX_COMPLETION_RESULTS:
            break
        found.append(
            CommandCompletion(
                commands[kept[position]].copy(), query.start, query.end
            )
        )
    return found^


def setting_completions(
    value: String, cursor: Int, settings: LensSettings
) -> Optional[List[CommandCompletion]]:
    var count = value.byte_length()
    if cursor < 0 or cursor > count or not text_boundary(value, cursor):
        return None
    if not text_blank(text_slice(value, cursor, count)):
        return None
    var before = text_slice(value, 0, cursor)
    var start = cursor - text_trim_start(before).byte_length()
    if start >= cursor or value.as_bytes()[start] != 47:
        return None
    var split = -1
    for index in range(start + 1, cursor):
        if value.as_bytes()[index] == 32:
            split = index
            break
    if split < 0:
        return None
    var name = text_slice(value, start + 1, split)
    var needle = text_slice(value, split + 1, cursor)
    if _spaced(needle):
        return None
    var found = setter(String("/", name))
    if not found:
        return None
    var option = found.value().option.copy()
    var choices = settings.choices(option)
    var current = settings.current(option)
    needle = ascii_lower(needle)
    var kept = List[Int]()
    var ranks = List[Int]()
    var keys = List[String]()
    for index in range(len(choices)):
        var first = completion_score(ascii_lower(choices[index].value), needle)
        var second = completion_score(ascii_lower(choices[index].name), needle)
        var score = first
        if score < 0 or second >= 0 and second < score:
            score = second
        if score < 0:
            continue
        kept.append(index)
        ranks.append(score * 2 + (0 if choices[index].value == current else 1))
        keys.append(String())
    var completions = List[CommandCompletion]()
    for position in _ordered(ranks, keys):
        if len(completions) == MAX_COMPLETION_RESULTS:
            break
        var choice = choices[kept[position]].copy()
        var description = choice.name.copy()
        if choice.value == current:
            description = String(choice.name, " · current")
        completions.append(
            CommandCompletion(
                AgentCommand(
                    String(option, " ", choice.value), description^, None, False
                ),
                start,
                cursor,
            )
        )
    return Optional[List[CommandCompletion]](completions^)


def composer_completions(
    value: String,
    cursor: Int,
    commands: List[AgentCommand],
    settings: Optional[LensSettings],
) -> List[CommandCompletion]:
    if settings:
        var found = setting_completions(value, cursor, settings.value())
        if found:
            return found.take()
    var query = completion_query(value, cursor)
    if query:
        return ranked_completions(commands, query.value())
    return List[CommandCompletion]()


def meaningful_command_description(description: String) -> Optional[String]:
    var trimmed = text_trim(description)
    for character in trimmed.codepoints():
        var scalar = Int(character)
        if scalar != 46 and scalar != 0x2026:
            return Optional[String](trimmed^)
    return None


def _same_ascii(left: String, right: String) -> Bool:
    if left.byte_length() != right.byte_length():
        return False
    return ascii_lower(left) == ascii_lower(right)


def active_command_hint(
    value: String, commands: List[AgentCommand]
) -> Optional[String]:
    var trimmed = text_trim_start(value)
    if trimmed.byte_length() == 0 or trimmed.as_bytes()[0] != 47:
        return None
    var command = text_slice(trimmed, 1, trimmed.byte_length())
    var name = command.copy()
    var at = 0
    for character in command.codepoints():
        var scalar = UInt32(Int(character))
        var width = scalar_width(scalar)
        if palette_space(scalar):
            name = text_slice(command, 0, at)
            if not text_blank(
                text_slice(command, at + width, command.byte_length())
            ):
                return None
            break
        at += width
    for entry in commands:
        if _same_ascii(bare_command_name(entry.name), name):
            if entry.input_hint:
                return Optional[String](
                    String("Argument · ", entry.input_hint.value())
                )
            return None
    return None
