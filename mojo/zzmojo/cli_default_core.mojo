from .exec_protocol import Invocation, CommandSource
from .palette_unicode import whitespace_table


comptime CONFIG_BYTE_BASE: UInt32 = 0xF0000
comptime CONFIG_LITERAL: UInt32 = 0xF0100
comptime CONFIG_EOF: UInt32 = 0xF0101
comptime DEFAULT_SOURCE = "default-client-command"
comptime MAX_DEFAULT_BYTES = 4 * 1024 * 1024
comptime MAX_DEFAULT_COMMANDS = 4096
comptime MAX_DEFAULT_WORDS = 65536
comptime MAX_DEFAULT_DEPTH = 256
comptime MAX_DEFAULT_EXPANDED = 16 * 1024 * 1024


def append_scalar(mut output: List[UInt8], scalar: UInt32):
    if scalar < 128:
        output.append(UInt8(scalar))
    elif scalar < 2048:
        output.append(UInt8(0xC0 | (scalar >> 6)))
        output.append(UInt8(0x80 | (scalar & 63)))
    elif scalar < 65536:
        output.append(UInt8(0xE0 | (scalar >> 12)))
        output.append(UInt8(0x80 | ((scalar >> 6) & 63)))
        output.append(UInt8(0x80 | (scalar & 63)))
    else:
        output.append(UInt8(0xF0 | (scalar >> 18)))
        output.append(UInt8(0x80 | ((scalar >> 12) & 63)))
        output.append(UInt8(0x80 | ((scalar >> 6) & 63)))
        output.append(UInt8(0x80 | (scalar & 63)))


def scalar_list(text: String) -> List[UInt32]:
    var result = List[UInt32]()
    for scalar in text.codepoints():
        result.append(UInt32(Int(scalar)))
    return result^


def stored_byte(scalar: UInt32) -> Bool:
    return scalar >= CONFIG_BYTE_BASE + 128 and scalar <= CONFIG_BYTE_BASE + 255


def raw_word(word: List[UInt32], encoded: Bool) -> List[UInt8]:
    var result = List[UInt8]()
    var index = 0
    while index < len(word):
        var scalar = word[index]
        index += 1
        if encoded and scalar == CONFIG_LITERAL:
            if index < len(word):
                scalar = word[index]
                index += 1
            append_scalar(result, scalar)
        elif encoded and stored_byte(scalar):
            result.append(UInt8(scalar - CONFIG_BYTE_BASE))
        else:
            append_scalar(result, scalar)
    return result^


def word_text(word: List[UInt32]) raises -> String:
    var bytes = raw_word(word, False)
    return String(from_utf8=Span(bytes))


def variable_character(scalar: UInt32, first: Bool) -> Bool:
    var digit = scalar >= 48 and scalar <= 57
    return (not first or not digit) and (digit or scalar >= 65 and scalar <= 90 or scalar >= 97 and scalar <= 122 or scalar == 95)


def conditional(token: String) -> Bool:
    return token == "%if" or token == "%elif" or token == "%else" or token == "%endif"


struct DefaultParse(Copyable, Movable):
    var commands: List[Invocation]
    var kind: UInt8
    var error: String
    var line: UInt32
    var column: UInt32
    var needed_home: Optional[String]

    def __init__(out self):
        self.commands = List[Invocation]()
        self.kind = 0
        self.error = String()
        self.line = 0
        self.column = 0
        self.needed_home = None


@fieldwise_init
struct DefaultAssignment(Copyable, Movable):
    var name: String
    var value: List[UInt32]


@fieldwise_init
struct ConditionalScope(TrivialRegisterPassable):
    var parent_active: Bool
    var branch_taken: Bool
    var active: Bool
    var saw_else: Bool


