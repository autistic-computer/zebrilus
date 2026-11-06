import Base
import ./tree_data.bend as Tree
import ./tree_edit.bend as Edit
import ./tree_fields.bend as Fields
import ./tree_sort.bend as Sort

law pane_edits_preserve_every_other_field:
  for +pane: Tree.PaneRecord
  for +edit: Edit.PaneEdit
  Fields.PaneFrame(pane, Edit.pane_edit(pane, edit), edit)

law pane_edits_write_the_exact_requested_field:
  for +pane: Tree.PaneRecord
  for +edit: Edit.PaneEdit
  {Fields.pane_field(Edit.pane_edit(pane, edit), edit) == Fields.desired_pane_field(edit) : U32}

law window_edits_preserve_every_other_field:
  for +window: Sort.Tagged<Tree.WindowRecord>
  for +edit: Edit.WindowEdit
  Fields.WindowResultFrame(window, edit, Edit.window_edit(window, edit))

law session_edits_preserve_every_other_field:
  for +session: Sort.Tagged<Tree.SessionRecord>
  for +edit: Edit.SessionEdit
  Fields.SessionResultFrame(session, edit, Edit.session_edit(session, edit))
