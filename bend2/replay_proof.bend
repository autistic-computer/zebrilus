import Base
import ./core.bend as Core
import ./replay.bend as Replay
import ./order_proof.bend as Order

law cursor_ge_refl:
  for +cursor: Replay.ReplayCursor
  {Replay.sequence_ge(Replay.cursor_last(cursor), Replay.cursor_last(cursor)) == True{} : Bool}

def cursor_ge_refl(cursor):
  Order.sequence_ge_refl(Replay.cursor_last(cursor))

law gap_monotonic:
  for +cursor: Replay.ReplayCursor
  for invalid: Bool
  for accepted: Bool
  {Replay.sequence_ge(Replay.acc_last(Replay.gap(cursor, invalid, accepted)), Replay.cursor_last(cursor)) == True{} : Bool}

def gap_monotonic(cursor, invalid, accepted):
  match cursor:
    case Replay.Cursor{last, _, _, _}:
      Order.sequence_ge_refl(last)

law accept_monotonic:
  for +cursor: Replay.ReplayCursor
  for +sequence: Core.StableId
  for size: U32
  for reset: Bool
  for invalid: Bool
  {Replay.sequence_ge(Replay.acc_last(Replay.accept_update(cursor, sequence, size, reset, invalid)), Replay.cursor_last(cursor)) == True{} : Bool}

def accept_monotonic(cursor, sequence, size, reset, invalid):
  match cursor reset:
    case Replay.Cursor{last, _, _, _} True{}:
      Order.max_monotonic(last, sequence)
    case Replay.Cursor{last, _, _, _} False{}:
      Order.max_monotonic(last, sequence)

law valid_gap_monotonic:
  for +cursor: Replay.ReplayCursor
  for +sequence: Core.StableId
  for reset: Bool
  for size: U32
  for invalid: Bool
  for accepted: Bool
  for missing: Bool
  {Replay.sequence_ge(Replay.acc_last(Replay.valid_gap(cursor, sequence, reset, size, invalid, accepted, missing)), Replay.cursor_last(cursor)) == True{} : Bool}

def valid_gap_monotonic(cursor, sequence, reset, size, invalid, accepted, missing):
  match missing:
    case True{}:
      gap_monotonic(cursor, invalid, accepted)
    case False{}:
      accept_monotonic(cursor, sequence, size, reset, invalid)

law valid_fresh_monotonic:
  for +cursor: Replay.ReplayCursor
  for +sequence: Core.StableId
  for +reset: Bool
  for +restoring: Bool
  for size: U32
  for invalid: Bool
  for accepted: Bool
  {Replay.sequence_ge(Replay.acc_last(Replay.valid_fresh(cursor, sequence, reset, restoring, size, invalid, accepted)), Replay.cursor_last(cursor)) == True{} : Bool}

def valid_fresh_monotonic(cursor, sequence, reset, restoring, size, invalid, accepted):
  valid_gap_monotonic(cursor, sequence, reset, size, invalid, accepted,
    Bool.not(reset && restoring) && Cmp.is_gt(Replay.sequence_cmp(sequence, Replay.next_sequence(Replay.cursor_last(cursor)))))

law valid_item_monotonic:
  for +cursor: Replay.ReplayCursor
  for +sequence: Core.StableId
  for reset: Bool
  for restoring: Bool
  for size: U32
  for invalid: Bool
  for accepted: Bool
  for stale: Bool
  {Replay.sequence_ge(Replay.acc_last(Replay.valid_item(cursor, sequence, reset, restoring, size, invalid, accepted, stale)), Replay.cursor_last(cursor)) == True{} : Bool}

def valid_item_monotonic(cursor, sequence, reset, restoring, size, invalid, accepted, stale):
  match stale:
    case True{}:
      cursor_ge_refl(cursor)
    case False{}:
      valid_fresh_monotonic(cursor, sequence, reset, restoring, size, invalid, accepted)

law invalid_monotonic:
  for +cursor: Replay.ReplayCursor
  for position: Core.StableId
  for accepted: Bool
  for missing: Bool
  {Replay.sequence_ge(Replay.acc_last(Replay.invalid_item(cursor, position, accepted, missing)), Replay.cursor_last(cursor)) == True{} : Bool}

def invalid_monotonic(cursor, position, accepted, missing):
  match cursor missing:
    case Replay.Cursor{last, pending, journal, bytes} True{}:
      Order.sequence_ge_refl(last)
    case Replay.Cursor{last, _, _, _} False{}:
      Order.max_monotonic(last, position)

law live_step_monotonic:
  for +item: Replay.ReplayItem
  for +acc: Replay.ReplayAcc
  {Replay.sequence_ge(Replay.acc_last(Replay.live_step(item, acc)), Replay.acc_last(acc)) == True{} : Bool}

def live_step_monotonic(item, acc):
  match item acc:
    case Replay.JournalInvalid{position} Replay.Acc{cursor, _, _, accepted, _}:
      invalid_monotonic(cursor, position, accepted, Cmp.is_gt(Replay.sequence_cmp(position, Replay.next_sequence(Replay.cursor_last(cursor)))))
    case Replay.JournalUpdate{sequence, reset, restoring, size} Replay.Acc{cursor, _, invalid, accepted, _}:
      valid_item_monotonic(cursor, sequence, reset, restoring, size, invalid, accepted, Cmp.is_le(Replay.sequence_cmp(sequence, Replay.cursor_last(cursor))))

law step_live_monotonic:
  for +item: Replay.ReplayItem
  for +acc: Replay.ReplayAcc
  for halted: Bool
  {Replay.sequence_ge(Replay.acc_last(Replay.step_live(item, acc, halted)), Replay.acc_last(acc)) == True{} : Bool}

def step_live_monotonic(item, acc, halted):
  match halted:
    case True{}:
      Order.sequence_ge_refl(Replay.acc_last(acc))
    case False{}:
      live_step_monotonic(item, acc)
