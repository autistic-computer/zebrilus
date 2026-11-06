import Base
import ./core.bend as Core
import ./tree_data.bend as Tree
import ./tree_edit.bend as Edit
import ./tree_sort.bend as Sort

def PaneFrame(before: Tree.PaneRecord, after: Tree.PaneRecord, edit: Edit.PaneEdit) -> Type:
  match before after edit:
    case Tree.PaneRecord{a, _, s, p, t, m} Tree.PaneRecord{b, _, ns, np, nt, nm} Edit.SetPaneTitle{_}:
      {a == b : Core.StableId} & ({s == ns : U32} & ({p == np : U32} & ({t == nt : U32} & {m == nm : U32})))
    case Tree.PaneRecord{a, x, _, p, t, m} Tree.PaneRecord{b, nx, _, np, nt, nm} Edit.SetPaneState{_}:
      {a == b : Core.StableId} & ({x == nx : U32} & ({p == np : U32} & ({t == nt : U32} & {m == nm : U32})))
    case Tree.PaneRecord{a, x, s, _, t, m} Tree.PaneRecord{b, nx, ns, _, nt, nm} Edit.SetPanePresentation{_}:
      {a == b : Core.StableId} & ({x == nx : U32} & ({s == ns : U32} & ({t == nt : U32} & {m == nm : U32})))
    case Tree.PaneRecord{a, x, s, p, _, m} Tree.PaneRecord{b, nx, ns, np, _, nm} Edit.SetPaneStatus{_}:
      {a == b : Core.StableId} & ({x == nx : U32} & ({s == ns : U32} & ({p == np : U32} & {m == nm : U32})))
    case Tree.PaneRecord{a, x, s, p, t, _} Tree.PaneRecord{b, nx, ns, np, nt, _} Edit.SetPaneSummary{_}:
      {a == b : Core.StableId} & ({x == nx : U32} & ({s == ns : U32} & ({p == np : U32} & {t == nt : U32})))

def pane_field(pane: Tree.PaneRecord, edit: Edit.PaneEdit) -> U32:
  match pane edit:
    case Tree.PaneRecord{_, title, _, _, _, _} Edit.SetPaneTitle{_}:
      title
    case Tree.PaneRecord{_, _, state, _, _, _} Edit.SetPaneState{_}:
      state
    case Tree.PaneRecord{_, _, _, presentation, _, _} Edit.SetPanePresentation{_}:
      presentation
    case Tree.PaneRecord{_, _, _, _, status, _} Edit.SetPaneStatus{_}:
      status
    case Tree.PaneRecord{_, _, _, _, _, summary} Edit.SetPaneSummary{_}:
      summary

def desired_pane_field(edit: Edit.PaneEdit) -> U32:
  match edit:
    case Edit.SetPaneTitle{value}:
      value
    case Edit.SetPaneState{value}:
      value
    case Edit.SetPanePresentation{value}:
      value
    case Edit.SetPaneStatus{value}:
      value
    case Edit.SetPaneSummary{value}:
      value

def WindowKeyFrame(before: Sort.SortKey, after: Sort.SortKey, edit: Edit.WindowEdit) -> Type:
  match before after edit:
    case Sort.SortKey{_, h, l} Sort.SortKey{_, nh, nl} Edit.SetWindowFlags{_, _}:
      {h == nh : U32} & {l == nl : U32}
    case first second _:
      {first == second : Sort.SortKey}