struct DefaultCharacters(Movable):
    var data: List[UInt32]
    var offset: Int
    var escapes: Int
    var skipped: UInt32
    var line: UInt32
    var column: UInt32
    var encoded: Bool

    def __init__(out self, text: String, encoded: Bool):
        self.data = List[UInt32]()
        if encoded:
            for byte in text.as_bytes():
                self.data.append(UInt32(byte) if byte < 128 else CONFIG_BYTE_BASE + UInt32(byte))
        else:
            self.data = scalar_list(text)
        self.offset = 0
        self.escapes = 0
        self.skipped = 0
        self.line = 1
        self.column = 0
        self.encoded = encoded

    def get(mut self) -> Optional[UInt32]:
        if not self.encoded:
            if self.offset >= len(self.data):
                return None
            var scalar = self.data[self.offset]
            self.offset += 1
            return scalar
        if self.escapes > 0:
            self.escapes -= 1
            return UInt32(92)
        while True:
            var scalar = CONFIG_EOF
            if self.offset < len(self.data):
                scalar = self.data[self.offset]
                self.offset += 1
            if scalar == 92:
                self.escapes += 1
                continue
            if scalar == 10 and self.escapes % 2 == 1:
                self.skipped += 1
                self.escapes -= 1
                continue
            if self.escapes > 0:
                self.unget(scalar)
                self.escapes -= 1
                return UInt32(92)
            return scalar

    def unget(mut self, scalar: UInt32):
        if self.offset > 0 and (not self.encoded or scalar != CONFIG_EOF):
            self.offset -= 1

    def _skip(mut self):
        if self.skipped > 0:
            self.line += self.skipped
            self.column = 0
            self.skipped = 0

    def take(mut self) -> Optional[UInt32]:
        var scalar = self.get()
        if scalar:
            self._skip()
            self.column += 1
        return scalar

    def peek(mut self) -> Optional[UInt32]:
        var scalar = self.get()
        if scalar:
            self._skip()
            self.unget(scalar.value())
        return scalar

    def eof(self, scalar: UInt32) -> Bool:
        return self.encoded and scalar == CONFIG_EOF


