import Base
import ./core.bend as Core
import ./tree_sort.bend as Sort

type PaneRecord is Data:
  PaneRecord{id: Core.StableId, title: U32, state: U32, presentation: U32, status: U32, summary: U32}

type WindowRecord is Data:
  WindowRecord{name: U32, active_pane: Core.StableId, layout: U32, flags: U32, presentation: U32, panes: List<&2, Sort.Tagged<PaneRecord>>}

type SessionRecord is Data:
  SessionRecord{name: U32, active_window: Core.StableId, viewers: U32, windows: List<&2, Sort.Tagged<WindowRecord>>}

type TreeSnapshot is Data:
  TreeSnapshot{generation: Core.StableId, focused: Maybe<&2, Core.StableId>, sessions: List<&2, Sort.Tagged<SessionRecord>>}

type Op is Data:
  UpsertSession{session: Sort.Tagged<SessionRecord>}
  RemoveSession{id: Core.StableId}
  SessionName{session: Core.StableId, name: U32}
  SessionActiveWindow{session: Core.StableId, window: Core.StableId}
  SessionViewers{session: Core.StableId, viewers: U32}
  UpsertWindow{session: Core.StableId, window: Sort.Tagged<WindowRecord>}
  RemoveWindow{session: Core.StableId, window: Core.StableId}
  WindowName{session: Core.StableId, window: Core.StableId, name: U32}
  WindowActivePane{session: Core.StableId, window: Core.StableId, pane: Core.StableId}
  WindowLayout{session: Core.StableId, window: Core.StableId, layout: U32}
  WindowFlags{session: Core.StableId, window: Core.StableId, index: U32, flags: U32}
  WindowPresentation{session: Core.StableId, window: Core.StableId, presentation: U32}
  UpsertPane{session: Core.StableId, window: Core.StableId, pane: PaneRecord}
  RemovePane{session: Core.StableId, window: Core.StableId, pane: Core.StableId}
  PaneTitle{session: Core.StableId, window: Core.StableId, pane: Core.StableId, title: U32}
  PaneState{session: Core.StableId, window: Core.StableId, pane: Core.StableId, state: U32}
  PanePresentation{session: Core.StableId, window: Core.StableId, pane: Core.StableId, presentation: U32}
  PaneStatus{session: Core.StableId, window: Core.StableId, pane: Core.StableId, status: U32}
  PaneSummary{session: Core.StableId, window: Core.StableId, pane: Core.StableId, summary: U32}

type TreeError is Data:
  BaseMismatch{expected: Core.StableId, received: Core.StableId}
  MissingSession{id: Core.StableId}
  MissingWindow{id: Core.StableId}
  MissingPane{id: Core.StableId}

type Outcome is Data:
  Outcome{error: Maybe<&2, TreeError>, snapshot: TreeSnapshot}

type Delta is Data:
  Delta{base: Core.StableId, version: Core.StableId, ops: List<&2, Op>}

type ClientView is Data:
  ClientView{focused: Maybe<&2, Core.StableId>, ops: List<&2, Op>}
