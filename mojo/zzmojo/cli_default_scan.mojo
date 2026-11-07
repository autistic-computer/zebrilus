from .cli_default_core import (
    DefaultBuilder, DefaultCharacters, CONFIG_BYTE_BASE,
    MAX_DEFAULT_DEPTH, variable_character, stored_byte,
    word_text,
)


@fieldwise_init
struct CommandBlock(TrivialRegisterPassable):
    var depth: Int
    var quote: UInt8
    var escaped: Bool
    var comment: Bool
    var comment_start: Int
    var saw_eof: Bool
    var word_start: Bool
    var line: UInt32
    var column: UInt32

    def feed(mut self, character: UInt32, mut builder: DefaultBuilder) -> Bool:
        if self.comment:
            self.word_start = character == 10
            if character == 10:
                self.comment = False
            return False
        if self.escaped:
            self.escaped = False
            if character != 10:
                self.word_start = False
            return False
        if character == 92 and self.quote != 1:
            self.escaped = True
            return False
        if self.quote == 1 and character == 39 or self.quote == 2 and character == 34:
            self.quote = 0
            self.word_start = False
        elif self.quote != 0:
            self.word_start = False
        elif character == 39 or character == 34:
            self.quote = UInt8(1 if character == 39 else 2)
            self.word_start = False
        elif character == 35 and self.word_start:
            self.comment = True
            self.comment_start = max(0, len(builder.word) - 1)
            self.word_start = False
        elif character == 123 and self.word_start:
            if self.depth >= MAX_DEFAULT_DEPTH:
                builder.diagnostic(self.line, self.column, "default command exceeds its block depth limit", 2)
                return False
            self.depth += 1
            self.word_start = True
        elif character == 125:
            self.depth = max(0, self.depth - 1)
            if self.depth == 0:
                return True
            self.word_start = True
        elif character == 59 or builder.space(character, True):
            self.word_start = True
        else:
            self.word_start = False
        return False


def escape(mut reader: DefaultCharacters) raises -> Optional[Tuple[UInt32, Bool]]:
    var next = reader.take()
    if not next or reader.eof(next.value()):
        raise Error("syntax error")
    var character = next.value()
    if character == 10:
        reader.line += 1
        reader.column = 0
        return None
    if character >= 52 and character <= 55:
        raise Error("invalid octal escape")
    if character >= 48 and character <= 51:
        var second = reader.take()
        var third: Optional[UInt32] = None
        if second and second.value() >= 48 and second.value() <= 55:
            third = reader.take()
        if not second or not third or third.value() < 48 or third.value() > 55:
            raise Error("invalid octal escape")
        var value = 64 * (character - 48) + 8 * (second.value() - 48) + third.value() - 48
        return (value, reader.encoded or value >= 128)
    if reader.encoded and stored_byte(character):
        return (character - CONFIG_BYTE_BASE, True)
    if character == 117 or character == 85:
        var digits = 4 if character == 117 else 8
        var invalid = String("invalid \\u argument") if character == 117 else String("invalid \\U argument")
        var value: UInt32 = 0
        for _ in range(digits):
            var next_digit = reader.take()
            if not next_digit or reader.eof(next_digit.value()):
                raise Error("syntax error")
            var digit = next_digit.value()
            if digit == 10:
                reader.line += 1
                reader.column = 0
                raise Error("syntax error")
            var nibble: UInt32
            if digit >= 48 and digit <= 57:
                nibble = digit - 48
            elif digit >= 65 and digit <= 70:
                nibble = digit - 55
            elif digit >= 97 and digit <= 102:
                nibble = digit - 87
            else:
                raise Error(invalid)
            value = (value << 4) | nibble
        if value > 0x10FFFF or value >= 0xD800 and value <= 0xDFFF:
            raise Error(invalid)
        return (value, False)
    var value = character
    if character == 97:
        value = 7
    elif character == 98:
        value = 8
    elif character == 101:
        value = 27
    elif character == 102:
        value = 12
    elif character == 115:
        value = 32
    elif character == 118:
        value = 11
    elif character == 114:
        value = 13
    elif character == 110:
        value = 10
    elif character == 116:
        value = 9
    return (value, False)


def expand_variable(mut reader: DefaultCharacters, builder: DefaultBuilder) raises -> List[UInt32]:
    var next = reader.peek()
    if not next or reader.eof(next.value()):
        raise Error("syntax error")
    var braced = next.value() == 123
    if braced:
        _ = reader.take()
    elif not variable_character(next.value(), True):
        var literal = List[UInt32]()
        literal.append(36)
        return literal^
    var name = List[UInt32]()
    while True:
        next = reader.peek()
        if not next or reader.eof(next.value()):
            if braced:
                raise Error("invalid environment variable")
            break
        var character = next.value()
        if braced and character == 125:
            _ = reader.take()
            break
        if not variable_character(character, False):
            if braced:
                _ = reader.take()
                raise Error("invalid environment variable")
            break
        if len(name) == 1022:
            raise Error("environment variable is too long")
        name.append(character)
        _ = reader.take()
    return builder.variable(word_text(name))


