from std.collections import Dict
from .protocol import Decoder
from .tree_wire import read_text
from .exec_protocol import Invocation
from .prefix_bindings import PrefixBinding, PrefixTable, PrefixBudget, read_prefix_tables, MAX_PREFIX_TABLES
from .cli_preflight import CatalogEntry, CliPreflight, _catalog, _resolve
from .palette_text import palette_trim, palette_words
from .key_spelling import canonical_protocol_key
from .which_key_data import WHICH_KEY_STOCK, WHICH_KEY_IMPLEMENTED


@fieldwise_init
struct WhichKeySet(Copyable, Movable):
    var keys: List[String]
    var yours: Bool


@fieldwise_init
struct WhichKeyRow(Copyable, Movable):
    var keys: List[WhichKeySet]
    var label: String
    var group: UInt8
    var repeat: Bool
    var core: Bool

    def first_key(self) -> String:
        if len(self.keys) == 0 or len(self.keys[0].keys) == 0:
            return String()
        return self.keys[0].keys[0].copy()

    def yours(self) -> Bool:
        for value in self.keys:
            if value.yours:
                return True
        return False


def which_key_group_title(group: UInt8) raises -> String:
    var titles: List[String] = ["Panes", "Windows", "Sessions", "Copy and paste", "Other", "Yours"]
    if group == 0 or group > 6:
        raise Error("unknown which-key group")
    return titles[Int(group) - 1].copy()


@fieldwise_init
struct _Class(Copyable, Movable):
    var group: UInt8
    var rank: UInt8
    var core: Bool
    var label: String
    var family: String
    var family_label: String
    var slot: UInt8


def _class(group: Int, rank: Int, core: Bool, label: String, family: String = "", family_label: String = "", slot: Int = 0) -> _Class:
    return _Class(UInt8(group), UInt8(rank), core, label, family, family_label, UInt8(slot))


def _uint(text: String, maximum: Int) -> Optional[Int]:
    var bytes = text.as_bytes()
    var start = 1 if len(bytes) > 0 and bytes[0] == 43 else 0
    if start == len(bytes):
        return None
    var value = 0
    for index in range(start, len(bytes)):
        if bytes[index] < 48 or bytes[index] > 57 or value > (maximum - Int(bytes[index] - 48)) // 10:
            return None
        value = value * 10 + Int(bytes[index] - 48)
    return value


def _has(args: List[String], flag: String) -> Bool:
    for arg in args:
        if arg == flag:
            return True
    return False


def _same_bytes(left: List[UInt8], right: List[UInt8]) -> Bool:
    if len(left) != len(right):
        return False
    for index in range(len(left)):
        if left[index] != right[index]:
            return False
    return True


def _same_commands(left: List[Invocation], right: List[Invocation]) -> Bool:
    if len(left) != len(right):
        return False
    for index in range(len(left)):
        ref a = left[index]
        ref b = right[index]
        if a.name != b.name or len(a.args) != len(b.args) or Bool(a.source) != Bool(b.source) or len(a.blocks) != len(b.blocks) or a.expanded_alias != b.expanded_alias or Bool(a.stdin) != Bool(b.stdin) or a.stdin_available != b.stdin_available:
            return False
        for word in range(len(a.args)):
            if not _same_bytes(a.args[word], b.args[word]):
                return False
        for block in range(len(a.blocks)):
            if a.blocks[block] != b.blocks[block]:
                return False
        if a.source:
            ref sa = a.source.value()
            ref sb = b.source.value()
            if sa.source != sb.source or sa.line != sb.line or sa.column != sb.column:
                return False
        if a.stdin and not _same_bytes(a.stdin.value(), b.stdin.value()):
            return False
    return True


def _shape(key: String) -> Tuple[String, Bool]:
    var bytes = key.as_bytes()
    var start = 0
    while len(bytes) - start > 2 and bytes[start + 1] == 45 and (bytes[start] == 67 or bytes[start] == 77 or bytes[start] == 83):
        start += 2
    var tail = String(from_utf8_lossy=bytes[start:])
    var scalars = 0
    for _ in tail.codepoints():
        scalars += 1
    return (String(from_utf8_lossy=bytes[:start]), scalars == 1)