def WindowBodyFrame(before: Tree.WindowRecord, after: Tree.WindowRecord, edit: Edit.WindowEdit) -> Type:
  match before after edit:
    case Tree.WindowRecord{_, a, l, f, p, ps} Tree.WindowRecord{_, na, nl, nf, np, nps} Edit.SetWindowName{_}:
      {a == na : Core.StableId} & ({l == nl : U32} & ({f == nf : U32} & ({p == np : U32} & {ps == nps : List<&2, Sort.Tagged<Tree.PaneRecord>>})))
    case Tree.WindowRecord{n, _, l, f, p, ps} Tree.WindowRecord{nn, _, nl, nf, np, nps} Edit.SetWindowActivePane{_}:
      {n == nn : U32} & ({l == nl : U32} & ({f == nf : U32} & ({p == np : U32} & {ps == nps : List<&2, Sort.Tagged<Tree.PaneRecord>>})))
    case Tree.WindowRecord{n, a, _, f, p, ps} Tree.WindowRecord{nn, na, _, nf, np, nps} Edit.SetWindowLayout{_}:
      {n == nn : U32} & ({a == na : Core.StableId} & ({f == nf : U32} & ({p == np : U32} & {ps == nps : List<&2, Sort.Tagged<Tree.PaneRecord>>})))
    case Tree.WindowRecord{n, a, l, _, p, ps} Tree.WindowRecord{nn, na, nl, _, np, nps} Edit.SetWindowFlags{_, _}:
      {n == nn : U32} & ({a == na : Core.StableId} & ({l == nl : U32} & ({p == np : U32} & {ps == nps : List<&2, Sort.Tagged<Tree.PaneRecord>>})))
    case Tree.WindowRecord{n, a, l, f, _, ps} Tree.WindowRecord{nn, na, nl, nf, _, nps} Edit.SetWindowPresentation{_}:
      {n == nn : U32} & ({a == na : Core.StableId} & ({l == nl : U32} & ({f == nf : U32} & {ps == nps : List<&2, Sort.Tagged<Tree.PaneRecord>>})))
    case Tree.WindowRecord{n, a, l, f, p, _} Tree.WindowRecord{nn, na, nl, nf, np, _} _:
      {n == nn : U32} & ({a == na : Core.StableId} & ({l == nl : U32} & ({f == nf : U32} & {p == np : U32})))

def WindowFrame(+before: Sort.Tagged<Tree.WindowRecord>, +after: Sort.Tagged<Tree.WindowRecord>, +edit: Edit.WindowEdit) -> Type:
  WindowKeyFrame(Sort.item_key(Tree.WindowRecord, before), Sort.item_key(Tree.WindowRecord, after), edit) & WindowBodyFrame(Sort.value(Tree.WindowRecord, before), Sort.value(Tree.WindowRecord, after), edit)

def WindowResultFrame(before: Sort.Tagged<Tree.WindowRecord>, edit: Edit.WindowEdit, result: Result<&2, &2, Tree.TreeError, Sort.Tagged<Tree.WindowRecord>>) -> Type:
  match result:
    case Fail{_}:
      Unit
    case Done{after}:
      WindowFrame(before, after, edit)

def SessionBodyFrame(before: Tree.SessionRecord, after: Tree.SessionRecord, edit: Edit.SessionEdit) -> Type:
  match before after edit:
    case Tree.SessionRecord{_, a, v, w} Tree.SessionRecord{_, na, nv, nw} Edit.SetSessionName{_}:
      {a == na : Core.StableId} & ({v == nv : U32} & {w == nw : List<&2, Sort.Tagged<Tree.WindowRecord>>})
    case Tree.SessionRecord{n, _, v, w} Tree.SessionRecord{nn, _, nv, nw} Edit.SetSessionActiveWindow{_}:
      {n == nn : U32} & ({v == nv : U32} & {w == nw : List<&2, Sort.Tagged<Tree.WindowRecord>>})
    case Tree.SessionRecord{n, a, _, w} Tree.SessionRecord{nn, na, _, nw} Edit.SetSessionViewers{_}:
      {n == nn : U32} & ({a == na : Core.StableId} & {w == nw : List<&2, Sort.Tagged<Tree.WindowRecord>>})
    case Tree.SessionRecord{n, a, v, _} Tree.SessionRecord{nn, na, nv, _} _:
      {n == nn : U32} & ({a == na : Core.StableId} & {v == nv : U32})

def SessionFrame(+before: Sort.Tagged<Tree.SessionRecord>, +after: Sort.Tagged<Tree.SessionRecord>, edit: Edit.SessionEdit) -> Type:
  {Sort.item_key(Tree.SessionRecord, before) == Sort.item_key(Tree.SessionRecord, after) : Sort.SortKey} & SessionBodyFrame(Sort.value(Tree.SessionRecord, before), Sort.value(Tree.SessionRecord, after), edit)

def SessionResultFrame(before: Sort.Tagged<Tree.SessionRecord>, edit: Edit.SessionEdit, result: Result<&2, &2, Tree.TreeError, Sort.Tagged<Tree.SessionRecord>>) -> Type:
  match result:
    case Fail{_}:
      Unit
    case Done{after}:
      SessionFrame(before, after, edit)
