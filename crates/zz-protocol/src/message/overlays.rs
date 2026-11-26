use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerminalUiCommand {
    BeginSearch { direction: SearchDirection },
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandPromptKind {
    Command,
    Value,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandPromptType {
    #[default]
    Command,
    Search,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandPromptMode {
    #[default]
    Text,
    Single,
    Numeric,
    Incremental,
    Key,
    BackspaceExit,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandPromptState {
    pub prompt: String,
    pub input: String,
    pub cursor: u32,
    pub kind: CommandPromptKind,
    pub history: Vec<String>,
    pub prompt_type: CommandPromptType,
    pub mode: CommandPromptMode,
    pub no_freeze: bool,
    pub pane: Option<PaneId>,
    pub command_mode: bool,
    pub prompt_cursor: PromptCursor,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptCursor {
    pub style: u8,
    pub colour: Option<crate::TmuxColour>,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChooseTreeKind {
    Windows,
    Panes,
    Clients,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChooseTreePaneKind {
    Terminal,
    Browser,
    Editor,
    Agent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ChooseTreeTarget {
    Session(SessionId),
    Window(WindowId),
    Pane(PaneId),
    Client(ClientId),
}

impl std::fmt::Display for ChooseTreeTarget {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Session(id) => id.fmt(formatter),
            Self::Window(id) => id.fmt(formatter),
            Self::Pane(id) => id.fmt(formatter),
            Self::Client(id) => id.fmt(formatter),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChooseTreeItem {
    pub label: String,
    pub detail: String,
    pub target: ChooseTreeTarget,
    pub depth: u8,
    pub flags: u8,
    pub pane_kind: Option<ChooseTreePaneKind>,
    #[serde(deserialize_with = "deserialize_choose_item_key")]
    pub key: String,
    #[serde(deserialize_with = "deserialize_choose_item_text")]
    pub text: String,
}

fn deserialize_choose_item_key<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_text(deserializer, MAX_CHOOSE_ITEM_KEY_BYTES)
}

fn deserialize_choose_item_text<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_text(deserializer, MAX_CHOOSE_ITEM_TEXT_BYTES)
}

impl ChooseTreeItem {
    pub const EXPANDED: u8 = 1 << 0;
    pub const HAS_CHILDREN: u8 = 1 << 1;
    pub const ACTIVE: u8 = 1 << 2;
    pub const TAGGED: u8 = 1 << 3;

    #[must_use]
    pub const fn expanded(&self) -> bool {
        self.flags & Self::EXPANDED != 0
    }

    #[must_use]
    pub const fn has_children(&self) -> bool {
        self.flags & Self::HAS_CHILDREN != 0
    }

    #[must_use]
    pub const fn active(&self) -> bool {
        self.flags & Self::ACTIVE != 0
    }

    #[must_use]
    pub const fn tagged(&self) -> bool {
        self.flags & Self::TAGGED != 0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChooseTreeSearchState {
    pub query: String,
    pub reverse: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChooseTreeState {
    pub items: Vec<ChooseTreeItem>,
    pub search: Option<ChooseTreeSearchState>,
    pub selected: u32,
    pub kind: ChooseTreeKind,
    pub filter_no_matches: bool,
    #[serde(default, deserialize_with = "deserialize_choose_item_text")]
    pub prompt: String,
    #[serde(default)]
    pub help: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChooseTreeAction {
    Previous,
    Next,
    PagePrevious,
    PageNext,
    First,
    Last,
    Collapse,
    Expand,
    Activate,
    Select(u32),
    ActivateIndex(u32),
    SearchStart { reverse: bool },
    SearchAppend(String),
    SearchBackspace,
    SearchAccept,
    SearchCancel,
    SearchNext { reverse: bool },
    Close,
    Key(KeyInput),
    Tag,
    TagNone,
    TagAll,
    KillCurrent,
    KillTagged,
    SwapUp,
    SwapDown,
    SortNext,
    SortReverse,
    Help,
    CollapseAll,
    ExpandAll,
    Mark,
    MarkClear,
    CommandPrompt,
    PreviewCycle,
    FilterPrompt,
    ClearFilter,
    ClientDetach,
    ClientDetachTagged,
    ClientInfo,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChooseBufferItem {
    pub name: String,
    pub preview: String,
    pub size_bytes: u64,
    pub created_unix_seconds: u64,
    #[serde(deserialize_with = "deserialize_choose_item_key")]
    pub key: String,
    #[serde(deserialize_with = "deserialize_choose_item_text")]
    pub text: String,
    #[serde(default)]
    pub tagged: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChooseBufferSearchState {
    pub query: String,
    pub reverse: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChooseBufferState {
    pub items: Vec<ChooseBufferItem>,
    pub search: Option<ChooseBufferSearchState>,
    pub selected: u32,
    pub filter_no_matches: bool,
    #[serde(default)]
    pub help: bool,
    #[serde(default)]
    pub prompt: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChooserPresentation {
    pub selected: u32,
    pub rows: Vec<ChooserRow>,
    pub sort: String,
    pub view: String,
    pub filter: bool,
    pub selection_style: String,
    pub border_style: String,
    pub prompt_style: String,
    pub preview_size: ChooserPreviewSize,
    pub preview: Option<ChooserPreview>,
    #[serde(default)]
    pub prompt_cursor: PromptCursor,
    #[serde(default)]
    pub prompt_column: u16,
    #[serde(default)]
    pub prompt_line: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChooserRow {
    pub name: String,
    pub text: String,
    pub align: bool,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChooserPreviewSize {
    #[default]
    Normal,
    Off,
    Big,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ChooserPreview {
    Tiles {
        tiles: Vec<ChooserPreviewTile>,
        current: u32,
    },
    Screen {
        #[serde(with = "crate::pane_frame::viewport_bytes")]
        viewport: TerminalViewport,
    },
    Text {
        lines: Vec<String>,
    },
    Client {
        #[serde(with = "crate::pane_frame::optional_viewport_bytes")]
        viewport: Option<TerminalViewport>,
        status: Vec<String>,
        status_style: String,
        status_width: u32,
        border_style: String,
    },
    Markup {
        lines: Vec<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChooserPreviewTile {
    pub label: String,
    pub label_style: String,
    pub border_style: String,
    #[serde(with = "crate::pane_frame::optional_viewport_bytes")]
    pub viewport: Option<TerminalViewport>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChooseBufferAction {
    Previous,
    Next,
    PagePrevious,
    PageNext,
    First,
    Last,
    Paste,
    Delete,
    Select(u32),
    PasteIndex(u32),
    SearchStart { reverse: bool },
    SearchAppend(String),
    SearchBackspace,
    SearchAccept,
    SearchCancel,
    SearchNext { reverse: bool },
    Close,
    Key(KeyInput),
    Tag,
    TagNone,
    TagAll,
    SortNext,
    SortReverse,
    Help,
    CollapseAll,
    ExpandAll,
    DeleteTagged,
    PasteTagged,
    PreviewCycle,
    FilterPrompt,
    ClearFilter,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneIndicator {
    pub pane: PaneId,
    pub index: u32,
    pub select_key: u8,
    pub flags: u8,
    #[serde(deserialize_with = "deserialize_pane_indicator_label")]
    pub label: String,
}

impl PaneIndicator {
    pub const ACTIVE: u8 = 1 << 0;

    #[must_use]
    pub const fn active(&self) -> bool {
        self.flags & Self::ACTIVE != 0
    }

    #[must_use]
    pub const fn selection_key(&self) -> Option<char> {
        if self.select_key == 0 {
            None
        } else {
            Some(self.select_key as char)
        }
    }
}

fn deserialize_pane_indicator_label<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_text(deserializer, MAX_PANE_INDICATOR_LABEL_BYTES)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayPanesState {
    pub window: WindowId,
    pub duration_ms: u32,
    pub indicators: Vec<PaneIndicator>,
    pub colour: Option<crate::TmuxColour>,
    pub active_colour: Option<crate::TmuxColour>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayPanesAction {
    Key(KeyInput),
    Select(PaneId),
    Close,
    Dismiss,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PopupBorderLines {
    #[default]
    Single,
    Double,
    Heavy,
    Simple,
    Rounded,
    Padded,
    None,
}

impl PopupBorderLines {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Double => "double",
            Self::Heavy => "heavy",
            Self::Simple => "simple",
            Self::Rounded => "rounded",
            Self::Padded => "padded",
            Self::None => "none",
        }
    }
}

impl std::str::FromStr for PopupBorderLines {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "single" => Ok(Self::Single),
            "double" => Ok(Self::Double),
            "heavy" => Ok(Self::Heavy),
            "simple" => Ok(Self::Simple),
            "rounded" => Ok(Self::Rounded),
            "padded" => Ok(Self::Padded),
            "none" => Ok(Self::None),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PopupState {
    pub pane: PaneId,
    pub left: u16,
    pub top: u16,
    pub width: u16,
    pub height: u16,
    pub client_columns: u16,
    pub client_rows: u16,
    pub cell_width_px: u32,
    pub cell_height_px: u32,
    pub title: String,
    pub style: String,
    pub border_style: String,
    pub border_lines: PopupBorderLines,
    pub close_on_exit: bool,
    pub close_on_exit_zero: bool,
    pub close_on_any_key: bool,
    pub dead: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PopupAction {
    Text(String),
    Key {
        input: KeyInput,
        text_follows: bool,
    },
    TerminalView(TerminalViewAction),
    Close,
    Pointer {
        pointer: PopupPointer,
        view: Option<TerminalViewAction>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PopupPointer {
    pub column: u16,
    pub row: u16,
    pub button: PopupPointerButton,
    pub drag: bool,
    pub release: bool,
    pub meta: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PopupPointerButton {
    #[default]
    None,
    Left,
    Middle,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MenuItem {
    pub name: String,
    pub key: Option<String>,
    pub annotation: Option<String>,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MenuState {
    pub left: u16,
    pub top: u16,
    pub width: u16,
    pub height: u16,
    pub client_columns: u16,
    pub client_rows: u16,
    pub cell_width_px: u32,
    pub cell_height_px: u32,
    pub title: String,
    pub style: String,
    pub selected_style: String,
    pub border_style: String,
    pub border_lines: PopupBorderLines,
    pub items: Vec<Option<MenuItem>>,
    pub selected: Option<u32>,
    pub stay_open: bool,
    #[serde(default)]
    pub mouse_keys: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MenuAction {
    Choose(u32),
    Cancel,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmState {
    pub prompt: String,
    pub confirm_key: u8,
    pub default_yes: bool,
    pub prompt_cursor: PromptCursor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfirmAction {
    Reply(bool),
}