@fieldwise_init
struct _Entry(Copyable, Movable):
    var order: Int
    var key: String
    var classed: _Class
    var yours: Bool
    var repeat: Bool


@fieldwise_init
struct _Family(Copyable, Movable):
    var head: _Entry
    var members: List[Tuple[UInt8, String]]
    var repeat: Bool


@fieldwise_init
struct _Row(Copyable, Movable):
    var order: Int
    var rank: UInt8
    var row: WhichKeyRow


@inline(.never)
def _merge(entries: List[_Entry], grouped: Bool) -> List[WhichKeyRow]:
    var families = List[_Family]()
    for entry in entries:
        var joined = -1
        if entry.classed.family.byte_length() != 0:
            var shape = _shape(entry.key)
            for index in range(len(families)):
                ref head = families[index].head
                var other = _shape(head.key)
                if head.classed.group == entry.classed.group and head.yours == entry.yours and head.classed.family == entry.classed.family and shape[0] == other[0] and shape[1] == other[1]:
                    joined = index
                    break
        if joined >= 0:
            ref family = families[joined]
            family.members.append((entry.classed.slot, entry.key.copy()))
            family.repeat &= entry.repeat
            family.head.classed.core |= entry.classed.core
        else:
            var members: List[Tuple[UInt8, String]] = [(entry.classed.slot, entry.key.copy())]
            families.append(_Family(entry.copy(), members^, entry.repeat))
    var merged = List[_Row]()
    for family in families:
        var members = family.members.copy()
        for index in range(1, len(members)):
            var position = index
            while position > 0 and members[position][0] < members[position - 1][0]:
                var temporary = members[position - 1].copy()
                members[position - 1] = members[position].copy()
                members[position] = temporary^
                position -= 1
        ref head = family.head
        var label = head.classed.family_label.copy() if head.classed.family.byte_length() != 0 and len(members) > 1 else head.classed.label.copy()
        var keys = List[String]()
        for member in members:
            keys.append(member[1].copy())
        var set = WhichKeySet(keys^, head.yours)
        var joined = -1
        for index in range(len(merged)):
            if merged[index].row.group == head.classed.group and merged[index].row.label == label:
                joined = index
                break
        if joined >= 0:
            ref item = merged[joined]
            if head.yours:
                item.row.keys.insert(0, set^)
            else:
                item.row.keys.append(set^)
            item.row.repeat &= family.repeat
            item.row.core |= head.classed.core
            item.order = min(item.order, head.order)
            item.rank = min(item.rank, head.classed.rank)
        else:
            var sets: List[WhichKeySet] = [set^]
            merged.append(_Row(head.order, head.classed.rank, WhichKeyRow(sets^, label^, head.classed.group, family.repeat, head.classed.core)))
    for index in range(1, len(merged)):
        var position = index
        while position > 0 and merged[position].order < merged[position - 1].order:
            var temporary = merged[position - 1].copy()
            merged[position - 1] = merged[position].copy()
            merged[position] = temporary^
            position -= 1
    if grouped:
        for index in range(1, len(merged)):
            var position = index
            while position > 0:
                ref a = merged[position]
                ref b = merged[position - 1]
                if a.row.group > b.row.group or a.row.group == b.row.group and a.rank >= b.rank:
                    break
                var temporary = b.copy()
                merged[position - 1] = merged[position].copy()
                merged[position] = temporary^
                position -= 1
    var result = List[WhichKeyRow]()
    for item in merged:
        result.append(item.row.copy())
    return result^


