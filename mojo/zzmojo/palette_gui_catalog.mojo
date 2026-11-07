from .palette_verbs import PaletteGuiVerb

comptime PALETTE_GUI_CATALOG = """titlebar-mode	Titlebar mode	titlebar mode	toggle sidebar hide sidebar show sidebar
pane-gaps	Pane gaps	pane gaps	gaps frames
window-background-blur	Window blur	window blur	blur translucent
chrome-accent	Accent	select accent	accent color color
flat-sidebar	Sidebar layout	sidebar layout	list tree flat"""


def gui_verb(id: String) -> Optional[PaletteGuiVerb]:
    for line in String(PALETTE_GUI_CATALOG).split("\n"):
        var fields = line.split("\t")
        if fields[0] == id:
            return PaletteGuiVerb(String(fields[0]), String(fields[1]), String(fields[2]), String(fields[3]))
    return None
