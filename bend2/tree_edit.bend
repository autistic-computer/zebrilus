import Base
import ./core.bend as Core
import ./tree_data.bend as Tree
import ./tree_list.bend as Lists
import ./tree_lookup.bend as Lookup
import ./tree_sort.bend as Sort

type PaneEdit is Data:
  SetPaneTitle{title: U32}
  SetPaneState{state: U32}
  SetPanePresentation{presentation: U32}
  SetPaneStatus{status: U32}
  SetPaneSummary{summary: U32}

type WindowEdit is Data:
  SetWindowName{name: U32}
  SetWindowActivePane{pane: Core.StableId}
  SetWindowLayout{layout: U32}
  SetWindowFlags{index: U32, flags: U32}
  SetWindowPresentation{presentation: U32}
  InsertPane{pane: Tree.PaneRecord}
  DeletePane{pane: Core.StableId}
  ChangePane{pane: Core.StableId, edit: PaneEdit}

type SessionEdit is Data:
  SetSessionName{name: U32}
  SetSessionActiveWindow{window: Core.StableId}
  SetSessionViewers{viewers: U32}
  InsertWindow{window: Sort.Tagged<Tree.WindowRecord>}
  DeleteWindow{window: Core.StableId}
  ChangeWindow{window: Core.StableId, edit: WindowEdit}

type RootEdit is Data:
  InsertSession{session: Sort.Tagged<Tree.SessionRecord>}
  DeleteSession{session: Core.StableId}
  ChangeSession{session: Core.StableId, edit: SessionEdit}

def lower(op: Tree.Op) -> RootEdit:
  match op:
    case Tree.UpsertSession{session}:
      InsertSession{session}
    case Tree.RemoveSession{id}:
      DeleteSession{id}
    case Tree.SessionName{session, name}:
      ChangeSession{session, SetSessionName{name}}
    case Tree.SessionActiveWindow{session, window}:
      ChangeSession{session, SetSessionActiveWindow{window}}
    case Tree.SessionViewers{session, viewers}:
      ChangeSession{session, SetSessionViewers{viewers}}
    case Tree.UpsertWindow{session, window}:
      ChangeSession{session, InsertWindow{window}}
    case Tree.RemoveWindow{session, window}:
      ChangeSession{session, DeleteWindow{window}}
    case Tree.WindowName{session, window, name}:
      ChangeSession{session, ChangeWindow{window, SetWindowName{name}}}
    case Tree.WindowActivePane{session, window, pane}:
      ChangeSession{session, ChangeWindow{window, SetWindowActivePane{pane}}}
    case Tree.WindowLayout{session, window, layout}:
      ChangeSession{session, ChangeWindow{window, SetWindowLayout{layout}}}
    case Tree.WindowFlags{session, window, index, flags}:
      ChangeSession{session, ChangeWindow{window, SetWindowFlags{index, flags}}}
    case Tree.WindowPresentation{session, window, presentation}:
      ChangeSession{session, ChangeWindow{window, SetWindowPresentation{presentation}}}
    case Tree.UpsertPane{session, window, pane}:
      ChangeSession{session, ChangeWindow{window, InsertPane{pane}}}
    case Tree.RemovePane{session, window, pane}:
      ChangeSession{session, ChangeWindow{window, DeletePane{pane}}}
    case Tree.PaneTitle{session, window, pane, title}:
      ChangeSession{session, ChangeWindow{window, ChangePane{pane, SetPaneTitle{title}}}}
    case Tree.PaneState{session, window, pane, state}:
      ChangeSession{session, ChangeWindow{window, ChangePane{pane, SetPaneState{state}}}}
    case Tree.PanePresentation{session, window, pane, presentation}:
      ChangeSession{session, ChangeWindow{window, ChangePane{pane, SetPanePresentation{presentation}}}}
    case Tree.PaneStatus{session, window, pane, status}:
      ChangeSession{session, ChangeWindow{window, ChangePane{pane, SetPaneStatus{status}}}}
    case Tree.PaneSummary{session, window, pane, summary}:
      ChangeSession{session, ChangeWindow{window, ChangePane{pane, SetPaneSummary{summary}}}}

def pane_id(pane: Tree.PaneRecord) -> Core.StableId:
  match pane:
    case Tree.PaneRecord{id, _, _, _, _, _}:
      id

def pane_entry(+pane: Tree.PaneRecord) -> Sort.Tagged<Tree.PaneRecord>:
  match pane:
    case Tree.PaneRecord{Core.Id{high, low}, title, state, presentation, status, summary}:
      Sort.Tagged{Sort.SortKey{0, high, low}, Tree.PaneRecord{Core.Id{high, low}, title, state, presentation, status, summary}}

