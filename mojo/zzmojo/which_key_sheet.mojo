from .which_key import WhichKeyModel, which_key_group_title
from .which_key_caps import WhichKeyCap, WhichKeyStroke, which_key_cap, which_key_stroke
from .prefix_bindings import PrefixTable


@fieldwise_init
struct WhichKeyVisualRow(Copyable, Movable):
    var id: String
    var caps: List[WhichKeyCap]
    var label: String
    var group: UInt8
    var repeat: Bool


@fieldwise_init
struct WhichKeySheet(Copyable, Movable):
    var table: String
    var prefix_raw: String
    var prefix: Optional[WhichKeyStroke]
    var more: Optional[WhichKeyCap]
    var rows: List[WhichKeyVisualRow]


@inline(.never)
def which_key_sheet(model: WhichKeyModel, tables: List[PrefixTable], table: String, prefix: Optional[String], all: Bool) raises -> Optional[WhichKeySheet]:
    var short = table == "prefix" and not all
    var source = model.rows(tables, table, prefix.value() if prefix else String("C-b"))
    var rows = List[WhichKeyVisualRow]()
    for row in source:
        if short and not row.core:
            continue
        if row.label.byte_length() > 65535:
            raise Error("which-key label exceeds display limit")
        var caps = List[WhichKeyCap]()
        for set in row.keys:
            var cap = which_key_cap(set.keys, set.yours)
            if cap.text().byte_length() > 65535:
                raise Error("which-key cap exceeds display limit")
            caps.append(cap^)
        rows.append(WhichKeyVisualRow(row.first_key(), caps^, row.label.copy(), row.group, row.repeat))
    if len(rows) == 0:
        return None
    var more = Optional[WhichKeyCap]()
    if short:
        for item in tables:
            if item.name == "prefix":
                for binding in item.bindings:
                    if model.opens_all_keys(binding.commands):
                        var keys: List[String] = [binding.key.copy()]
                        more = which_key_cap(keys, False)
                        break
                break
    var raw = prefix.value().copy() if table == "prefix" and prefix else String()
    if raw.byte_length() > 65535:
        raise Error("which-key prefix exceeds display limit")
    return WhichKeySheet(table.copy(), raw.copy(), which_key_stroke(raw), more^, rows^)