def condition_format(mut reader: DefaultCharacters, mut builder: DefaultBuilder) raises:
    builder.append_char(35)
    var next = reader.take()
    if not next or reader.eof(next.value()):
        raise Error("syntax error")
    builder.append_char(next.value())
    var depth = 1
    while True:
        next = reader.take()
        if not next or reader.eof(next.value()):
            raise Error("syntax error")
        var character = next.value()
        if character == 10:
            reader.line += 1
            reader.column = 0
            raise Error("syntax error")
        builder.append_char(character)
        if character == 35:
            next = reader.take()
            if not next or reader.eof(next.value()):
                raise Error("syntax error")
            if next.value() == 10:
                reader.line += 1
                reader.column = 0
                raise Error("syntax error")
            builder.append_char(next.value())
            if next.value() == 123:
                if depth >= MAX_DEFAULT_DEPTH:
                    builder.diagnostic(reader.line, reader.column, "default command exceeds its format depth limit", 2)
                    return
                depth += 1
        elif character == 125:
            depth -= 1
            if depth == 0:
                return


def quoted_prefix(mut reader: DefaultCharacters, mut builder: DefaultBuilder) -> Optional[UInt32]:
    var next = reader.take()
    if not next:
        return None
    while next.value() == 32 or next.value() == 9:
        next = reader.take()
        if not next:
            return None
    if next.value() != 35:
        return next
    next = reader.take()
    if not next or reader.eof(next.value()):
        return None
    var value = next.value()
    if value == 44 or value == 35 or value == 123 or value == 125 or value == 58:
        builder.append_char(35)
        return next
    if value == 10:
        return next
    while True:
        next = reader.take()
        if not next or reader.eof(next.value()):
            return None
        if next.value() == 10:
            return next


