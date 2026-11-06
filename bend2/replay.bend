import Base
import ./core.bend as Core

def compare_words(high: Cmp, low: Cmp) -> Cmp:
  match high:
    case EQ{}:
      low
    case LT{}:
      LT{}
    case GT{}:
      GT{}

def sequence_cmp(a: Core.StableId, b: Core.StableId) -> Cmp:
  match a b:
    case Core.Id{ah, al} Core.Id{bh, bl}:
      compare_words(U32.cmp(ah, bh), U32.cmp(al, bl))

def sequence_ge(a: Core.StableId, b: Core.StableId) -> Bool:
  Cmp.is_ge(sequence_cmp(a, b))

def sequence_max.choose(a: Core.StableId, b: Core.StableId, order: Cmp) -> Core.StableId:
  match order:
    case GT{}:
      a
    case _:
      b

def sequence_max(+a: Core.StableId, +b: Core.StableId) -> Core.StableId:
  sequence_max.choose(a, b, sequence_cmp(a, b))

def next_sequence.choose(high: U32, low: U32, carry: Bool, saturated: Bool) -> Core.StableId:
  match carry saturated:
    case True{} True{}:
      Core.Id{4294967295, 4294967295}
    case True{} False{}:
      Core.Id{U32.add(high, 1), 0}
    case False{} _:
      Core.Id{high, U32.add(low, 1)}

def next_sequence(value: Core.StableId) -> Core.StableId:
  match value:
    case Core.Id{+high, +low}:
      next_sequence.choose(high, low, U32.is_eq(low, 4294967295), U32.is_eq(high, 4294967295))

def add_bytes.choose(high: U32, low: U32, carry: Bool, saturated: Bool) -> Core.StableId:
  match carry saturated:
    case True{} True{}:
      Core.Id{4294967295, 4294967295}
    case True{} False{}:
      Core.Id{U32.add(high, 1), low}
    case False{} _:
      Core.Id{high, low}

def add_bytes(value: Core.StableId, count: U32) -> Core.StableId:
  match value:
    case Core.Id{+high, +low}:
      +sum = U32.add(low, count)
      add_bytes.choose(high, sum, U32.is_lt(sum, low), U32.is_eq(high, 4294967295))

type JournalEntry is Data:
  Entry{sequence: Core.StableId, bytes: U32}

type ReplayItem is Data:
  JournalInvalid{position: Core.StableId}
  JournalUpdate{sequence: Core.StableId, reset: Bool, restoring: Bool, bytes: U32}

type ReplayCursor is Data:
  Cursor{last: Core.StableId, pending: Bool, journal: List<&2, JournalEntry>, bytes: Core.StableId}

type ReplayAcc is Data:
  Acc{cursor: ReplayCursor, request: Maybe<&2, Core.StableId>, invalid: Bool, accepted: Bool, halted: Bool}

def cursor_last(cursor: ReplayCursor) -> Core.StableId:
  match cursor:
    case Cursor{last, _, _, _}:
      last

def acc_last(acc: ReplayAcc) -> Core.StableId:
  match acc:
    case Acc{cursor, _, _, _, _}:
      cursor_last(cursor)

def was_halted(acc: ReplayAcc) -> Bool:
  match acc:
    case Acc{_, _, _, _, halted}:
      halted

def append_entry(journal: List<&2, JournalEntry>, entry: JournalEntry) -> List<&2, JournalEntry>:
  match journal:
    case Nil{}:
      [entry]
    case head <> tail:
      head <> append_entry(tail, entry)

def replay_request(last: Core.StableId, pending: Bool) -> Maybe<&2, Core.StableId>:
  match pending:
    case True{}:
      None{}
    case False{}:
      Some{last}

def gap(cursor: ReplayCursor, invalid: Bool, accepted: Bool) -> ReplayAcc:
  match cursor:
    case Cursor{+last, pending, journal, bytes}:
      Acc{Cursor{last, True{}, journal, bytes}, replay_request(last, pending), invalid, accepted, True{}}

