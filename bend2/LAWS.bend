import Base
import ./core.bend as Core
import ./replay.bend as Replay
import ./terminal_laws.bend as Terminal
import ./terminal_cells_laws.bend as Cells
import ./terminal_cells_origins_laws.bend as CellOrigins
import ./tree_laws.bend as Tree

law swap_preserves_count:
  for +node: Core.Layout
  for +source: Core.StableId
  for +target: Core.StableId
  {Core.pane_count(Core.swapped_layout(node, source, target)) == Core.pane_count(node) : Nat}

law swap_preserves_shape:
  for +node: Core.Layout
  for +source: Core.StableId
  for +target: Core.StableId
  {Core.shape(Core.swapped_layout(node, source, target)) == Core.shape(node) : Core.LayoutShape}

law frame_acceptance:
  for +short: Bool
  for +large: Bool
  for +length: Bool
  for +header: Bool
  for +lane: Bool
  for +version: Bool
  for +flags: Bool
  {Core.accepted(Core.frame_gate(short, large, length, header, lane, version, flags)) == Bool.not(short || large || length || header || lane || version || flags) : Bool}

law truncated_prefix_dominates:
  for large: Bool
  for length: Bool
  for header: Bool
  for lane: Bool
  for version: Bool
  for flags: Bool
  {Core.frame_gate(True{}, large, length, header, lane, version, flags) == Core.Truncated{} : Core.FrameStatus}

law oversized_length_dominates:
  for length: Bool
  for header: Bool
  for lane: Bool
  for version: Bool
  for flags: Bool
  {Core.frame_gate(False{}, True{}, length, header, lane, version, flags) == Core.FrameTooLarge{} : Core.FrameStatus}

law version_precedes_flags:
  for flags: Bool
  {Core.frame_gate(False{}, False{}, False{}, False{}, False{}, True{}, flags) == Core.VersionMismatch{} : Core.FrameStatus}

law invalid_lane_precedes_version:
  for version: Bool
  for flags: Bool
  {Core.frame_gate(False{}, False{}, False{}, False{}, True{}, version, flags) == Core.UnsupportedLane{} : Core.FrameStatus}

law permission_demands_attention:
  for phase: Core.AgentPhase
  {Core.attention(phase, True{}) == Core.NeedsInput{} : Core.Attention}

law stable_attention_has_no_edge:
  for +state: Core.Attention
  {Core.attention_edge(state, state) == Core.NoEdge{} : Core.AttentionEdge}

law swap_is_involutive:
  for +node: Core.Layout
  for +source: Core.StableId
  for +target: Core.StableId
  {Core.swapped_layout(Core.swapped_layout(node, source, target), source, target) == node : Core.Layout}

law replay_step_never_rewinds:
  for +item: Replay.ReplayItem
  for +acc: Replay.ReplayAcc
  {Replay.sequence_ge(Replay.acc_last(Replay.replay_step(item, acc)), Replay.acc_last(acc)) == True{} : Bool}

law replay_gap_is_requested_once:
  for +last: Core.StableId
  {Replay.replay_request(last, True{}) == None{} : Maybe<&2, Core.StableId>}

law replay_gap_stops_the_batch:
  for items: List<&2, Replay.ReplayItem>
  for +cursor: Replay.ReplayCursor
  for +request: Maybe<&2, Core.StableId>
  for +invalid: Bool
  for +accepted: Bool
  {Replay.fold(items, Replay.Acc{cursor, request, invalid, accepted, True{}}) == Replay.Acc{cursor, request, invalid, accepted, True{}} : Replay.ReplayAcc}

law replay_duplicate_has_no_effect:
  for +last: Core.StableId
  for +pending: Bool
  for +journal: List<&2, Replay.JournalEntry>
  for +bytes: Core.StableId
  for reset: Bool
  for restoring: Bool
  for size: U32
  for +invalid: Bool
  for +accepted: Bool
  {Replay.replay_step(Replay.JournalUpdate{last, reset, restoring, size}, Replay.Acc{Replay.Cursor{last, pending, journal, bytes}, None{}, invalid, accepted, False{}}) == Replay.Acc{Replay.Cursor{last, pending, journal, bytes}, None{}, invalid, accepted, False{}} : Replay.ReplayAcc}

law replay_gap_applies_only_the_prefix:
  for items: List<&2, Replay.ReplayItem>
  for +cursor: Replay.ReplayCursor
  for +invalid: Bool
  for +accepted: Bool
  {Replay.fold(items, Replay.gap(cursor, invalid, accepted)) == Replay.gap(cursor, invalid, accepted) : Replay.ReplayAcc}