def pane_edit(pane: Tree.PaneRecord, edit: PaneEdit) -> Tree.PaneRecord:
  match pane edit:
    case Tree.PaneRecord{id, _, state, presentation, status, summary} SetPaneTitle{title}:
      Tree.PaneRecord{id, title, state, presentation, status, summary}
    case Tree.PaneRecord{id, title, _, presentation, status, summary} SetPaneState{state}:
      Tree.PaneRecord{id, title, state, presentation, status, summary}
    case Tree.PaneRecord{id, title, state, _, status, summary} SetPanePresentation{presentation}:
      Tree.PaneRecord{id, title, state, presentation, status, summary}
    case Tree.PaneRecord{id, title, state, presentation, _, summary} SetPaneStatus{status}:
      Tree.PaneRecord{id, title, state, presentation, status, summary}
    case Tree.PaneRecord{id, title, state, presentation, status, _} SetPaneSummary{summary}:
      Tree.PaneRecord{id, title, state, presentation, status, summary}

def window_with_panes(window: Sort.Tagged<Tree.WindowRecord>, panes: List<&2, Sort.Tagged<Tree.PaneRecord>>) -> Sort.Tagged<Tree.WindowRecord>:
  match window:
    case Sort.Tagged{key, Tree.WindowRecord{name, active, layout, flags, presentation, _}}:
      Sort.Tagged{key, Tree.WindowRecord{name, active, layout, flags, presentation, panes}}

def window_panes(window: Sort.Tagged<Tree.WindowRecord>) -> List<&2, Sort.Tagged<Tree.PaneRecord>>:
  match window:
    case Sort.Tagged{_, Tree.WindowRecord{_, _, _, _, _, panes}}:
      panes

def window_key(key: Sort.SortKey, edit: WindowEdit) -> Sort.SortKey:
  match key edit:
    case Sort.SortKey{_, high, low} SetWindowFlags{index, _}:
      Sort.SortKey{index, high, low}
    case rest _:
      rest

def window_direct(window: Tree.WindowRecord, edit: WindowEdit) -> Tree.WindowRecord:
  match window edit:
    case Tree.WindowRecord{_, active, layout, flags, presentation, panes} SetWindowName{name}:
      Tree.WindowRecord{name, active, layout, flags, presentation, panes}
    case Tree.WindowRecord{name, _, layout, flags, presentation, panes} SetWindowActivePane{active}:
      Tree.WindowRecord{name, active, layout, flags, presentation, panes}
    case Tree.WindowRecord{name, active, _, flags, presentation, panes} SetWindowLayout{layout}:
      Tree.WindowRecord{name, active, layout, flags, presentation, panes}
    case Tree.WindowRecord{name, active, layout, _, presentation, panes} SetWindowFlags{_, flags}:
      Tree.WindowRecord{name, active, layout, flags, presentation, panes}
    case Tree.WindowRecord{name, active, layout, flags, _, panes} SetWindowPresentation{presentation}:
      Tree.WindowRecord{name, active, layout, flags, presentation, panes}
    case Tree.WindowRecord{name, active, layout, flags, presentation, panes} InsertPane{pane}:
      Tree.WindowRecord{name, active, layout, flags, presentation, Sort.radix(Tree.PaneRecord, 64n, 63, Lookup.upsert(Tree.PaneRecord, panes, pane_entry(pane)))}
    case Tree.WindowRecord{name, active, layout, flags, presentation, panes} DeletePane{pane}:
      Tree.WindowRecord{name, active, layout, flags, presentation, Lookup.remove(Tree.PaneRecord, panes, pane)}
    case rest ChangePane{_, _}:
      rest

def changed_pane(window: Sort.Tagged<Tree.WindowRecord>, target: Core.StableId, edit: PaneEdit, found: Maybe<&2, Lookup.Zipper<Tree.PaneRecord>>) -> Result<&2, &2, Tree.TreeError, Sort.Tagged<Tree.WindowRecord>>:
  match found:
    case None{}:
      Fail{Tree.MissingPane{target}}
    case Some{+zipper}:
      Done{window_with_panes(window, Lookup.replace_value(Tree.PaneRecord, zipper, pane_edit(Lookup.zipper_value(Tree.PaneRecord, zipper), edit)))}

def window_edit(+window: Sort.Tagged<Tree.WindowRecord>, +edit: WindowEdit) -> Result<&2, &2, Tree.TreeError, Sort.Tagged<Tree.WindowRecord>>:
  match window edit:
    case entry ChangePane{+target, pane_edit}:
      changed_pane(entry, target, pane_edit, Lookup.locate(Tree.PaneRecord, window_panes(entry), target))
    case Sort.Tagged{key, body} other:
      Done{Sort.Tagged{window_key(key, other), window_direct(body, other)}}

def session_with_windows(session: Sort.Tagged<Tree.SessionRecord>, windows: List<&2, Sort.Tagged<Tree.WindowRecord>>) -> Sort.Tagged<Tree.SessionRecord>:
  match session:
    case Sort.Tagged{key, Tree.SessionRecord{name, active, viewers, _}}:
      Sort.Tagged{key, Tree.SessionRecord{name, active, viewers, windows}}

def session_windows(session: Sort.Tagged<Tree.SessionRecord>) -> List<&2, Sort.Tagged<Tree.WindowRecord>>:
  match session:
    case Sort.Tagged{_, Tree.SessionRecord{_, _, _, windows}}:
      windows