struct WhichKeyModel(Movable):
    var _stock: List[PrefixBinding]
    var _catalog: List[CatalogEntry]
    var _implemented: Dict[String, Bool]

    def __init__(out self) raises:
        var data = String(WHICH_KEY_STOCK).as_bytes()
        var bytes = List[UInt8]()
        for index in range(0, len(data), 2):
            var value: UInt8 = 0
            for part in range(2):
                var digit = data[index + part]
                value = value * 16 + (digit - 48 if digit <= 57 else digit - 87)
            bytes.append(value)
        var reader = Decoder(bytes^)
        reader.offset = 0
        if reader.uint() != 10 or reader.uint() != 0 or reader.uint() != 25:
            raise Error("invalid built-in which-key snapshot")
        var tables = read_prefix_tables(reader)
        reader.finish()
        if len(tables) != 1 or tables[0].name != "prefix":
            raise Error("invalid built-in which-key table")
        self._stock = tables[0].bindings.copy()
        self._catalog = _catalog()
        self._implemented = Dict[String, Bool]()
        for name in String(WHICH_KEY_IMPLEMENTED).split("\n"):
            self._implemented[String(name)] = True

    def canonical(self, name: String) -> String:
        var status = CliPreflight()
        var index = _resolve(self._catalog, name, status)
        return self._catalog[index].name.copy() if index >= 0 else name.copy()

    def opens_all_keys(self, commands: List[Invocation]) -> Bool:
        if len(commands) != 1 or len(commands[0].args) != 1:
            return False
        ref arg = commands[0].args[0]
        return self.canonical(commands[0].name) == "list-keys" and len(arg) == 2 and arg[0] == 45 and arg[1] == 78

    def _inner(self, command: Invocation) -> Tuple[String, List[String], Bool]:
        var name = self.canonical(command.name)
        var args = List[String]()
        for arg in command.args:
            args.append(String(from_utf8_lossy=Span(arg)))
        if (name == "confirm-before" or name == "command-prompt") and len(args) > 0:
            var text = palette_trim(args[len(args) - 1])
            var bytes = text.as_bytes()
            var first = 0
            var last = len(bytes)
            while first < last and bytes[first] == 123:
                first += 1
            while last > first and bytes[last - 1] == 125:
                last -= 1
            var words = palette_words(String(from_utf8_lossy=bytes[first:last]))
            if len(words) > 0 and words[0] in self._implemented:
                var inner = self.canonical(words[0])
                _ = words.pop(0)
                return (inner^, words^, name == "command-prompt")
        return (name^, args^, False)

    @inline(.never)
    def _classify(self, commands: List[Invocation]) -> Optional[_Class]:
        if len(commands) == 0:
            return None
        if len(commands) > 1 and self.canonical(commands[0].name) == "new-pane" and self.canonical(commands[len(commands) - 1].name) == "switch-mode":
            var windows = False
            for arg in commands[len(commands) - 1].args:
                var text = String(from_utf8_lossy=Span(arg))
                windows |= text.startswith("-") and "w" in text
            return _class(2 if windows else 3, 11 if windows else 9, False, "Switch to a window" if windows else "Switch to a session")
        for index in range(1, len(commands)):
            if self.canonical(commands[index].name) != "display-message":
                return None
        var inner = self._inner(commands[0])
        var name = inner[0].copy()
        var args = inner[1].copy()
        var prompted = inner[2]
        var direction = -1
        var sides: List[String] = ["left", "down", "up", "right"]
        var flags: List[String] = ["-L", "-D", "-U", "-R"]
        for arg in args:
            for slot in range(4):
                if arg == flags[slot]:
                    direction = slot
                    break
            if direction >= 0:
                break
        var kind = Optional[String]()
        for index in range(len(args)):
            if args[index] == "--kind":
                if index + 1 < len(args):
                    kind = args[index + 1].copy()
                break
        if not kind:
            for arg in args:
                if arg.startswith("--kind="):
                    kind = String(from_utf8_lossy=arg.as_bytes()[7:])
                    break
        if kind and (kind.value() == "terminal" or kind.value() == "picker"):
            kind = None
        if name == "split-window":
            var horizontal = _has(args, "-h")
            var label = String("Split ", "right" if horizontal else "down")
            if kind:
                label += String(" (", kind.value(), ")")
            return _class(1, 0 if horizontal else 1, not Bool(kind), label^)
        if name == "select-pane":
            if prompted and _has(args, "-T"):
                return _class(1, 19, False, "Change the pane title")
            if _has(args, "-m"):
                return _class(1, 17, False, "Mark pane")
            if _has(args, "-M"):
                return _class(1, 18, False, "Clear marked pane")
            for arg in args:
                if arg.endswith(".+"):
                    return _class(1, 5, True, "Next pane")
            if len(args) == 1 and direction >= 0:
                return _class(1, 2, True, "Focus pane " + sides[direction], "focus", "Focus pane", direction)
            return None
        if name == "resize-pane":
            if _has(args, "-Z"):
                return _class(1, 3, True, "Zoom pane")
            if direction < 0:
                return None
            var cells = String("1")
            for arg in args:
                if _uint(arg, 65535):
                    cells = arg.copy()
                    break
            var suffix = String() if cells == "1" else String(" by ", cells)
            return _class(1, 7 if cells == "1" else 8, False, String("Resize pane ", sides[direction], suffix), "resize " + cells, "Resize pane" + suffix, direction)
        if name == "swap-pane" and (_has(args, "-U") or _has(args, "-D")):
            var up = _has(args, "-U")
            return _class(1, 10, False, "Swap pane up" if up else "Swap pane down", "swap", "Swap pane", 0 if up else 1)
        if name == "if-shell":
            for arg in args:
                if "pane_floating_flag" in arg:
                    return _class(1, 21, False, "Float or tile pane")
            return None
        if name == "switch-client":
            if _has(args, "-T"):
                return _class(1, 22, False, "Move a floating pane")
            if _has(args, "-n") or _has(args, "-p"):
                var next = _has(args, "-n")
                return _class(3, 4, False, "Next session" if next else "Previous session", "session", "Next / previous session", 0 if next else 1)
            if _has(args, "-l"):
                return _class(3, 5, False, "Last session")
            return None
        if name == "rotate-window":
            var back = _has(args, "-D")
            return _class(1, 13 if back else 12, False, "Rotate panes back" if back else "Rotate panes")
        if name == "select-layout":
            if _has(args, "-E"):
                return _class(1, 15, False, "Spread panes evenly")
            var layouts: List[String] = ["even-horizontal", "even-vertical", "main-horizontal", "main-vertical", "tiled", "main-horizontal-mirrored", "main-vertical-mirrored"]
            for arg in args:
                if not arg.startswith("-"):
                    for slot in range(len(layouts)):
                        if arg == layouts[slot]:
                            return _class(1, 16, False, "Layout " + arg, "layout", "Layouts", slot)
                    return None
            return None
        if name == "next-window" or name == "previous-window":
            var next = name == "next-window"
            var alert = _has(args, "-a")
            var label = String("Next" if next else "Previous", " window", " with alert" if alert else "")
            return _class(2, 7 if alert else 1, not alert, label^, "alert" if alert else "cycle", "Next / previous alert" if alert else "Next / previous window", 0 if next else 1)
        if name == "select-window":
            if prompted:
                return _class(2, 8, False, "Go to window by index")
            for position in range(len(args)):
                if args[position] == "-t":
                    if position + 1 == len(args):
                        return None
                    var bytes = args[position + 1].as_bytes()
                    var start = 0
                    while start < len(bytes) and (bytes[start] == 58 or bytes[start] == 61):
                        start += 1
                    var index = _uint(String(from_utf8_lossy=bytes[start:]), 255)
                    if index:
                        return _class(2, 2, True, String("Go to window ", index.value()), "index", "Go to window", index.value())
                    return None
            return None
        if name == "choose-tree":
            for arg in args:
                if arg.byte_length() > 1 and arg.startswith("-") and not arg.startswith("--") and "w" in arg:
                    return _class(2, 3, True, "Pick window")
            return _class(3, 0, True, "Pick session")
        if name == "detach-client":
            var others = _has(args, "-a")
            return _class(3, 6 if others else 1, not others, "Detach other clients" if others else "Detach")
        if name == "copy-mode":
            var up = _has(args, "-u")
            return _class(4, 2 if up else 0, not up, "Copy mode, page up" if up else "Copy mode")
        if name == "display-message":
            return _class(5, 5, False, "Show pane info") if len(args) == 0 else None
        var names: List[String] = ["kill-pane", "last-pane", "break-pane", "display-panes", "new-pane", "next-layout", "new-window", "last-window", "rename-window", "kill-window", "find-window", "move-window", "rename-session", "new-session", "choose-client", "kill-session", "paste-buffer", "choose-buffer", "list-buffers", "delete-buffer", "command-prompt", "choose-path", "list-keys", "reload-config", "source-file", "send-last-output", "show-messages", "send-prefix", "clock-mode"]
        var labels: List[String] = ["Kill pane", "Last pane", "Break pane to a window", "Show pane numbers", "New floating pane", "Next layout", "New window", "Last window", "Rename window", "Kill window", "Find window", "Move window", "Rename session", "New session", "Pick a client", "Kill session", "Paste", "Pick a paste buffer", "List paste buffers", "Delete a paste buffer", "Command prompt", "Insert a path", "All keys", "Reload config", "Reload config", "Send output to an agent", "Show messages", "Send the prefix", "Clock"]
        var groups: List[UInt8] = [1,1,1,1,1,1,2,2,2,2,2,2,3,3,3,3,4,4,4,4,5,5,5,5,5,5,5,5,5]
        var ranks: List[UInt8] = [4,6,9,11,20,14,0,4,5,6,9,10,2,3,7,8,1,3,4,5,0,1,2,3,3,4,6,7,8]
        var cores: List[Bool] = [True,True,False,False,False,False,True,True,True,True,False,False,True,True,False,False,True,False,False,False,True,True,False,False,False,False,False,False,False]
        for index in range(len(names)):
            if name == names[index]:
                return _class(Int(groups[index]), Int(ranks[index]), cores[index], labels[index].copy())
        return None

    def _raw(self, commands: List[Invocation]) -> String:
        var label = String()
        var count = 0
        for index in range(len(commands)):
            ref command = commands[index]
            if index > 0 and self.canonical(command.name) == "display-message":
                continue
            if count > 0:
                label += " ; "
            label += command.name
            for arg in command.args:
                label += " "
                label += String(from_utf8_lossy=Span(arg))
            count += 1
        return label^

    @inline(.never)
    def rows(self, tables: List[PrefixTable], table: String, prefix: String, families: Bool = True) raises -> List[WhichKeyRow]:
        if len(tables) > MAX_PREFIX_TABLES:
            raise Error("which-key tables exceed count limit")
        var budget = PrefixBudget()
        budget.text(table)
        budget.text(prefix)
        var found = -1
        for index in range(len(tables)):
            budget.table(tables[index])
            if found < 0 and tables[index].name == table:
                found = index
        if found < 0:
            return List[WhichKeyRow]()
        var grouped = table == "prefix"
        var stock = self._stock.copy() if grouped else List[PrefixBinding]()
        if grouped:
            var next = canonical_protocol_key(prefix)
            if next != "C-b":
                for index in range(len(stock)):
                    if stock[index].key == "C-b":
                        var previous = stock.pop(index)
                        var occupied = False
                        for item in stock:
                            occupied |= item.key == next
                        if not occupied:
                            previous.key = next^
                            stock.append(previous^)
                        break
        var entries = List[_Entry]()
        for binding in tables[found].bindings:
            if binding.key == "None" or "Mouse" in binding.key or "Wheel" in binding.key or "Click" in binding.key:
                continue
            var original = -1
            for index in range(len(stock)):
                if stock[index].key == binding.key:
                    original = index
                    break
            var yours = grouped and (original < 0 or stock[original].repeat != binding.repeat or not _same_commands(stock[original].commands, binding.commands))
            var override = False
            if binding.note and binding.note.value().byte_length() != 0:
                override = original < 0 or not stock[original].note or stock[original].note.value() != binding.note.value()
            var classed = self._classify(binding.commands)
            var value = classed.take() if classed else _class(6, 255, False, self._raw(binding.commands))
            if not grouped:
                value.group = 0
            value.core |= yours
            if override:
                value.label = binding.note.value().copy()
                value.family = String()
            if not families:
                value.family = String()
            entries.append(_Entry(len(entries), binding.key.copy(), value^, yours, binding.repeat))
        return _merge(entries, grouped)
