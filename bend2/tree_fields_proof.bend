import Base
import ./core.bend as Core
import ./tree_data.bend as Tree
import ./tree_edit.bend as Edit
import ./tree_fields.bend as Fields
import ./tree_fields_laws.bend as Laws
import ./tree_lookup.bend as Lookup
import ./tree_sort.bend as Sort

def Laws.pane_edits_preserve_every_other_field(pane, edit):
  match pane edit:
    case Tree.PaneRecord{_, _, _, _, _, _} Edit.SetPaneTitle{_}:
      ({==}, ({==}, ({==}, ({==}, {==}))))
    case Tree.PaneRecord{_, _, _, _, _, _} Edit.SetPaneState{_}:
      ({==}, ({==}, ({==}, ({==}, {==}))))
    case Tree.PaneRecord{_, _, _, _, _, _} Edit.SetPanePresentation{_}:
      ({==}, ({==}, ({==}, ({==}, {==}))))
    case Tree.PaneRecord{_, _, _, _, _, _} Edit.SetPaneStatus{_}:
      ({==}, ({==}, ({==}, ({==}, {==}))))
    case Tree.PaneRecord{_, _, _, _, _, _} Edit.SetPaneSummary{_}:
      ({==}, ({==}, ({==}, ({==}, {==}))))

def Laws.pane_edits_write_the_exact_requested_field(pane, edit):
  match pane edit:
    case Tree.PaneRecord{_, _, _, _, _, _} Edit.SetPaneTitle{_}:
      {==}
    case Tree.PaneRecord{_, _, _, _, _, _} Edit.SetPaneState{_}:
      {==}
    case Tree.PaneRecord{_, _, _, _, _, _} Edit.SetPanePresentation{_}:
      {==}
    case Tree.PaneRecord{_, _, _, _, _, _} Edit.SetPaneStatus{_}:
      {==}
    case Tree.PaneRecord{_, _, _, _, _, _} Edit.SetPaneSummary{_}:
      {==}

law direct_window_frame:
  for +key: Sort.SortKey
  for +body: Tree.WindowRecord
  for +edit: Edit.WindowEdit
  Fields.WindowFrame(Sort.Tagged{key, body}, Sort.Tagged{Edit.window_key(key, edit), Edit.window_direct(body, edit)}, edit)

def direct_window_frame(key, body, edit):
  match key body edit:
    case Sort.SortKey{_, _, _} Tree.WindowRecord{_, _, _, _, _, _} Edit.SetWindowName{_}:
      ({==}, ({==}, ({==}, ({==}, ({==}, {==})))))
    case Sort.SortKey{_, _, _} Tree.WindowRecord{_, _, _, _, _, _} Edit.SetWindowActivePane{_}:
      ({==}, ({==}, ({==}, ({==}, ({==}, {==})))))
    case Sort.SortKey{_, _, _} Tree.WindowRecord{_, _, _, _, _, _} Edit.SetWindowLayout{_}:
      ({==}, ({==}, ({==}, ({==}, ({==}, {==})))))
    case Sort.SortKey{_, _, _} Tree.WindowRecord{_, _, _, _, _, _} Edit.SetWindowFlags{_, _}:
      (({==}, {==}), ({==}, ({==}, ({==}, ({==}, {==})))))
    case Sort.SortKey{_, _, _} Tree.WindowRecord{_, _, _, _, _, _} Edit.SetWindowPresentation{_}:
      ({==}, ({==}, ({==}, ({==}, ({==}, {==})))))
    case Sort.SortKey{_, _, _} Tree.WindowRecord{_, _, _, _, _, _} Edit.InsertPane{_}:
      ({==}, ({==}, ({==}, ({==}, ({==}, {==})))))
    case Sort.SortKey{_, _, _} Tree.WindowRecord{_, _, _, _, _, _} Edit.DeletePane{_}:
      ({==}, ({==}, ({==}, ({==}, ({==}, {==})))))
    case Sort.SortKey{_, _, _} Tree.WindowRecord{_, _, _, _, _, _} Edit.ChangePane{_, _}:
      ({==}, ({==}, ({==}, ({==}, ({==}, {==})))))

law changed_pane_frame:
  for +window: Sort.Tagged<Tree.WindowRecord>
  for +target: Core.StableId
  for +edit: Edit.PaneEdit
  for +found: Maybe<&2, Lookup.Zipper<Tree.PaneRecord>>
  Fields.WindowResultFrame(window, Edit.ChangePane{target, edit}, Edit.changed_pane(window, target, edit, found))

def changed_pane_frame(window, target, edit, found):
  match window found:
    case _ None{}:
      Unit{}
    case Sort.Tagged{Sort.SortKey{_, _, _}, Tree.WindowRecord{_, _, _, _, _, _}} Some{_}:
      ({==}, ({==}, ({==}, ({==}, ({==}, {==})))))