def scan_default(mut reader: DefaultCharacters, mut builder: DefaultBuilder) raises:
    var quote: UInt8 = 0
    var in_comment = False
    var in_block = False
    var block = CommandBlock(1, 0, False, False, 0, False, True, 1, 1)
    var command_line: UInt32 = 1
    var command_column: UInt32 = 1
    var reprocess: Optional[UInt32] = None
    var in_tilde = False
    var tilde_name = List[UInt32]()
    var tilde_length = 0
    var tilde_line: UInt32 = 0
    var tilde_column: UInt32 = 0
    var tilde_quote: UInt8 = 0
    var last_state = -1
    var byte_eof_seen = False
    var byte_hard_eof = False
    while not builder.aborted():
        var next: Optional[UInt32]
        if reprocess:
            next = reprocess.take()
        else:
            next = reader.take()
        if not next:
            break
        var character = next.value()
        if in_tilde:
            if reader.eof(character):
                builder.expand_home(word_text(tilde_name), tilde_line, tilde_column)
                in_tilde = False
                last_state = Int(tilde_quote)
            elif character == 47 or character == 32 or character == 9 or character == 10 or character == 34 or character == 39:
                builder.expand_home(word_text(tilde_name), tilde_line, tilde_column)
                in_tilde = False
                last_state = Int(tilde_quote)
                reprocess = character
            else:
                var width = 1 if reader.encoded or character < 128 else 2 if character < 2048 else 3 if character < 65536 else 4
                if tilde_length + width > 1022:
                    builder.diagnostic(tilde_line, tilde_column, "user name is too long")
                else:
                    tilde_name.append(character)
                    tilde_length += width
            continue
        if in_block:
            if reader.eof(character):
                block.saw_eof = True
                if block.comment:
                    builder.word.shrink(block.comment_start)
                    block.comment = False
                    block.word_start = True
                    continue
                if block.escaped:
                    byte_hard_eof = True
                    break
                if not block.word_start or block.quote != 0:
                    builder.append_char(32)
                    block.quote = 0
                    block.word_start = True
                elif byte_eof_seen:
                    byte_hard_eof = True
                    break
                else:
                    builder.append_char(10)
                    block.word_start = True
                    byte_eof_seen = True
                continue
            builder.append_char(character)
            if character == 10:
                reader.line += 1
                reader.column = 0
            if block.feed(character, builder):
                in_block = False
                builder.simple_finish_word()
                last_state = -1
            continue
        if in_comment:
            if reader.eof(character):
                in_comment = False
                continue
            if character == 10:
                builder.finish_statement(command_line, command_column, reader.line)
                in_comment = False
                reader.line += 1
                reader.column = 0
                command_line = reader.line
                command_column = 1
                last_state = -1
            continue
        if reader.encoded and quote == 0 and not (builder.word_started and builder.percent_word) and character == 13 and reader.peek() == Optional[UInt32](10):
            continue
        if reader.eof(character):
            if builder.word_started:
                builder.finish_word(command_line, command_column)
                quote = 0
                last_state = -1
                continue
            if byte_eof_seen:
                byte_hard_eof = True
                break
            builder.finish_statement(command_line, command_column, reader.line)
            byte_eof_seen = True
            command_line = reader.line
            command_column = reader.column + 1
            last_state = -1
            continue
        if character == 92 and quote != 1:
            var escape_line = reader.line
            var escape_column = reader.column
            try:
                var value = escape(reader)
                if value:
                    if not builder.word_started and len(builder.words) == 0:
                        command_line = escape_line
                        command_column = escape_column
                    if not builder.word_started:
                        builder.percent_word = False
                    builder.word_started = True
                    if value.value()[1]:
                        if reader.encoded:
                            var raw = value.value()[0]
                            builder.append_char(raw if raw < 128 else CONFIG_BYTE_BASE + raw)
                        else:
                            builder.needs_bytes = True
                            builder.append_char(value.value()[0])
                    else:
                        builder.literal_char(value.value()[0])
                    last_state = Int(quote)
            except error:
                builder.diagnostic(escape_line, escape_column, String(error))
            continue
        if not reader.encoded and character == 92 and quote == 1 and reader.peek() == Optional[UInt32](10):
            _ = reader.take()
            reader.line += 1
            reader.column = 0
            continue
        if character == 36 and quote != 1:
            if not builder.word_started and len(builder.words) == 0:
                command_line = reader.line
                command_column = reader.column
            if not builder.word_started:
                builder.percent_word = False
            builder.word_started = True
            try:
                var value = expand_variable(reader, builder)
                builder.extend_word(value)
            except error:
                builder.diagnostic(reader.line, reader.column, String(error))
            last_state = Int(quote)
            continue
        if character == 126 and quote != 1 and last_state != Int(quote):
            if not builder.word_started and len(builder.words) == 0:
                command_line = reader.line
                command_column = reader.column
            if not builder.word_started:
                builder.percent_word = False
            builder.word_started = True
            in_tilde = True
            tilde_name.clear()
            tilde_length = 0
            tilde_line = reader.line
            tilde_column = reader.column
            tilde_quote = quote
            continue
        if quote == 1 and character == 39 or quote == 2 and character == 34:
            quote = 0
        elif quote != 0:
            builder.append_char(character)
            if character == 10:
                reader.line += 1
                reader.column = 0
                reprocess = quoted_prefix(reader, builder)
            else:
                last_state = Int(quote)
        elif character == 39 or character == 34:
            if not builder.word_started and len(builder.words) == 0:
                command_line = reader.line
                command_column = reader.column
            if not builder.word_started:
                builder.percent_word = False
            builder.word_started = True
            quote = UInt8(1 if character == 39 else 2)
        elif character == 35:
            var previous = word_text(builder.words[len(builder.words) - 1]) if len(builder.words) > 0 else String()
            if not builder.word_started and (previous == "%if" or previous == "%elif") and reader.peek() == Optional[UInt32](123):
                var format_line = reader.line
                var format_column = reader.column
                builder.percent_word = False
                builder.word_started = True
                try:
                    condition_format(reader, builder)
                except error:
                    builder.diagnostic(format_line, format_column, String(error))
            elif not builder.word_started and (previous == "%else" or previous == "%endif") and reader.peek() == Optional[UInt32](123):
                builder.diagnostic(reader.line, reader.column, "syntax error")
            elif not builder.word_started:
                in_comment = True
            else:
                builder.append_char(character)
                last_state = 0
        elif character == 123 and not builder.word_started:
            if len(builder.words) == 0:
                command_line = reader.line
                command_column = reader.column
            builder.percent_word = False
            builder.word_started = True
            builder.word_block = True
            builder.append_char(123)
            in_block = True
            block = CommandBlock(1, 0, False, False, 0, False, True, reader.line, reader.column)
        elif character == 125:
            builder.diagnostic(reader.line, reader.column, "syntax error")
        elif character == 59 or character == 10:
            builder.finish_statement(command_line, command_column, reader.line)
            if character == 10:
                reader.line += 1
                reader.column = 0
            command_line = reader.line
            command_column = reader.column + 1
            last_state = -1
        elif builder.space(character):
            builder.finish_word(command_line, command_column)
            last_state = -1
        else:
            if len(builder.words) == 0 and not builder.word_started:
                command_line = reader.line
                command_column = reader.column
            if not builder.word_started:
                builder.percent_word = character == 37
            builder.word_started = True
            builder.append_char(character)
            last_state = 0
    if not builder.aborted():
        if byte_hard_eof:
            if in_block:
                builder.diagnostic(reader.line if block.saw_eof else block.line, reader.column if block.saw_eof else block.column, "syntax error")
            elif builder.word_started or len(builder.words) > 0 or in_tilde:
                builder.diagnostic(reader.line, reader.column, "syntax error")
        else:
            if in_tilde:
                builder.expand_home(word_text(tilde_name), tilde_line, tilde_column)
            if not builder.aborted():
                if in_block:
                    if reader.encoded and block.saw_eof:
                        builder.diagnostic(reader.line, reader.column, "syntax error")
                    else:
                        builder.diagnostic(block.line, block.column, "unterminated command block")
                else:
                    builder.finish_statement(command_line, command_column, reader.line)
    builder.finish(reader.line, reader.column)