struct DefaultBuilder(Movable):
    var result: DefaultParse
    var overlay: Dict[String, List[UInt32]]
    var scopes: List[ConditionalScope]
    var spaces: List[UInt32]
    var home: Optional[String]
    var homes: Dict[String, Optional[String]]
    var encoded: Bool
    var needs_bytes: Bool
    var word: List[UInt32]
    var words: List[List[UInt32]]
    var blocks: List[Int]
    var percents: List[Int]
    var word_started: Bool
    var word_block: Bool
    var percent_word: Bool
    var eager_assignment: Bool
    var total_words: Int
    var expanded_bytes: Int

    def __init__(out self, encoded: Bool, home: Optional[String], homes: Dict[String, Optional[String]]):
        self.result = DefaultParse()
        self.overlay = Dict[String, List[UInt32]]()
        self.scopes = List[ConditionalScope]()
        self.spaces = whitespace_table()
        self.home = home.copy()
        self.homes = homes.copy()
        self.encoded = encoded
        self.needs_bytes = False
        self.word = List[UInt32]()
        self.words = List[List[UInt32]]()
        self.blocks = List[Int]()
        self.percents = List[Int]()
        self.word_started = False
        self.word_block = False
        self.percent_word = False
        self.eager_assignment = False
        self.total_words = 0
        self.expanded_bytes = 0

    def append_char(mut self, scalar: UInt32):
        if self.aborted():
            return
        var width = 1 if scalar < 128 else 2 if scalar < 2048 else 3 if scalar < 65536 else 4
        if width > MAX_DEFAULT_EXPANDED - self.expanded_bytes:
            self.diagnostic(0, 0, "default command exceeds its expanded text limit", 2)
            return
        self.expanded_bytes += width
        self.word.append(scalar)

    def extend_word(mut self, value: List[UInt32]):
        if self.aborted():
            return
        var size = 0
        for scalar in value:
            size += 1 if scalar < 128 else 2 if scalar < 2048 else 3 if scalar < 65536 else 4
        if size > MAX_DEFAULT_EXPANDED - self.expanded_bytes:
            self.diagnostic(0, 0, "default command exceeds its expanded text limit", 2)
            return
        self.expanded_bytes += size
        self.word.extend(Span(value))

    def literal_char(mut self, scalar: UInt32):
        if self.encoded and (scalar == CONFIG_LITERAL or stored_byte(scalar)):
            self.append_char(CONFIG_LITERAL)
        self.append_char(scalar)

    def literal(mut self, text: String):
        for scalar in text.codepoints():
            if self.aborted():
                return
            self.literal_char(UInt32(Int(scalar)))

    def active(self) -> Bool:
        return len(self.scopes) == 0 or self.scopes[len(self.scopes) - 1].active

    def aborted(self) -> Bool:
        return self.result.kind != 0

    def diagnostic(mut self, line: UInt32, column: UInt32, message: String, kind: UInt8 = 1):
        if self.aborted():
            return
        self.result.commands.clear()
        self.result.kind = kind
        self.result.error = message
        self.result.line = line
        self.result.column = column

    def space(self, scalar: UInt32, word_start: Bool = False) -> Bool:
        if self.encoded:
            return scalar == 32 or scalar == 9 or word_start and scalar == 10
        for value in self.spaces:
            if value == scalar:
                return True
        return False

    def assignment(mut self, token: List[UInt32], line: UInt32, column: UInt32, report_error: Bool = True) raises -> Optional[DefaultAssignment]:
        var equal = -1
        for index in range(len(token)):
            if token[index] == 61:
                equal = index
                break
        if equal <= 0:
            return None
        for index in range(equal):
            if not variable_character(token[index], index == 0):
                return None
        var bytes = raw_word(token, self.encoded)
        if len(bytes) > 16384:
            if report_error:
                self.diagnostic(line, column, "environment variable is too long")
            return None
        var name = List[UInt32](Span(token)[:equal])
        var value = List[UInt32](Span(token)[equal + 1:])
        return DefaultAssignment(word_text(name), value^)

    def push_assignment(mut self, assignment: DefaultAssignment):
        if self.active():
            self.overlay[assignment.name] = assignment.value.copy()

    def variable(self, name: String) raises -> List[UInt32]:
        if name in self.overlay:
            return self.overlay[name].copy()
        return List[UInt32]()

    def expand_home(mut self, name: String, line: UInt32, column: UInt32) raises:
        if name.byte_length() == 0:
            var value = self.variable("HOME")
            if len(value) > 0:
                self.extend_word(value)
                return
            if self.home:
                var path = self.home.value().copy()
                self.literal(path)
            else:
                self.diagnostic(line, column, "syntax error")
        elif name not in self.homes:
            self.diagnostic(line, column, "default command requires a named home lookup", 2)
            self.result.needed_home = name
        elif self.homes[name]:
            var path = self.homes[name].value().copy()
            self.literal(path)
        else:
            self.diagnostic(line, column, "syntax error")

    def finish_word(mut self, line: UInt32, column: UInt32) raises:
        if not self.word_started:
            return
        if self.total_words >= MAX_DEFAULT_WORDS:
            self.diagnostic(line, column, "default command exceeds its word limit", 2)
            return
        self.total_words += 1
        if self.encoded and not self.word_block:
            for index in range(len(self.word)):
                if self.word[index] == 0:
                    self.word.shrink(index)
                    break
        if self.word_block:
            self.blocks.append(len(self.words))
        if self.percent_word:
            self.percents.append(len(self.words))
        self.words.append(self.word^)
        self.word = List[UInt32]()
        self.word_started = False
        self.word_block = False
        self.percent_word = False
        if len(self.words) == 1:
            var first_word = self.words[0].copy()
            var assignment = self.assignment(first_word, line, column)
            if assignment:
                self.push_assignment(assignment.value())
                self.eager_assignment = True

    def simple_finish_word(mut self):
        if not self.word_started:
            return
        if self.total_words >= MAX_DEFAULT_WORDS:
            self.diagnostic(0, 0, "default command exceeds its word limit", 2)
            return
        if self.word_block:
            self.blocks.append(len(self.words))
        if self.percent_word:
            self.percents.append(len(self.words))
        self.words.append(self.word^)
        self.word = List[UInt32]()
        self.word_started = False
        self.word_block = False
        self.percent_word = False
        self.total_words += 1

    def finish_conditional(mut self, directive: String, line: UInt32, column: UInt32):
        if directive == "%if":
            if len(self.scopes) >= MAX_DEFAULT_DEPTH:
                self.diagnostic(line, column, "default command exceeds its conditional depth limit", 2)
                return
            self.scopes.append(ConditionalScope(self.active(), False, False, False))
        elif directive == "%endif":
            if len(self.scopes) == 0:
                self.diagnostic(line, column, "syntax error")
            else:
                _ = self.scopes.pop()
        elif len(self.scopes) == 0:
            self.diagnostic(line, column, "syntax error")
        else:
            ref scope = self.scopes[len(self.scopes) - 1]
            if scope.saw_else:
                self.diagnostic(line, column, "syntax error")
            elif directive == "%elif":
                scope.active = False
            else:
                scope.active = scope.parent_active and not scope.branch_taken
                scope.branch_taken = True
                scope.saw_else = True

    def finish_command(mut self, tokens: List[List[UInt32]], blocks: List[Int], start: Int, end: Int, line: UInt32, column: UInt32, completion_line: UInt32, assignment_recorded: Bool) raises:
        var name_index = start
        var assignment = self.assignment(tokens[start], line, column)
        if self.aborted():
            return
        if assignment:
            if not assignment_recorded:
                self.push_assignment(assignment.value())
            if end - start == 1:
                return
            name_index += 1
            var second = self.assignment(tokens[name_index], line, column, False)
            if second:
                self.diagnostic(line, column, "syntax error")
                return
            if self.aborted():
                return
        for block in blocks:
            if block == name_index:
                self.diagnostic(line, column, "syntax error")
                return
        if not self.active():
            return
        if len(self.result.commands) >= MAX_DEFAULT_COMMANDS:
            self.diagnostic(line, column, "default command exceeds its command limit", 2)
            return
        var command = Invocation()
        var name_bytes = raw_word(tokens[name_index], self.encoded)
        command.name = String(from_utf8_lossy=Span(name_bytes))
        for index in range(name_index + 1, end):
            command.args.append(raw_word(tokens[index], self.encoded))
        command.source = CommandSource(String(DEFAULT_SOURCE), completion_line, column)
        for block in blocks:
            if block > name_index and block < end:
                command.blocks.append(UInt32(block - name_index - 1))
        self.result.commands.append(command^)

    def finish_statement(mut self, line: UInt32, column: UInt32, completion_line: UInt32) raises:
        self.finish_word(line, column)
        if self.aborted():
            return
        if len(self.words) == 0:
            self.blocks.clear()
            self.percents.clear()
            return
        var words = self.words^
        self.words = List[List[UInt32]]()
        var blocks = self.blocks^
        self.blocks = List[Int]()
        var percents = self.percents^
        self.percents = List[Int]()
        for index in percents:
            var token = word_text(words[index])
            var invalid = len(words[index]) > 0 and words[index][0] == 37
            if invalid:
                invalid = False
                for scalar in words[index]:
                    if scalar != 37 and (scalar < 48 or scalar > 57):
                        invalid = True
                        break
            if invalid and not conditional(token) and not (index == 0 and token == "%hidden"):
                self.diagnostic(line, column, "syntax error")
                return
        if word_text(words[0]) == "%hidden":
            if len(words) != 2:
                self.diagnostic(line, column, "syntax error")
                return
            var assignment = self.assignment(words[1], line, column)
            if assignment:
                self.push_assignment(assignment.value())
            elif not self.aborted():
                self.diagnostic(line, column, "syntax error")
            return
        var start = 0
        while start < len(words):
            var token = word_text(words[start])
            if token == "%if" or token == "%elif":
                if start + 1 >= len(words):
                    self.diagnostic(line, column, "syntax error")
                    return
                self.finish_conditional(token, line, column)
                start += 2
            elif token == "%else" or token == "%endif":
                self.finish_conditional(token, line, column)
                start += 1
            else:
                var end = start + 1
                while end < len(words) and not conditional(word_text(words[end])):
                    end += 1
                self.finish_command(words, blocks, start, end, line, column, completion_line, start == 0 and self.eager_assignment)
                start = end
            if self.aborted():
                break
        self.eager_assignment = False

    def finish(mut self, line: UInt32, column: UInt32):
        if not self.aborted() and len(self.scopes) > 0:
            self.diagnostic(line, column + 1, "syntax error")

    def take_result(mut self) -> DefaultParse:
        var result = DefaultParse()
        swap(result, self.result)
        return result^