def Laws.window_edits_preserve_every_other_field(window, edit):
  match window edit:
    case Sort.Tagged{key, body} Edit.SetWindowName{name}:
      direct_window_frame(key, body, Edit.SetWindowName{name})
    case Sort.Tagged{key, body} Edit.SetWindowActivePane{pane}:
      direct_window_frame(key, body, Edit.SetWindowActivePane{pane})
    case Sort.Tagged{key, body} Edit.SetWindowLayout{layout}:
      direct_window_frame(key, body, Edit.SetWindowLayout{layout})
    case Sort.Tagged{key, body} Edit.SetWindowFlags{index, flags}:
      direct_window_frame(key, body, Edit.SetWindowFlags{index, flags})
    case Sort.Tagged{key, body} Edit.SetWindowPresentation{presentation}:
      direct_window_frame(key, body, Edit.SetWindowPresentation{presentation})
    case Sort.Tagged{key, body} Edit.InsertPane{pane}:
      direct_window_frame(key, body, Edit.InsertPane{pane})
    case Sort.Tagged{key, body} Edit.DeletePane{pane}:
      direct_window_frame(key, body, Edit.DeletePane{pane})
    case entry Edit.ChangePane{target, pane_edit}:
      changed_pane_frame(entry, target, pane_edit, Lookup.locate(Tree.PaneRecord, Edit.window_panes(entry), target))

law direct_session_frame:
  for +key: Sort.SortKey
  for +body: Tree.SessionRecord
  for +edit: Edit.SessionEdit
  Fields.SessionFrame(Sort.Tagged{key, body}, Sort.Tagged{key, Edit.session_direct(body, edit)}, edit)

def direct_session_frame(key, body, edit):
  match body edit:
    case Tree.SessionRecord{_, _, _, _} Edit.SetSessionName{_}:
      ({==}, ({==}, ({==}, {==})))
    case Tree.SessionRecord{_, _, _, _} Edit.SetSessionActiveWindow{_}:
      ({==}, ({==}, ({==}, {==})))
    case Tree.SessionRecord{_, _, _, _} Edit.SetSessionViewers{_}:
      ({==}, ({==}, ({==}, {==})))
    case Tree.SessionRecord{_, _, _, _} Edit.InsertWindow{_}:
      ({==}, ({==}, ({==}, {==})))
    case Tree.SessionRecord{_, _, _, _} Edit.DeleteWindow{_}:
      ({==}, ({==}, ({==}, {==})))
    case Tree.SessionRecord{_, _, _, _} Edit.ChangeWindow{_, _}:
      ({==}, ({==}, ({==}, {==})))

law changed_window_result_frame:
  for +session: Sort.Tagged<Tree.SessionRecord>
  for +target: Core.StableId
  for +edit: Edit.WindowEdit
  for +zipper: Lookup.Zipper<Tree.WindowRecord>
  for +result: Result<&2, &2, Tree.TreeError, Sort.Tagged<Tree.WindowRecord>>
  Fields.SessionResultFrame(session, Edit.ChangeWindow{target, edit}, Edit.changed_window_result(session, zipper, result))

def changed_window_result_frame(session, target, edit, zipper, result):
  match session result:
    case _ Fail{_}:
      Unit{}
    case Sort.Tagged{_, Tree.SessionRecord{_, _, _, _}} Done{_}:
      ({==}, ({==}, ({==}, {==})))

law changed_window_frame:
  for +session: Sort.Tagged<Tree.SessionRecord>
  for +target: Core.StableId
  for +edit: Edit.WindowEdit
  for +found: Maybe<&2, Lookup.Zipper<Tree.WindowRecord>>
  Fields.SessionResultFrame(session, Edit.ChangeWindow{target, edit}, Edit.changed_window(session, target, edit, found))

def changed_window_frame(session, target, edit, found):
  match found:
    case None{}:
      Unit{}
    case Some{zipper}:
      changed_window_result_frame(session, target, edit, zipper, Edit.window_edit(Lookup.zipper_entry(Tree.WindowRecord, zipper), edit))

def Laws.session_edits_preserve_every_other_field(session, edit):
  match session edit:
    case Sort.Tagged{key, body} Edit.SetSessionName{name}:
      direct_session_frame(key, body, Edit.SetSessionName{name})
    case Sort.Tagged{key, body} Edit.SetSessionActiveWindow{window}:
      direct_session_frame(key, body, Edit.SetSessionActiveWindow{window})
    case Sort.Tagged{key, body} Edit.SetSessionViewers{viewers}:
      direct_session_frame(key, body, Edit.SetSessionViewers{viewers})
    case Sort.Tagged{key, body} Edit.InsertWindow{window}:
      direct_session_frame(key, body, Edit.InsertWindow{window})
    case Sort.Tagged{key, body} Edit.DeleteWindow{window}:
      direct_session_frame(key, body, Edit.DeleteWindow{window})
    case entry Edit.ChangeWindow{target, edit}:
      changed_window_frame(entry, target, edit, Lookup.locate(Tree.WindowRecord, Edit.session_windows(entry), target))
