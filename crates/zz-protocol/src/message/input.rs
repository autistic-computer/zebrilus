use super::*;

fn deserialize_gui_text<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_text(deserializer, MAX_GUI_TEXT_BYTES)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyToken {
    Literal(String),
    Named(String),
    Raw(u8),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputMessage {
    Text {
        pane: PaneId,
        text: String,
    },
    Key {
        pane: PaneId,
        input: KeyInput,
        text_follows: bool,
    },
    BrowserSurfaceText {
        pane: PaneId,
        text: String,
    },
    BrowserSurfaceKey {
        pane: PaneId,
        input: KeyInput,
        text_follows: bool,
    },
    ResizeTerminal {
        pane: PaneId,
        columns: u16,
        rows: u16,
        cell_width_px: u32,
        cell_height_px: u32,
    },
    TerminalView {
        pane: PaneId,
        action: TerminalViewAction,
    },
    ResizeCommandOutput {
        columns: u16,
        rows: u16,
        cell_width_px: u32,
        cell_height_px: u32,
    },
    CommandOutputView {
        action: TerminalViewAction,
    },
    ChooseTree {
        action: ChooseTreeAction,
    },
    ChooseBuffer {
        action: ChooseBufferAction,
    },
    DisplayPanes {
        action: DisplayPanesAction,
    },
    CommandPrompt {
        action: CommandPromptAction,
    },
    ResizeSplit {
        window: WindowId,
        split: SplitId,
        ratio_basis_points: u16,
    },
    CancelPrefix {
        request_id: u64,
    },
    #[serde(rename = "Popup")]
    RetiredPopup {
        action: PopupAction,
    },
    Menu {
        action: MenuAction,
    },
    Confirm {
        action: ConfirmAction,
    },
    ClientTerminalSize {
        columns: u16,
        rows: u16,
    },
    ClientFocus {
        focused: bool,
    },
    DismissClientMessage,
    MouseKey {
        key: String,
        pane: Option<PaneId>,
        window: Option<WindowId>,
        column: u16,
        row: u16,
        border: Option<Axis>,
        view_action: Option<TerminalViewAction>,
        press_action: Option<TerminalViewAction>,
        #[serde(default)]
        status_range_start: Option<u16>,
        #[serde(default)]
        press: Option<(u16, u16)>,
    },
    ClientSuspendState {
        suspended: bool,
    },
    ResizeTerminalV2 {
        pane: PaneId,
        columns: u16,
        rows: u16,
        cell_width_px: u32,
        cell_height_px: u32,
        layout_generation: u64,
    },
    ClientTerminalSizeV2 {
        columns: u16,
        rows: u16,
        layout_generation: u64,
    },
    ClipboardReply {
        data: Vec<u8>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandPromptAction {
    Update { input: String, cursor: u32 },
    Submit { input: String },
    Close,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrowserCommand {
    Navigate(String),
    Reload,
    Back,
    Forward,
    SendKeys(Vec<KeyToken>),
    Key(KeyInput),
    Screenshot {
        request_id: u64,
        #[serde(deserialize_with = "deserialize_gui_text")]
        path: String,
    },
    SendKeysRepeated {
        keys: Vec<KeyToken>,
        count: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SimulatorButton {
    Home,
    Lock,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SimulatorCommand {
    Screenshot {
        request_id: u64,
        #[serde(deserialize_with = "deserialize_gui_text")]
        path: String,
    },
    Tap {
        request_id: u64,
        x: u32,
        y: u32,
    },
    Swipe {
        request_id: u64,
        from: (u32, u32),
        to: (u32, u32),
    },
    Type {
        request_id: u64,
        #[serde(deserialize_with = "deserialize_gui_text")]
        text: String,
    },
    Press {
        request_id: u64,
        button: SimulatorButton,
    },
}

impl SimulatorCommand {
    #[must_use]
    pub const fn request_id(&self) -> u64 {
        match self {
            Self::Screenshot { request_id, .. }
            | Self::Tap { request_id, .. }
            | Self::Swipe { request_id, .. }
            | Self::Type { request_id, .. }
            | Self::Press { request_id, .. } => *request_id,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GuiResponse {
    Success {
        request_id: u64,
        #[serde(deserialize_with = "deserialize_gui_text")]
        output: String,
    },
    Error {
        request_id: u64,
        #[serde(deserialize_with = "deserialize_gui_text")]
        message: String,
    },
}

impl GuiResponse {
    #[must_use]
    pub const fn request_id(&self) -> u64 {
        match self {
            Self::Success { request_id, .. } | Self::Error { request_id, .. } => *request_id,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        let text = match self {
            Self::Success { output, .. } => output,
            Self::Error { message, .. } => message,
        };
        if text.len() > MAX_GUI_TEXT_BYTES {
            return Err("GUI response text exceeds the wire byte limit");
        }
        Ok(())
    }
}
