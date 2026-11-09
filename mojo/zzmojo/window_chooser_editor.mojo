from mjui.widgets.textedit import TextEdit, Edit


struct WindowChooserEditorCheckpoint(Movable):
    var bytes: List[UInt8]
    var caret: Int
    var anchor: Int
    var upstream: Bool
    var pref_x: Float32
    var undo: List[Edit]
    var undo_top: Int
    var coalesce: UInt8
    var marked: List[UInt8]
    var marked_a: Int
    var marked_b: Int

    def __init__(out self, field: TextEdit):
        self.bytes = field.buf.copy()
        self.caret = field.caret
        self.anchor = field.anchor
        self.upstream = field.upstream
        self.pref_x = field.pref_x
        self.undo = field.undo.copy()
        self.undo_top = field.undo_top
        self.coalesce = field.coalesce
        self.marked = field.marked.copy()
        self.marked_a = field.marked_a
        self.marked_b = field.marked_b

    def restore(var self, mut field: TextEdit):
        field.buf = self.bytes^
        field.caret = self.caret
        field.anchor = self.anchor
        field.upstream = self.upstream
        field.pref_x = self.pref_x
        field.undo = self.undo^
        field.undo_top = self.undo_top
        field.coalesce = self.coalesce
        field.marked = self.marked^
        field.marked_a = self.marked_a
        field.marked_b = self.marked_b
        field.disp.clear()
        field.follow = True
        field.dirty = True
        field.changed = False