def accept_update(cursor: ReplayCursor, +sequence: Core.StableId, +size: U32, reset: Bool, invalid: Bool) -> ReplayAcc:
  match cursor reset:
    case Cursor{last, pending, _, _} True{}:
      Acc{Cursor{sequence_max(last, sequence), pending, [Entry{sequence, size}], Core.Id{0, size}}, None{}, invalid, True{}, False{}}
    case Cursor{last, pending, journal, bytes} False{}:
      Acc{Cursor{sequence_max(last, sequence), pending, append_entry(journal, Entry{sequence, size}), add_bytes(bytes, size)}, None{}, invalid, True{}, False{}}

def valid_gap(cursor: ReplayCursor, sequence: Core.StableId, reset: Bool, size: U32, invalid: Bool, accepted: Bool, missing: Bool) -> ReplayAcc:
  match missing:
    case True{}:
      gap(cursor, invalid, accepted)
    case False{}:
      accept_update(cursor, sequence, size, reset, invalid)

def valid_fresh(+cursor: ReplayCursor, +sequence: Core.StableId, +reset: Bool, restoring: Bool, size: U32, invalid: Bool, accepted: Bool) -> ReplayAcc:
  valid_gap(cursor, sequence, reset, size, invalid, accepted,
    Bool.not(reset && restoring) && Cmp.is_gt(sequence_cmp(sequence, next_sequence(cursor_last(cursor)))))

def valid_item(cursor: ReplayCursor, sequence: Core.StableId, reset: Bool, restoring: Bool, size: U32, invalid: Bool, accepted: Bool, stale: Bool) -> ReplayAcc:
  match stale:
    case True{}:
      Acc{cursor, None{}, invalid, accepted, False{}}
    case False{}:
      valid_fresh(cursor, sequence, reset, restoring, size, invalid, accepted)

def invalid_item(cursor: ReplayCursor, position: Core.StableId, accepted: Bool, missing: Bool) -> ReplayAcc:
  match cursor missing:
    case _ True{}:
      gap(cursor, True{}, accepted)
    case Cursor{last, pending, journal, bytes} False{}:
      Acc{Cursor{sequence_max(last, position), pending, journal, bytes}, None{}, True{}, accepted, False{}}

def live_step(item: ReplayItem, acc: ReplayAcc) -> ReplayAcc:
  match item acc:
    case JournalInvalid{+position} Acc{+cursor, _, _, accepted, _}:
      invalid_item(cursor, position, accepted, Cmp.is_gt(sequence_cmp(position, next_sequence(cursor_last(cursor)))))
    case JournalUpdate{+sequence, reset, restoring, size} Acc{+cursor, _, invalid, accepted, _}:
      valid_item(cursor, sequence, reset, restoring, size, invalid, accepted, Cmp.is_le(sequence_cmp(sequence, cursor_last(cursor))))

def step_live(item: ReplayItem, acc: ReplayAcc, halted: Bool) -> ReplayAcc:
  match halted:
    case True{}:
      acc
    case False{}:
      live_step(item, acc)

def replay_step(item: ReplayItem, +acc: ReplayAcc) -> ReplayAcc:
  step_live(item, acc, was_halted(acc))

def fold(items: List<&2, ReplayItem>, acc: ReplayAcc) -> ReplayAcc:
  match items:
    case Nil{}:
      acc
    case head <> tail:
      fold(tail, replay_step(head, acc))

def finish(acc: ReplayAcc) -> ReplayAcc:
  match acc:
    case Acc{Cursor{last, pending, journal, bytes}, request, invalid, True{}, False{}}:
      Acc{Cursor{last, False{}, journal, bytes}, request, invalid, True{}, False{}}
    case other:
      other

def apply(items: List<&2, ReplayItem>, cursor: ReplayCursor) -> ReplayAcc:
  finish(fold(items, Acc{cursor, None{}, False{}, False{}, False{}}))