def session_direct(session: Tree.SessionRecord, edit: SessionEdit) -> Tree.SessionRecord:
  match session edit:
    case Tree.SessionRecord{_, active, viewers, windows} SetSessionName{name}:
      Tree.SessionRecord{name, active, viewers, windows}
    case Tree.SessionRecord{name, _, viewers, windows} SetSessionActiveWindow{active}:
      Tree.SessionRecord{name, active, viewers, windows}
    case Tree.SessionRecord{name, active, _, windows} SetSessionViewers{viewers}:
      Tree.SessionRecord{name, active, viewers, windows}
    case Tree.SessionRecord{name, active, viewers, windows} InsertWindow{window}:
      Tree.SessionRecord{name, active, viewers, Lookup.upsert(Tree.WindowRecord, windows, window)}
    case Tree.SessionRecord{name, active, viewers, windows} DeleteWindow{window}:
      Tree.SessionRecord{name, active, viewers, Lookup.remove(Tree.WindowRecord, windows, window)}
    case rest ChangeWindow{_, _}:
      rest

def changed_window_result(session: Sort.Tagged<Tree.SessionRecord>, zipper: Lookup.Zipper<Tree.WindowRecord>, result: Result<&2, &2, Tree.TreeError, Sort.Tagged<Tree.WindowRecord>>) -> Result<&2, &2, Tree.TreeError, Sort.Tagged<Tree.SessionRecord>>:
  match result:
    case Fail{error}:
      Fail{error}
    case Done{window}:
      Done{session_with_windows(session, Lookup.replace(Tree.WindowRecord, zipper, window))}

def changed_window(session: Sort.Tagged<Tree.SessionRecord>, target: Core.StableId, edit: WindowEdit, found: Maybe<&2, Lookup.Zipper<Tree.WindowRecord>>) -> Result<&2, &2, Tree.TreeError, Sort.Tagged<Tree.SessionRecord>>:
  match found:
    case None{}:
      Fail{Tree.MissingWindow{target}}
    case Some{+zipper}:
      changed_window_result(session, zipper, window_edit(Lookup.zipper_entry(Tree.WindowRecord, zipper), edit))

def session_edit(+session: Sort.Tagged<Tree.SessionRecord>, edit: SessionEdit) -> Result<&2, &2, Tree.TreeError, Sort.Tagged<Tree.SessionRecord>>:
  match session edit:
    case entry ChangeWindow{+target, edit}:
      changed_window(entry, target, edit, Lookup.locate(Tree.WindowRecord, session_windows(entry), target))
    case Sort.Tagged{key, body} other:
      Done{Sort.Tagged{key, session_direct(body, other)}}

def snapshot_with_sessions(snapshot: Tree.TreeSnapshot, sessions: List<&2, Sort.Tagged<Tree.SessionRecord>>) -> Tree.TreeSnapshot:
  match snapshot:
    case Tree.TreeSnapshot{generation, focused, _}:
      Tree.TreeSnapshot{generation, focused, sessions}

def changed_session_result(snapshot: Tree.TreeSnapshot, zipper: Lookup.Zipper<Tree.SessionRecord>, result: Result<&2, &2, Tree.TreeError, Sort.Tagged<Tree.SessionRecord>>) -> Result<&2, &2, Tree.TreeError, Tree.TreeSnapshot>:
  match result:
    case Fail{error}:
      Fail{error}
    case Done{session}:
      Done{snapshot_with_sessions(snapshot, Lookup.replace(Tree.SessionRecord, zipper, session))}

def changed_session(snapshot: Tree.TreeSnapshot, target: Core.StableId, edit: SessionEdit, found: Maybe<&2, Lookup.Zipper<Tree.SessionRecord>>) -> Result<&2, &2, Tree.TreeError, Tree.TreeSnapshot>:
  match found:
    case None{}:
      Fail{Tree.MissingSession{target}}
    case Some{+zipper}:
      changed_session_result(snapshot, zipper, session_edit(Lookup.zipper_entry(Tree.SessionRecord, zipper), edit))

def root_edit(+snapshot: Tree.TreeSnapshot, edit: RootEdit) -> Result<&2, &2, Tree.TreeError, Tree.TreeSnapshot>:
  match snapshot edit:
    case Tree.TreeSnapshot{generation, focused, sessions} InsertSession{session}:
      Done{Tree.TreeSnapshot{generation, focused, Lookup.upsert(Tree.SessionRecord, sessions, session)}}
    case Tree.TreeSnapshot{generation, focused, sessions} DeleteSession{session}:
      Done{Tree.TreeSnapshot{generation, focused, Lookup.remove(Tree.SessionRecord, sessions, session)}}
    case Tree.TreeSnapshot{generation, focused, +sessions} ChangeSession{+target, edit}:
      changed_session(Tree.TreeSnapshot{generation, focused, sessions}, target, edit, Lookup.locate(Tree.SessionRecord, sessions, target))

def step(snapshot: Tree.TreeSnapshot, op: Tree.Op) -> Result<&2, &2, Tree.TreeError, Tree.TreeSnapshot>:
  root_edit(snapshot, lower(op))
