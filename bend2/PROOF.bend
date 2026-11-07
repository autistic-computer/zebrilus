import Base
import ./core.bend as Core
import ./LAWS.bend as Laws
import ./id_proof.bend as IdProof
import ./replay.bend as Replay
import ./replay_proof.bend as ReplayProof
import ./order_proof.bend as Order
import ./terminal_proof.bend as Terminal
import ./terminal_cells_proof.bend as Cells
import ./terminal_cells_origins_proof.bend as CellOrigins
import ./tree_proof.bend as Tree
import ./input_router_proof.bend as InputRouter

def Laws.swap_preserves_count(node, source, target):
  match node:
    case Core.Empty{}:
      {==}
    case Core.Pane{_}:
      {==}
    case Core.Split{id, axis, ratio, first, second}:
      %Laws.swap_preserves_count(first, source, target) :
        {Nat.add(Core.pane_count(Core.swapped_layout(first, source, target)), Core.pane_count(Core.swapped_layout(second, source, target))) == Nat.add(_, Core.pane_count(second)) : Nat}
      %Laws.swap_preserves_count(second, source, target) :
        {Nat.add(Core.pane_count(Core.swapped_layout(first, source, target)), Core.pane_count(Core.swapped_layout(second, source, target))) == Nat.add(Core.pane_count(Core.swapped_layout(first, source, target)), _) : Nat}
      {==}

def Laws.swap_preserves_shape(node, source, target):
  match node:
    case Core.Empty{}:
      {==}
    case Core.Pane{_}:
      {==}
    case Core.Split{id, axis, ratio, first, second}:
      %Laws.swap_preserves_shape(first, source, target) :
        {Core.SplitShape{id, axis, ratio, Core.shape(Core.swapped_layout(first, source, target)), Core.shape(Core.swapped_layout(second, source, target))} == Core.SplitShape{id, axis, ratio, _, Core.shape(second)} : Core.LayoutShape}
      %Laws.swap_preserves_shape(second, source, target) :
        {Core.SplitShape{id, axis, ratio, Core.shape(Core.swapped_layout(first, source, target)), Core.shape(Core.swapped_layout(second, source, target))} == Core.SplitShape{id, axis, ratio, Core.shape(Core.swapped_layout(first, source, target)), _} : Core.LayoutShape}
      {==}

def Laws.frame_acceptance(short, large, length, header, lane, version, flags):
  match short large length header lane version flags:
    case True{} _ _ _ _ _ _:
      {==}
    case False{} True{} _ _ _ _ _:
      {==}
    case False{} False{} True{} _ _ _ _:
      {==}
    case False{} False{} False{} True{} _ _ _:
      {==}
    case False{} False{} False{} False{} True{} _ _:
      {==}
    case False{} False{} False{} False{} False{} True{} _:
      {==}
    case False{} False{} False{} False{} False{} False{} True{}:
      {==}
    case False{} False{} False{} False{} False{} False{} False{}:
      {==}

def Laws.truncated_prefix_dominates(large, length, header, lane, version, flags):
  {==}

def Laws.oversized_length_dominates(length, header, lane, version, flags):
  {==}

def Laws.version_precedes_flags(flags):
  {==}

def Laws.invalid_lane_precedes_version(version, flags):
  {==}

def Laws.permission_demands_attention(phase):
  match phase:
    case Core.PhaseStarting{}:
      {==}
    case Core.PhaseReady{}:
      {==}
    case Core.PhaseRunning{}:
      {==}
    case Core.PhaseAwaitingPermission{}:
      {==}
    case Core.PhaseFailed{}:
      {==}

def Laws.stable_attention_has_no_edge(state):
  match state:
    case Core.Idle{}:
      {==}
    case Core.Working{}:
      {==}
    case Core.NeedsInput{}:
      {==}
    case Core.AttentionFailed{}:
      {==}


def Laws.swap_is_involutive(node, source, target):
  match node:
    case Core.Empty{}:
      {==}
    case Core.Pane{id}:
      Equal.cong(Core.StableId, Core.Layout, x => Core.Pane{x}, Core.swap_id(Core.swap_id(id, source, target), source, target), id, IdProof.swap_twice(id, source, target))
    case Core.Split{id, axis, ratio, first, second}:
      %Laws.swap_is_involutive(first, source, target) :
        {Core.Split{id, axis, ratio, Core.swapped_layout(Core.swapped_layout(first, source, target), source, target), Core.swapped_layout(Core.swapped_layout(second, source, target), source, target)} == Core.Split{id, axis, ratio, _, second} : Core.Layout}
      %Laws.swap_is_involutive(second, source, target) :
        {Core.Split{id, axis, ratio, Core.swapped_layout(Core.swapped_layout(first, source, target), source, target), Core.swapped_layout(Core.swapped_layout(second, source, target), source, target)} == Core.Split{id, axis, ratio, Core.swapped_layout(Core.swapped_layout(first, source, target), source, target), _} : Core.Layout}
      {==}


def Laws.replay_step_never_rewinds(item, acc):
  ReplayProof.step_live_monotonic(item, acc, Replay.was_halted(acc))

def Laws.replay_gap_is_requested_once(last):
  {==}

def Laws.replay_gap_stops_the_batch(items, cursor, request, invalid, accepted):
  match items:
    case Nil{}:
      {==}
    case head <> tail:
      Laws.replay_gap_stops_the_batch(tail, cursor, request, invalid, accepted)


def Laws.replay_duplicate_has_no_effect(last, pending, journal, bytes, reset, restoring, size, invalid, accepted):
  %Equal.sym(Cmp, Replay.sequence_cmp(last, last), EQ{}, Order.sequence_cmp_refl(last)) :
    {Replay.valid_item(Replay.Cursor{last, pending, journal, bytes}, last, reset, restoring, size, invalid, accepted, Cmp.is_le(_)) == Replay.Acc{Replay.Cursor{last, pending, journal, bytes}, None{}, invalid, accepted, False{}} : Replay.ReplayAcc}
  {==}

def Laws.replay_gap_applies_only_the_prefix(items, cursor, invalid, accepted):
  match cursor:
    case Replay.Cursor{last, pending, journal, bytes}:
      Laws.replay_gap_stops_the_batch(items, Replay.Cursor{last, True{}, journal, bytes}, Replay.replay_request(last, pending), invalid, accepted)
