use std::time::{Duration, Instant};

use zz_client::{
    ChromeAction, Disposition, Effect, MenuKeyResult, MenuPointerKind, PrefixView, resolve_menu_key,
};
use zz_daemon_client::{
    Endpoint, InteractiveClient, configured_fleet_hosts, validate_fleet_host, write_fleet_host,
};
use zz_protocol::{
    ChooseBufferAction, ChooseTreeAction, CommandInvocation, CommandPromptAction,
    CommandPromptMode, ConfirmAction, ConfirmState, DisplayPanesAction, InputMessage,
    MAX_COMMAND_PROMPT_BYTES, MenuAction, MenuState, PaneKindSnapshot,
};
use zz_terminal::{
    KeyAction, KeyCode, KeyInput, Modifiers, PointerCellEvent, SearchQuery, TerminalColorScheme,
    TerminalMode, TerminalMouseButton, TerminalMouseInput, TerminalMousePhase, TerminalViewAction,
};

use crate::{
    browser::BrowserZoomStep,
    browser::{
        BrowserState, ProviderModifiers, ProviderPointerButton, ProviderPointerInput,
        ProviderPointerPhase,
    },
    layout::Rect,
    picker::{self, Action as PickerAction},
    sidebar::{self, EditKind as SidebarEditKind, Target as SidebarTarget},
    state::{ClientMessage, HostSwitch, Model},
    terminal_event::{
        Event as TerminalEvent, KeyCode as TerminalKeyCode, KeyEvent, KeyEventKind, KeyModifiers,
        MouseButton, MouseEvent, MouseEventKind,
    },
};

pub(crate) enum InputOutcome {
    None,
    Repaint,
    RepaintAll,
    Resize(crate::tty::TerminalSize),
    AttachRequested,
    SwitchHost(HostSwitch),
}

const MAX_COMMAND_OUTPUT_SEARCH_BYTES: usize = 4096;

pub(crate) fn handle(
    model: &mut Model,
    client: &InteractiveClient,
    browser: &mut BrowserState,
    event: TerminalEvent,
    pixel_mouse: bool,
    prefix: PrefixView,
) -> Result<InputOutcome, String> {
    if let TerminalEvent::Clipboard(data) = event {
        client
            .send_input(InputMessage::ClipboardReply { data })
            .map_err(|error| error.to_string())?;
        return Ok(InputOutcome::None);
    }
    crate::overlay::dismiss_client_message(model, client, &event)?;
    let menu_box = model.menu_box();
    let event = match menu_input_route(
        model.menu.as_ref(),
        model.menu_selection,
        model.menu_action_pending,
        menu_box,
        pixel_mouse_position(model, pixel_mouse),
        &mut model.menu_swallowed_key,
        event,
    ) {
        MenuInputRoute::Forward(event) => event,
        MenuInputRoute::Consume => return Ok(InputOutcome::None),
        MenuInputRoute::Select(selection) => {
            model.menu_selection = selection;
            return Ok(InputOutcome::Repaint);
        }
        MenuInputRoute::Action {
            action,
            swallowed_key,
        } => {
            client
                .send_input(InputMessage::Menu { action })
                .map_err(|error| error.to_string())?;
            model.menu_action_pending = true;
            model.menu_swallowed_key = swallowed_key;
            return Ok(InputOutcome::None);
        }
        MenuInputRoute::Paste {
            selection,
            action,
            remainder,
        } => {
            model.menu_selection = selection;
            let Some(action) = action else {
                return Ok(InputOutcome::Repaint);
            };
            client
                .send_input(InputMessage::Menu { action })
                .map_err(|error| error.to_string())?;
            model.menu_action_pending = true;
            model.menu_swallowed_key = None;
            if remainder.is_empty() {
                return Ok(InputOutcome::None);
            }
            return handle_paste(model, client, remainder);
        }
    };
    let event = match confirm_input_route(
        model.confirm.as_ref(),
        model.confirm_reply_pending,
        &mut model.confirm_swallowed_key,
        event,
    ) {
        ConfirmInputRoute::Forward(event) => event,
        ConfirmInputRoute::Consume => return Ok(InputOutcome::None),
        ConfirmInputRoute::Reply {
            accepted,
            swallowed_key,
        } => {
            client
                .send_input(InputMessage::Confirm {
                    action: ConfirmAction::Reply(accepted),
                })
                .map_err(|error| error.to_string())?;
            model.confirm_reply_pending = true;
            model.confirm_swallowed_key = swallowed_key;
            return Ok(InputOutcome::None);
        }
    };
    match event {
        TerminalEvent::CellSize {
            width_px,
            height_px,
        } => Ok(InputOutcome::Resize(
            model.size.with_cell_pixels(width_px, height_px),
        )),
        TerminalEvent::Clipboard(_)
        | TerminalEvent::DeviceAttributes
        | TerminalEvent::KittyGraphicsResponse { .. } => Ok(InputOutcome::None),
        TerminalEvent::DarkTheme | TerminalEvent::LightTheme => {
            let scheme = if matches!(event, TerminalEvent::DarkTheme) {
                TerminalColorScheme::Dark
            } else {
                TerminalColorScheme::Light
            };
            client
                .set_color_scheme(scheme)
                .map_err(|error| error.to_string())?;
            Ok(InputOutcome::None)
        }
        TerminalEvent::SecondaryDeviceAttributes(kind) => {
            crate::tty::note_secondary_device_attributes(kind);
            Ok(InputOutcome::None)
        }
        TerminalEvent::SynchronizedOutput(supported) => {
            if supported {
                crate::tty::note_synchronized_output();
            }
            Ok(InputOutcome::None)
        }
        TerminalEvent::ExtendedDeviceAttributes(name) => {
            crate::tty::note_extended_device_attributes(&name);
            Ok(InputOutcome::None)
        }
        TerminalEvent::FocusGained => {
            send_focus(model, client, true)?;
            Ok(InputOutcome::None)
        }
        TerminalEvent::FocusLost => {
            send_focus(model, client, false)?;
            Ok(InputOutcome::None)
        }
        TerminalEvent::Paste(text) => handle_paste(model, client, text),
        TerminalEvent::Mouse(event) => handle_mouse(model, client, browser, event, pixel_mouse),
        TerminalEvent::Key(event) => handle_key(model, client, browser, event, prefix),
    }
}

pub(crate) fn plain_key_pane(
    model: &Model,
    browser: &BrowserState,
    prefix_armed: bool,
    releases: bool,
) -> Option<zz_protocol::PaneId> {
    if prefix_armed
        || model.menu.is_some()
        || model.menu_action_pending
        || model.menu_swallowed_key.is_some()
        || model.confirm.is_some()
        || model.confirm_reply_pending
        || model.confirm_swallowed_key.is_some()
        || model.sidebar_edit.is_some()
        || model.command_prompt.is_some()
        || model.choose_tree.is_some()
        || model.choose_buffer.is_some()
        || model.display_panes.is_some()
        || model.command_output_focus().is_some()
        || model.click.is_some()
        || model.sidebar_focused()
        || !model.router.passes_plain_keys(releases)
    {
        return None;
    }
    let pane = model.active_pane()?;
    (matches!(model.pane_snapshot(pane)?.kind, PaneKindSnapshot::Terminal)
        && !browser.has_surface(pane))
    .then_some(pane)
}

#[derive(Debug, Eq, PartialEq)]
enum MenuInputRoute {
    Forward(TerminalEvent),
    Consume,
    Select(Option<usize>),
    Action {
        action: MenuAction,
        swallowed_key: Option<KeyCode>,
    },
    Paste {
        selection: Option<usize>,
        action: Option<MenuAction>,
        remainder: String,
    },
}

fn menu_pointer_kind(kind: MouseEventKind) -> MenuPointerKind {
    match kind {
        MouseEventKind::Down(_) => MenuPointerKind::Press,
        MouseEventKind::Up(_) => MenuPointerKind::Release,
        MouseEventKind::Drag(_) => MenuPointerKind::Drag,
        MouseEventKind::Moved => MenuPointerKind::Motion,
        MouseEventKind::ScrollUp
        | MouseEventKind::ScrollDown
        | MouseEventKind::ScrollLeft
        | MouseEventKind::ScrollRight => MenuPointerKind::Wheel,
    }
}

fn menu_pointer_buttons(kind: MouseEventKind) -> u8 {
    match kind {
        MouseEventKind::Down(button) | MouseEventKind::Drag(button) => match button {
            MouseButton::Left => 0,
            MouseButton::Middle => 1,
            MouseButton::Right => 2,
        },
        MouseEventKind::Up(_) | MouseEventKind::Moved => 3,
        MouseEventKind::ScrollUp => 64,
        MouseEventKind::ScrollDown => 65,
        MouseEventKind::ScrollLeft => 66,
        MouseEventKind::ScrollRight => 67,
    }
}

fn menu_input_route(
    state: Option<&MenuState>,
    selection: Option<usize>,
    action_pending: bool,
    menu_box: Option<zz_client::MenuBox>,
    pointer_cells: PointerCells,
    swallowed_key: &mut Option<KeyCode>,
    event: TerminalEvent,
) -> MenuInputRoute {
    if let TerminalEvent::Key(key) = &event
        && swallow_overlay_key(swallowed_key, *key)
    {
        return MenuInputRoute::Consume;
    }
    let Some(state) = state else {
        return MenuInputRoute::Forward(event);
    };
    if action_pending
        && matches!(
            event,
            TerminalEvent::Key(_) | TerminalEvent::Paste(_) | TerminalEvent::Mouse(_)
        )
    {
        return MenuInputRoute::Consume;
    }
    match event {
        TerminalEvent::Key(event) => match resolve_menu_key(state, selection, &key_input(event)) {
            MenuKeyResult::Action(action) => MenuInputRoute::Action {
                action,
                swallowed_key: Some(key_code(event.code)),
            },
            MenuKeyResult::Select(selection) => MenuInputRoute::Select(selection),
            MenuKeyResult::Consumed => MenuInputRoute::Consume,
        },
        TerminalEvent::Paste(text) => {
            let paste = zz_client::resolve_menu_paste(state, selection, &text);
            MenuInputRoute::Paste {
                selection: paste.selection,
                action: paste.action,
                remainder: paste.remainder,
            }
        }
        TerminalEvent::Mouse(event) => {
            let Some(menu_box) = menu_box else {
                return MenuInputRoute::Consume;
            };
            let (column, row) = pointer_cells.cells(event);
            match zz_client::resolve_menu_mouse(
                state,
                selection,
                menu_box,
                menu_pointer_kind(event.kind),
                menu_pointer_buttons(event.kind),
                column,
                row,
            ) {
                MenuKeyResult::Action(action) => MenuInputRoute::Action {
                    action,
                    swallowed_key: None,
                },
                MenuKeyResult::Select(selection) => MenuInputRoute::Select(selection),
                MenuKeyResult::Consumed => MenuInputRoute::Consume,
            }
        }
        event => MenuInputRoute::Forward(event),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PointerCells {
    pixels: bool,
    cell_width_px: u32,
    cell_height_px: u32,
}

impl PointerCells {
    fn cells(self, event: MouseEvent) -> (u16, u16) {
        if !self.pixels {
            return (event.column, event.row);
        }
        (
            u16::try_from(u32::from(event.column) / self.cell_width_px.max(1)).unwrap_or(u16::MAX),
            u16::try_from(u32::from(event.row) / self.cell_height_px.max(1)).unwrap_or(u16::MAX),
        )
    }
}

fn pixel_mouse_position(model: &Model, pixel_mouse: bool) -> PointerCells {
    PointerCells {
        pixels: pixel_mouse,
        cell_width_px: model.size.cell_width_px,
        cell_height_px: model.size.cell_height_px,
    }
}

#[derive(Debug, Eq, PartialEq)]
enum ConfirmInputRoute {
    Forward(TerminalEvent),
    Consume,
    Reply {
        accepted: bool,
        swallowed_key: Option<KeyCode>,
    },
}

fn confirm_input_route(
    state: Option<&ConfirmState>,
    reply_pending: bool,
    swallowed_key: &mut Option<KeyCode>,
    event: TerminalEvent,
) -> ConfirmInputRoute {
    if let TerminalEvent::Key(key) = &event
        && swallow_overlay_key(swallowed_key, *key)
    {
        return ConfirmInputRoute::Consume;
    }
    let Some(state) = state else {
        return ConfirmInputRoute::Forward(event);
    };
    if reply_pending
        && matches!(
            event,
            TerminalEvent::Key(_) | TerminalEvent::Paste(_) | TerminalEvent::Mouse(_)
        )
    {
        return ConfirmInputRoute::Consume;
    }
    match event {
        TerminalEvent::Key(event) if event.kind == KeyEventKind::Release => {
            ConfirmInputRoute::Consume
        }
        TerminalEvent::Key(event) => match confirm_reply(state, event) {
            Some(accepted) => ConfirmInputRoute::Reply {
                accepted,
                swallowed_key: Some(key_code(event.code)),
            },
            None => ConfirmInputRoute::Consume,
        },
        TerminalEvent::Paste(_) | TerminalEvent::Mouse(_) => ConfirmInputRoute::Consume,
        event => ConfirmInputRoute::Forward(event),
    }
}

fn swallow_overlay_key(swallowed_key: &mut Option<KeyCode>, event: KeyEvent) -> bool {
    let Some(swallowed) = *swallowed_key else {
        return false;
    };
    if key_code(event.code) != swallowed {
        return false;
    }
    match event.kind {
        KeyEventKind::Repeat => true,
        KeyEventKind::Release => {
            *swallowed_key = None;
            true
        }
        KeyEventKind::Press => {
            *swallowed_key = None;
            false
        }
    }
}

fn confirm_reply(state: &ConfirmState, event: KeyEvent) -> Option<bool> {
    match event.code {
        TerminalKeyCode::Enter => Some(state.default_yes),
        TerminalKeyCode::Char(character) => Some(
            !event.modifiers.contains(KeyModifiers::CONTROL)
                && character.is_ascii()
                && character as u8 == state.confirm_key,
        ),
        TerminalKeyCode::Backspace | TerminalKeyCode::Tab | TerminalKeyCode::Esc => Some(false),
        _ => None,
    }
}

fn handle_key(
    model: &mut Model,
    client: &InteractiveClient,
    browser: &mut BrowserState,
    event: KeyEvent,
    prefix: PrefixView,
) -> Result<InputOutcome, String> {
    if model.sidebar_edit.is_some() && model.command_output_focus().is_none() {
        return handle_sidebar_edit_key(model, client, event);
    }

    if model.command_prompt.is_some() {
        return handle_command_prompt(model, client, event);
    }
    if model.choose_tree.is_some() {
        client
            .send_input(InputMessage::ChooseTree {
                action: ChooseTreeAction::Key(key_input(event)),
            })
            .map_err(|error| error.to_string())?;
        return Ok(InputOutcome::None);
    }
    if model.choose_buffer.is_some() {
        client
            .send_input(InputMessage::ChooseBuffer {
                action: ChooseBufferAction::Key(key_input(event)),
            })
            .map_err(|error| error.to_string())?;
        return Ok(InputOutcome::None);
    }
    if model.display_panes.is_some() {
        if event.kind != KeyEventKind::Release {
            client
                .send_input(InputMessage::DisplayPanes {
                    action: DisplayPanesAction::Key(key_input(event)),
                })
                .map_err(|error| error.to_string())?;
        }
        return Ok(InputOutcome::None);
    }
    if let Some(pane) = model.command_output_focus() {
        match command_output_key_route(
            &mut model.command_output_search,
            &mut model.command_output_swallowed_key,
            pane,
            event,
        ) {
            CommandOutputKeyRoute::Search(edit) => {
                if let Some(action) = edit.action {
                    client
                        .send_input(InputMessage::CommandOutputView { action })
                        .map_err(|error| error.to_string())?;
                }
                return Ok(if edit.repaint {
                    if model.command_output_search.is_some() {
                        InputOutcome::Repaint
                    } else {
                        InputOutcome::RepaintAll
                    }
                } else {
                    InputOutcome::None
                });
            }
            CommandOutputKeyRoute::Swallowed => return Ok(InputOutcome::None),
            CommandOutputKeyRoute::Forward(input) => {
                if let Some(input) = input {
                    client
                        .send_input(input)
                        .map_err(|error| error.to_string())?;
                }
            }
        }
        return Ok(InputOutcome::None);
    }

    if let Some(pane) = model.active_picker() {
        return handle_picker_key(model, client, pane, event);
    }

    handle_routed_key(
        model,
        browser,
        event,
        prefix,
        |input| client.send_input(input).map_err(|error| error.to_string()),
        |model, target| activate_sidebar_target(model, client, target),
    )
}

fn handle_routed_key(
    model: &mut Model,
    browser: &mut BrowserState,
    event: KeyEvent,
    prefix: PrefixView,
    mut send: impl FnMut(InputMessage) -> Result<(), String>,
    mut activate: impl FnMut(&mut Model, SidebarTarget) -> Result<InputOutcome, String>,
) -> Result<InputOutcome, String> {
    let input = key_input(event);
    let disposition = model.router.key(&input, prefix);
    let mut outcome = InputOutcome::None;
    let mut forward = disposition == Disposition::Native && !model.sidebar_focused();
    for effect in model.router.drain_effects() {
        match effect {
            Effect::ForwardKey { pane, input } => send(InputMessage::Key {
                pane,
                input,
                text_follows: false,
            })?,
            Effect::RequestFocus(owner) => model.apply_input_focus(owner),
            Effect::Chrome(action) => {
                outcome = InputOutcome::Repaint;
                match action {
                    ChromeAction::SidebarSelectUp => model.move_sidebar_selection(-1),
                    ChromeAction::SidebarSelectDown => model.move_sidebar_selection(1),
                    ChromeAction::SidebarConfirm => {
                        if let Some(target) = model.selected_sidebar_target() {
                            outcome = activate(model, target)?;
                        }
                    }
                    ChromeAction::SidebarRename => model.begin_sidebar_rename(),
                    ChromeAction::SidebarCancel => model.focus_active_pane(),
                    ChromeAction::ToggleSidebar => {
                        if model.hide_sidebar() {
                            outcome = InputOutcome::RepaintAll;
                        }
                    }
                    ChromeAction::BrowserZoomIn
                    | ChromeAction::BrowserZoomOut
                    | ChromeAction::BrowserZoomReset => {
                        let step = match action {
                            ChromeAction::BrowserZoomIn => BrowserZoomStep::In,
                            ChromeAction::BrowserZoomOut => BrowserZoomStep::Out,
                            _ => BrowserZoomStep::Reset,
                        };
                        forward = event.kind != KeyEventKind::Press
                            || !model
                                .active_pane()
                                .is_some_and(|pane| browser.zoom(pane, step));
                        outcome = InputOutcome::None;
                    }
                    _ => outcome = InputOutcome::None,
                }
            }
        }
    }
    if forward && let Some(pane) = model.active_pane() {
        let kitty_keyboard = model
            .viewports
            .get(&pane)
            .is_some_and(|viewport| viewport.kitty_keyboard);
        if should_forward_key(event.kind, browser.has_surface(pane), kitty_keyboard) {
            send(InputMessage::Key {
                pane,
                input,
                text_follows: false,
            })?;
        }
    }
    Ok(outcome)
}

fn command_output_key_input(pane: zz_protocol::PaneId, event: KeyEvent) -> Option<InputMessage> {
    (event.kind != KeyEventKind::Release).then(|| InputMessage::Key {
        pane,
        input: key_input(event),
        text_follows: false,
    })
}

enum CommandOutputKeyRoute {
    Search(CommandOutputSearchEdit),
    Swallowed,
    Forward(Option<InputMessage>),
}

fn command_output_key_route(
    search: &mut Option<SearchQuery>,
    swallowed_key: &mut Option<KeyCode>,
    pane: zz_protocol::PaneId,
    event: KeyEvent,
) -> CommandOutputKeyRoute {
    if search.is_some() {
        let edit = command_output_search_key(search, event);
        if let Some(key) = edit.swallowed_key {
            *swallowed_key = Some(key);
        }
        return CommandOutputKeyRoute::Search(edit);
    }
    if swallow_command_output_key(swallowed_key, event) {
        CommandOutputKeyRoute::Swallowed
    } else {
        CommandOutputKeyRoute::Forward(command_output_key_input(pane, event))
    }
}

fn swallow_command_output_key(swallowed_key: &mut Option<KeyCode>, event: KeyEvent) -> bool {
    let Some(swallowed) = *swallowed_key else {
        return false;
    };
    if key_input(event).key != swallowed {
        return false;
    }
    match event.kind {
        KeyEventKind::Repeat => true,
        KeyEventKind::Release => {
            *swallowed_key = None;
            true
        }
        KeyEventKind::Press => {
            *swallowed_key = None;
            false
        }
    }
}

#[derive(Debug, Default, Eq, PartialEq)]
struct CommandOutputSearchEdit {
    action: Option<TerminalViewAction>,
    repaint: bool,
    swallowed_key: Option<KeyCode>,
}

fn command_output_search_key(
    search: &mut Option<SearchQuery>,
    event: KeyEvent,
) -> CommandOutputSearchEdit {
    if event.kind == KeyEventKind::Release {
        return CommandOutputSearchEdit::default();
    }
    match event.code {
        TerminalKeyCode::Esc if event.kind == KeyEventKind::Press => {
            *search = None;
            CommandOutputSearchEdit {
                action: Some(TerminalViewAction::SearchClose),
                repaint: true,
                swallowed_key: Some(KeyCode::Escape),
            }
        }
        TerminalKeyCode::Enter if event.kind == KeyEventKind::Press => {
            *search = None;
            CommandOutputSearchEdit {
                action: None,
                repaint: true,
                swallowed_key: Some(KeyCode::Enter),
            }
        }
        TerminalKeyCode::Backspace => {
            let query = search.as_mut().expect("search is active");
            query.text.pop();
            CommandOutputSearchEdit {
                action: Some(TerminalViewAction::SearchUpdate(query.clone())),
                repaint: true,
                swallowed_key: None,
            }
        }
        TerminalKeyCode::Char(character)
            if !character.is_control()
                && !event.modifiers.intersects(
                    KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER,
                ) =>
        {
            let mut encoded = [0; 4];
            let Some(query) =
                append_command_output_search(search, character.encode_utf8(&mut encoded))
            else {
                return CommandOutputSearchEdit::default();
            };
            CommandOutputSearchEdit {
                action: Some(TerminalViewAction::SearchUpdate(query)),
                repaint: true,
                swallowed_key: None,
            }
        }
        _ => CommandOutputSearchEdit::default(),
    }
}

fn command_output_search_paste(
    search: &mut Option<SearchQuery>,
    text: &str,
) -> Option<TerminalViewAction> {
    append_command_output_search(search, text).map(TerminalViewAction::SearchUpdate)
}

fn append_command_output_search(
    search: &mut Option<SearchQuery>,
    text: &str,
) -> Option<SearchQuery> {
    let query = search.as_mut()?;
    let mut changed = false;
    for character in text.chars().filter(|character| !character.is_control()) {
        if query.text.len().saturating_add(character.len_utf8()) > MAX_COMMAND_OUTPUT_SEARCH_BYTES {
            break;
        }
        query.text.push(character);
        changed = true;
    }
    changed.then(|| query.clone())
}

const fn should_forward_key(
    kind: KeyEventKind,
    browser_surface: bool,
    kitty_keyboard: bool,
) -> bool {
    !matches!(kind, KeyEventKind::Release) || browser_surface || kitty_keyboard
}

fn activate_sidebar_target(
    model: &mut Model,
    client: &InteractiveClient,
    target: SidebarTarget,
) -> Result<InputOutcome, String> {
    apply_sidebar_target(model, target, |target| match target {
        SidebarTarget::Session(session) => client
            .attach(session.to_string())
            .map(drop)
            .map_err(|error| error.to_string()),
        SidebarTarget::Window(window) => {
            execute_target(client, "select-window", window.to_string())
        }
        SidebarTarget::Pane(pane) => execute_target(client, "select-pane", pane.to_string()),
        SidebarTarget::NewPane(target) => client
            .execute(CommandInvocation::new(
                "split-window",
                [
                    "--kind".to_owned(),
                    "picker".to_owned(),
                    "-t".to_owned(),
                    target.to_string(),
                ],
            ))
            .map(drop)
            .map_err(|error| error.to_string()),
        _ => Ok(()),
    })
}

fn apply_sidebar_target(
    model: &mut Model,
    target: SidebarTarget,
    send: impl FnOnce(SidebarTarget) -> Result<(), String>,
) -> Result<InputOutcome, String> {
    send(target)?;
    match target {
        SidebarTarget::Session(_) => {
            model.begin_client_focus_attach();
            Ok(InputOutcome::AttachRequested)
        }
        SidebarTarget::Pane(pane) | SidebarTarget::NewPane(pane) => {
            model.activate_pane(pane);
            Ok(InputOutcome::Repaint)
        }
        SidebarTarget::Window(_) => Ok(InputOutcome::Repaint),
        SidebarTarget::LocalHost | SidebarTarget::FleetHost(_) => Ok(model
            .host_switch(target)
            .map_or(InputOutcome::Repaint, InputOutcome::SwitchHost)),
        SidebarTarget::AddHost => {
            model.begin_add_host();
            Ok(InputOutcome::Repaint)
        }
    }
}

fn execute_target(
    client: &InteractiveClient,
    command: &'static str,
    target: String,
) -> Result<(), String> {
    client
        .execute(CommandInvocation::new(command, ["-t".to_owned(), target]))
        .map(drop)
        .map_err(|error| error.to_string())
}

fn handle_sidebar_edit_key(
    model: &mut Model,
    client: &InteractiveClient,
    event: KeyEvent,
) -> Result<InputOutcome, String> {
    if event.kind == KeyEventKind::Release {
        return Ok(InputOutcome::None);
    }
    match event.code {
        TerminalKeyCode::Esc => model.sidebar_edit = None,
        TerminalKeyCode::Enter => commit_sidebar_edit(model, client)?,
        TerminalKeyCode::Left => model
            .sidebar_edit
            .as_mut()
            .expect("sidebar editor checked above")
            .move_left(),
        TerminalKeyCode::Right => model
            .sidebar_edit
            .as_mut()
            .expect("sidebar editor checked above")
            .move_right(),
        TerminalKeyCode::Home => model
            .sidebar_edit
            .as_mut()
            .expect("sidebar editor checked above")
            .move_home(),
        TerminalKeyCode::End => model
            .sidebar_edit
            .as_mut()
            .expect("sidebar editor checked above")
            .move_end(),
        TerminalKeyCode::Backspace => model
            .sidebar_edit
            .as_mut()
            .expect("sidebar editor checked above")
            .backspace(),
        TerminalKeyCode::Delete => model
            .sidebar_edit
            .as_mut()
            .expect("sidebar editor checked above")
            .delete(),
        TerminalKeyCode::Char(character)
            if !event
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER) =>
        {
            model
                .sidebar_edit
                .as_mut()
                .expect("sidebar editor checked above")
                .insert_char(character);
        }
        _ => {}
    }
    Ok(InputOutcome::Repaint)
}

fn commit_sidebar_edit(model: &mut Model, client: &InteractiveClient) -> Result<(), String> {
    let Some(edit) = model.sidebar_edit.as_ref() else {
        return Ok(());
    };
    let kind = edit.kind;
    let buffer = edit.buffer.clone();
    match kind {
        SidebarEditKind::RenameSession(_) | SidebarEditKind::RenameWindow(_) => {
            let Some(command) = rename_command(kind, &buffer) else {
                model.sidebar_edit = None;
                return Ok(());
            };
            client
                .execute(command)
                .map(drop)
                .map_err(|error| error.to_string())?;
            model.sidebar_edit = None;
        }
        SidebarEditKind::AddHost => {
            let (name, endpoint) = match parse_add_host_input(&buffer) {
                Ok(parsed) => parsed,
                Err(error) => {
                    model.client_message = Some(ClientMessage::local(error));
                    return Ok(());
                }
            };
            if let Err(error) = write_fleet_host(&name, &endpoint) {
                model.client_message = Some(ClientMessage::local(format!(
                    "could not write zz/config: {error}"
                )));
                return Ok(());
            }
            let (hosts, _) = match configured_fleet_hosts() {
                Ok(configured) => configured,
                Err(error) => {
                    model.client_message = Some(ClientMessage::local(format!(
                        "could not read zz/config: {error}"
                    )));
                    return Ok(());
                }
            };
            model.refresh_fleet_hosts(hosts);
            model.sidebar_edit = None;
            model.client_message = Some(ClientMessage::local(format!("host {name} added")));
        }
    }
    Ok(())
}

fn rename_command(kind: SidebarEditKind, name: &str) -> Option<CommandInvocation> {
    if name.is_empty() {
        return None;
    }
    let (command, target) = match kind {
        SidebarEditKind::RenameSession(session) => ("rename-session", session.to_string()),
        SidebarEditKind::RenameWindow(window) => ("rename-window", window.to_string()),
        SidebarEditKind::AddHost => return None,
    };
    Some(CommandInvocation::new(
        command,
        ["-t".to_owned(), target, name.to_owned()],
    ))
}

fn parse_add_host_input(input: &str) -> Result<(String, String), String> {
    let input = input.trim();
    let Some((name, destination)) = input.split_once(' ') else {
        return Err("add host expects <name> <ssh-destination>".to_owned());
    };
    let destination = destination.trim();
    if name.is_empty() || destination.is_empty() {
        return Err("add host expects <name> <ssh-destination>".to_owned());
    }
    let destination = destination.strip_prefix("ssh://").unwrap_or(destination);
    if destination.starts_with('-') {
        return Err("ssh destination must not start with `-`".to_owned());
    }
    let endpoint = format!("ssh://{destination}");
    Endpoint::parse(&endpoint).map_err(|error| error.to_string())?;
    validate_fleet_host(name, &endpoint)?;
    Ok((name.to_owned(), endpoint))
}

fn handle_picker_key(
    model: &mut Model,
    client: &InteractiveClient,
    pane: zz_protocol::PaneId,
    event: KeyEvent,
) -> Result<InputOutcome, String> {
    let Some(action) = picker::key_action(event, model.picker_selection) else {
        return Ok(InputOutcome::None);
    };
    match action {
        PickerAction::Previous => model.move_picker_selection(-1),
        PickerAction::Next => model.move_picker_selection(1),
        PickerAction::Materialize(choice) => {
            client
                .execute(CommandInvocation::new(
                    "select-pane-kind",
                    [
                        "-t".to_owned(),
                        pane.to_string(),
                        choice.argument().to_owned(),
                    ],
                ))
                .map(drop)
                .map_err(|error| error.to_string())?;
        }
        PickerAction::Cancel => {
            execute_target(client, "kill-pane", pane.to_string())?;
        }
    }
    Ok(InputOutcome::Repaint)
}

fn handle_command_prompt(
    model: &mut Model,
    client: &InteractiveClient,
    event: KeyEvent,
) -> Result<InputOutcome, String> {
    if event.kind == KeyEventKind::Release {
        return Ok(InputOutcome::None);
    }
    if let Some(pane) = model.active_pane() {
        client
            .send_input(InputMessage::Key {
                pane,
                input: key_input(event),
                text_follows: false,
            })
            .map_err(|error| error.to_string())?;
    }
    Ok(InputOutcome::None)
}

fn scalar_byte_index(text: &str, scalar: u32) -> usize {
    text.char_indices()
        .nth(usize::try_from(scalar).unwrap_or(usize::MAX))
        .map_or(text.len(), |(index, _)| index)
}

fn handle_paste(
    model: &mut Model,
    client: &InteractiveClient,
    text: String,
) -> Result<InputOutcome, String> {
    let command_output_focused = model.command_output_focus().is_some();
    if command_output_focused
        && let Some(action) = command_output_search_paste(&mut model.command_output_search, &text)
    {
        client
            .send_input(InputMessage::CommandOutputView { action })
            .map_err(|error| error.to_string())?;
        return Ok(InputOutcome::Repaint);
    }
    if command_output_focused {
        return Ok(InputOutcome::None);
    }
    if let Some(edit) = model.sidebar_edit.as_mut() {
        edit.insert_text(&text);
        return Ok(InputOutcome::Repaint);
    }
    if model.sidebar_focused() || model.active_picker().is_some() {
        return Ok(InputOutcome::None);
    }
    if let Some(state) = model.command_prompt.as_mut() {
        if matches!(
            state.mode,
            CommandPromptMode::Single | CommandPromptMode::Numeric | CommandPromptMode::Key
        ) {
            return Ok(InputOutcome::None);
        }
        let available = MAX_COMMAND_PROMPT_BYTES.saturating_sub(state.input.len());
        let mut end = text.len().min(available);
        while !text.is_char_boundary(end) {
            end = end.saturating_sub(1);
        }
        let index = scalar_byte_index(&state.input, state.cursor);
        state.input.insert_str(index, &text[..end]);
        state.cursor = state
            .cursor
            .saturating_add(u32::try_from(text[..end].chars().count()).unwrap_or(u32::MAX));
        client
            .send_input(InputMessage::CommandPrompt {
                action: CommandPromptAction::Update {
                    input: state.input.clone(),
                    cursor: state.cursor,
                },
            })
            .map_err(|error| error.to_string())?;
        return Ok(InputOutcome::Repaint);
    }
    let Some(pane) = model.active_pane() else {
        return Ok(InputOutcome::None);
    };
    client
        .send_input(InputMessage::TerminalView {
            pane,
            action: TerminalViewAction::Paste(text),
        })
        .map_err(|error| error.to_string())?;
    Ok(InputOutcome::None)
}

fn handle_mouse(
    model: &mut Model,
    client: &InteractiveClient,
    browser: &mut BrowserState,
    event: MouseEvent,
    pixel_mouse: bool,
) -> Result<InputOutcome, String> {
    if model.command_output_focus().is_some() {
        return Ok(InputOutcome::None);
    }
    let (global_column, global_row, global_x, global_y) =
        global_mouse_position(model, event, pixel_mouse);
    pointer_focus_follows_mouse(model, client, event, global_column, global_row)?;
    match mouse_route_owner(
        model.command_output_focus().is_some(),
        model.mouse_option,
        model.sidebar_edit.is_some() || model.sidebar_visible() && global_column <= sidebar::WIDTH,
    ) {
        MouseRouteOwner::CommandOutput => unreachable!("command output returned before routing"),
        MouseRouteOwner::Application => {
            if let Some((pane, action)) = app_mouse_forward_action(
                model,
                event,
                global_column,
                global_row,
                global_x,
                global_y,
            ) {
                client
                    .send_input(InputMessage::TerminalView { pane, action })
                    .map_err(|error| error.to_string())?;
            }
            return Ok(InputOutcome::None);
        }
        MouseRouteOwner::Sidebar => {
            if model.sidebar_edit.is_some() || global_column == sidebar::WIDTH {
                return Ok(InputOutcome::None);
            }
            match event.kind {
                MouseEventKind::ScrollUp => model.scroll_sidebar(-3),
                MouseEventKind::ScrollDown => model.scroll_sidebar(3),
                MouseEventKind::Down(MouseButton::Left)
                    if global_row < model.sidebar_tree_height() =>
                {
                    if let Some(target) = model.select_sidebar_row(global_row) {
                        return activate_sidebar_target(model, client, target);
                    }
                }
                _ => return Ok(InputOutcome::None),
            }
            return Ok(InputOutcome::Repaint);
        }
        MouseRouteOwner::Workspace => {}
    }
    match bound_mouse_key(model, event, global_column, global_row, global_x, global_y) {
        MouseKeyRoute::Send(bound) => {
            client
                .send_input(bound)
                .map_err(|error| error.to_string())?;
            return Ok(InputOutcome::None);
        }
        MouseKeyRoute::Native => {}
    }
    let sidebar_focus_changed =
        model.sidebar_focused() && matches!(event.kind, MouseEventKind::Down(MouseButton::Left));
    if sidebar_focus_changed {
        model.focus_active_pane();
    }
    if let Some(index) = model.status_row_at(global_row) {
        let (status_x, _) = model.status_area();
        if matches!(event.kind, MouseEventKind::Down(MouseButton::Left))
            && global_column >= status_x
            && let Some(zz_protocol::TmuxRange::Window(window)) =
                model.status_hit_target(index, global_column - status_x)
        {
            execute_target(client, "select-window", format!(":{window}"))?;
        }
        return Ok(if sidebar_focus_changed {
            InputOutcome::Repaint
        } else {
            InputOutcome::None
        });
    }
    let Some(entry) = model.pane_at(global_column, global_row) else {
        return Ok(if sidebar_focus_changed {
            InputOutcome::Repaint
        } else {
            InputOutcome::None
        });
    };
    let content = entry.content();
    if !content.contains(global_column, global_row) {
        if matches!(event.kind, MouseEventKind::Down(_)) {
            focus_pane(model, client, entry.pane)?;
        }
        return Ok(if sidebar_focus_changed {
            InputOutcome::Repaint
        } else {
            InputOutcome::None
        });
    }
    if browser.has_surface(entry.pane) {
        if matches!(event.kind, MouseEventKind::Down(_)) {
            focus_pane(model, client, entry.pane)?;
        }
        let input = browser_pointer_input(
            event,
            content,
            global_x,
            global_y,
            model.size.cell_width_px,
            model.size.cell_height_px,
        );
        browser.pointer(entry.pane, input);
        return Ok(if sidebar_focus_changed {
            InputOutcome::Repaint
        } else {
            InputOutcome::None
        });
    }
    let Some(viewport) = model.viewports.get(&entry.pane) else {
        if matches!(event.kind, MouseEventKind::Down(_)) {
            focus_pane(model, client, entry.pane)?;
        }
        return Ok(if sidebar_focus_changed {
            InputOutcome::Repaint
        } else {
            InputOutcome::None
        });
    };
    let mouse_tracking = viewport.mouse_tracking;
    if matches!(event.kind, MouseEventKind::Moved) && !mouse_tracking {
        return Ok(InputOutcome::None);
    }
    if matches!(event.kind, MouseEventKind::Down(_)) {
        focus_pane(model, client, entry.pane)?;
    }
    if let Some(action) = native_pane_mouse_action(
        model,
        entry.pane,
        content,
        event,
        (global_column, global_row, global_x, global_y),
    ) {
        client
            .send_input(InputMessage::TerminalView {
                pane: entry.pane,
                action,
            })
            .map_err(|error| error.to_string())?;
    }
    Ok(if sidebar_focus_changed {
        InputOutcome::Repaint
    } else {
        InputOutcome::None
    })
}

enum MouseKeyRoute {
    Send(InputMessage),
    Native,
}

fn bound_mouse_key(
    model: &mut Model,
    event: MouseEvent,
    global_column: u16,
    global_row: u16,
    global_x: u32,
    global_y: u32,
) -> MouseKeyRoute {
    let latch = latched_mouse_location(model, event, global_column, global_row, global_x, global_y);
    let Some(key) = mouse_key_name(event, &latch.location, latch.dragging) else {
        return MouseKeyRoute::Native;
    };
    let key = advance_click_sequence(
        model,
        event,
        &latch,
        key,
        (global_column, global_row, global_x, global_y),
    );
    let outside_modal =
        latch.location == "Empty" && model.window().is_some_and(|window| window.modal.is_some());
    let gesture_continues = matches!(event.kind, MouseEventKind::Drag(_) | MouseEventKind::Up(_));
    let bound_gesture = gesture_continues && latch.bound;
    if !outside_modal
        && !bound_gesture
        && !model.mouse_bindings.contains(&key)
        && !copy_mouse_key_is_reachable(model, &latch, &key)
        && !pane_mode_mouse_key_is_reachable(model, &latch, &key)
        && !is_click_sequence_name(&key)
    {
        return MouseKeyRoute::Native;
    }
    if matches!(event.kind, MouseEventKind::Drag(_))
        && let Some(latch) = model.mouse_drag.as_mut()
    {
        latch.bound = true;
    }
    MouseKeyRoute::Send(InputMessage::MouseKey {
        key,
        pane: latch.pane,
        window: latch.window,
        column: global_column,
        row: global_row,
        border: latch.border,
        status_range_start: mouse_status_range_start(model, global_column, global_row),
        press: gesture_continues.then_some((latch.press.0, latch.press.1)),
        view_action: latch.pane.and_then(|pane| {
            bound_mouse_view_action(
                model,
                pane,
                event,
                global_column,
                global_row,
                global_x,
                global_y,
            )
        }),
        press_action: latch.pane.and_then(|pane| {
            bound_mouse_view_action(
                model,
                pane,
                MouseEvent {
                    kind: MouseEventKind::Down(latch.button),
                    ..event
                },
                latch.press.0,
                latch.press.1,
                latch.press.2,
                latch.press.3,
            )
        }),
    })
}

fn is_click_sequence_name(key: &str) -> bool {
    let base = key.rsplit_once('-').map_or(key, |(_, base)| base);
    base.starts_with("SecondClick")
        || base.starts_with("DoubleClick")
        || base.starts_with("TripleClick")
}

const CLICK_TIMEOUT: Duration = Duration::from_millis(300);

fn advance_click_sequence(
    model: &mut Model,
    event: MouseEvent,
    latch: &crate::state::MouseDragLatch,
    key: String,
    cell: (u16, u16, u32, u32),
) -> String {
    let MouseEventKind::Down(button) = event.kind else {
        return key;
    };
    let now = Instant::now();
    let index = mouse_button_index(button);
    let carried = model.click.as_ref().filter(|sequence| {
        sequence.deadline > now
            && sequence.button == button
            && sequence.event.modifiers == event.modifiers
            && sequence.location == latch.location
            && sequence.pane == latch.pane
    });
    let (key, arm) = match carried {
        Some(sequence) if sequence.triple => {
            (format!("TripleClick{index}{}", latch.location), None)
        }
        Some(_) => (format!("SecondClick{index}{}", latch.location), Some(true)),
        None => (key, Some(false)),
    };
    model.click = arm.map(|triple| crate::state::ClickSequence {
        deadline: now + CLICK_TIMEOUT,
        triple,
        button,
        location: latch.location.clone(),
        pane: latch.pane,
        window: latch.window,
        event,
        cell,
    });
    key
}

pub(crate) fn expire_click_sequence(
    model: &mut Model,
    client: &InteractiveClient,
) -> Result<(), String> {
    let Some(sequence) = model.click.take() else {
        return Ok(());
    };
    if !sequence.triple {
        return Ok(());
    }
    let key = format!(
        "DoubleClick{}{}",
        mouse_button_index(sequence.button),
        sequence.location
    );
    let latch = crate::state::MouseDragLatch {
        button: sequence.button,
        location: sequence.location.clone(),
        pane: sequence.pane,
        window: sequence.window,
        border: None,
        dragging: false,
        bound: false,
        press: sequence.cell,
    };
    if !model.mouse_bindings.contains(&key)
        && !copy_mouse_key_is_reachable(model, &latch, &key)
        && !pane_mode_mouse_key_is_reachable(model, &latch, &key)
    {
        return Ok(());
    }
    let (column, row, x, y) = sequence.cell;
    let action = sequence
        .pane
        .and_then(|pane| bound_mouse_view_action(model, pane, sequence.event, column, row, x, y));
    client
        .send_input(InputMessage::MouseKey {
            key,
            pane: sequence.pane,
            window: sequence.window,
            column,
            row,
            border: None,
            status_range_start: mouse_status_range_start(model, column, row),
            view_action: action.clone(),
            press_action: action,
            press: None,
        })
        .map_err(|error| error.to_string())
}

fn bound_mouse_view_action(
    model: &Model,
    pane: zz_protocol::PaneId,
    event: MouseEvent,
    global_column: u16,
    global_row: u16,
    global_x: u32,
    global_y: u32,
) -> Option<TerminalViewAction> {
    let content = model
        .layout
        .panes
        .iter()
        .find(|entry| entry.pane == pane)?
        .content();
    native_pane_mouse_action(
        model,
        pane,
        content,
        event,
        (global_column, global_row, global_x, global_y),
    )
}

fn native_pane_mouse_action(
    model: &Model,
    pane: zz_protocol::PaneId,
    content: Rect,
    event: MouseEvent,
    (global_column, global_row, global_x, global_y): (u16, u16, u32, u32),
) -> Option<TerminalViewAction> {
    let viewport = model.viewports.get(&pane)?;
    let force_selection = event.modifiers.contains(KeyModifiers::SHIFT) || !viewport.mouse_tracking;
    let (content, global_column) = match crate::mode_view::presentation(model, pane, viewport)
        .and_then(|mode| crate::mode_view::line_number_gutter(mode, viewport))
    {
        Some(gutter) => {
            let body = gutter.body(content);
            let last = body.x.saturating_add(body.width.saturating_sub(1));
            (body, global_column.clamp(body.x, last.max(body.x)))
        }
        None => (content, global_column),
    };
    let source = model
        .layout
        .panes
        .iter()
        .find(|entry| entry.pane == pane)
        .map_or((0, 0), |entry| entry.source);
    pane_mouse_action(
        &model.size,
        event,
        (content, source),
        global_column,
        global_row,
        global_x,
        global_y,
        force_selection,
    )
}

const PANE_MODE_POINTER_KEYS: [&str; 5] = [
    "MouseDown1Pane",
    "MouseDown3Pane",
    "DoubleClick1Pane",
    "WheelUpPane",
    "WheelDownPane",
];

fn pane_mode_mouse_key_is_reachable(
    model: &Model,
    latch: &crate::state::MouseDragLatch,
    key: &str,
) -> bool {
    PANE_MODE_POINTER_KEYS.contains(&key)
        && latch.pane.is_some_and(|pane| {
            model
                .pane_snapshot(pane)
                .is_some_and(|snapshot| snapshot.mode.is_some())
        })
}

fn copy_mouse_key_is_reachable(
    model: &Model,
    latch: &crate::state::MouseDragLatch,
    key: &str,
) -> bool {
    let pane_holds_a_mode = latch.pane.is_some_and(|pane| {
        model
            .viewports
            .get(&pane)
            .is_some_and(|viewport| !matches!(viewport.mode, TerminalMode::Live))
    });
    (pane_holds_a_mode || latch.dragging && latch.bound) && model.copy_mouse_bindings.contains(key)
}

fn latched_mouse_location(
    model: &mut Model,
    event: MouseEvent,
    global_column: u16,
    global_row: u16,
    global_x: u32,
    global_y: u32,
) -> crate::state::MouseDragLatch {
    let press = (global_column, global_row, global_x, global_y);
    match event.kind {
        MouseEventKind::Down(button) => {
            let (location, pane, window) = mouse_key_location(model, global_column, global_row);
            let latch = crate::state::MouseDragLatch {
                button,
                location,
                pane,
                window,
                border: divider_axis(model, global_column, global_row),
                dragging: false,
                bound: false,
                press,
            };
            model.mouse_drag = Some(latch.clone());
            latch
        }
        MouseEventKind::Drag(button) => match model.mouse_drag.as_mut() {
            Some(latch) if latch.button == button => {
                latch.dragging = true;
                latch.clone()
            }
            _ => resolved_mouse_latch(model, global_column, global_row, press),
        },
        MouseEventKind::Up(button) => {
            let latch = match model.mouse_drag.as_ref() {
                Some(latch) if latch.button == button => latch.clone(),
                _ => resolved_mouse_latch(model, global_column, global_row, press),
            };
            model.mouse_drag = None;
            latch
        }
        _ => resolved_mouse_latch(model, global_column, global_row, press),
    }
}

fn resolved_mouse_latch(
    model: &Model,
    global_column: u16,
    global_row: u16,
    press: (u16, u16, u32, u32),
) -> crate::state::MouseDragLatch {
    let (location, pane, window) = mouse_key_location(model, global_column, global_row);
    crate::state::MouseDragLatch {
        button: MouseButton::Left,
        location,
        pane,
        window,
        border: divider_axis(model, global_column, global_row),
        dragging: false,
        bound: false,
        press,
    }
}

fn divider_axis(model: &Model, global_column: u16, global_row: u16) -> Option<zz_protocol::Axis> {
    model
        .layout
        .dividers
        .iter()
        .find(|divider| divider.rect.contains(global_column, global_row))
        .map(|divider| divider.axis)
}

fn mouse_status_range_start(model: &Model, global_column: u16, global_row: u16) -> Option<u16> {
    let index = model.status_row_at(global_row)?;
    let (status_x, _) = model.status_area();
    let column = global_column.checked_sub(status_x)?;
    model
        .status_hit_range_start(index, column)
        .map(|start| status_x.saturating_add(start))
}

fn mouse_key_location(
    model: &Model,
    global_column: u16,
    global_row: u16,
) -> (
    String,
    Option<zz_protocol::PaneId>,
    Option<zz_protocol::WindowId>,
) {
    if let Some(index) = model.status_row_at(global_row) {
        let (status_x, _) = model.status_area();
        let target = global_column
            .checked_sub(status_x)
            .and_then(|column| model.status_hit_target(index, column));
        let location = match &target {
            None | Some(zz_protocol::TmuxRange::None) => "StatusDefault".to_owned(),
            Some(zz_protocol::TmuxRange::Left) => "StatusLeft".to_owned(),
            Some(zz_protocol::TmuxRange::Right) => "StatusRight".to_owned(),
            Some(zz_protocol::TmuxRange::Control(control)) => format!("Control{control}"),
            Some(
                zz_protocol::TmuxRange::Pane(_)
                | zz_protocol::TmuxRange::Window(_)
                | zz_protocol::TmuxRange::Session(_)
                | zz_protocol::TmuxRange::User(_)
                | zz_protocol::TmuxRange::Other { .. },
            ) => "Status".to_owned(),
        };
        let window = match target {
            Some(zz_protocol::TmuxRange::Window(window)) => model.window_id_at_index(window),
            _ => None,
        };
        return (location, None, window);
    }
    let window = model.window().map(|window| window.id);
    if let Some(entry) = model.pane_at(global_column, global_row) {
        let location = if entry.content().contains(global_column, global_row) {
            "Pane"
        } else {
            "Border"
        };
        return (location.to_owned(), Some(entry.pane), window);
    }
    let Some(entry) = divider_owner(model, global_column, global_row) else {
        return ("Empty".to_owned(), None, window);
    };
    ("Border".to_owned(), Some(entry), window)
}

fn divider_owner(
    model: &Model,
    global_column: u16,
    global_row: u16,
) -> Option<zz_protocol::PaneId> {
    let divider = model
        .layout
        .dividers
        .iter()
        .find(|divider| divider.rect.contains(global_column, global_row))?;
    let (column, row) = match divider.axis {
        zz_protocol::Axis::Horizontal => (global_column.checked_sub(1)?, global_row),
        zz_protocol::Axis::Vertical => (global_column, global_row.checked_sub(1)?),
    };
    model.pane_at(column, row).map(|entry| entry.pane)
}

fn mouse_key_name(event: MouseEvent, location: &str, dragging: bool) -> Option<String> {
    let base = match event.kind {
        MouseEventKind::Down(button) => {
            format!("MouseDown{}{location}", mouse_button_index(button))
        }
        MouseEventKind::Up(button) if dragging => {
            format!("MouseDragEnd{}{location}", mouse_button_index(button))
        }
        MouseEventKind::Up(button) => format!("MouseUp{}{location}", mouse_button_index(button)),
        MouseEventKind::Drag(button) => {
            format!("MouseDrag{}{location}", mouse_button_index(button))
        }
        MouseEventKind::ScrollUp => format!("WheelUp{location}"),
        MouseEventKind::ScrollDown => format!("WheelDown{location}"),
        MouseEventKind::Moved | MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight => {
            return None;
        }
    };
    let mut key = String::new();
    if event.modifiers.contains(KeyModifiers::CONTROL) {
        key.push_str("C-");
    }
    if event.modifiers.contains(KeyModifiers::ALT) {
        key.push_str("M-");
    }
    if event.modifiers.contains(KeyModifiers::SHIFT) {
        key.push_str("S-");
    }
    key.push_str(&base);
    Some(key)
}

const fn mouse_button_index(button: MouseButton) -> u8 {
    match button {
        MouseButton::Left => 1,
        MouseButton::Middle => 2,
        MouseButton::Right => 3,
    }
}

fn pointer_focus_follows_mouse(
    model: &mut Model,
    client: &InteractiveClient,
    event: MouseEvent,
    global_column: u16,
    global_row: u16,
) -> Result<(), String> {
    if !model.focus_follows_mouse || !matches!(event.kind, MouseEventKind::Moved) {
        return Ok(());
    }
    let Some(entry) = model.pane_at(global_column, global_row) else {
        return Ok(());
    };
    if !entry.content().contains(global_column, global_row)
        || model.active_pane() == Some(entry.pane)
    {
        return Ok(());
    }
    focus_pane(model, client, entry.pane)
}

fn global_mouse_position(
    model: &Model,
    event: MouseEvent,
    pixel_mouse: bool,
) -> (u16, u16, u32, u32) {
    if pixel_mouse {
        (
            u16::try_from(u32::from(event.column) / model.size.cell_width_px.max(1))
                .unwrap_or(u16::MAX),
            u16::try_from(u32::from(event.row) / model.size.cell_height_px.max(1))
                .unwrap_or(u16::MAX),
            u32::from(event.column),
            u32::from(event.row),
        )
    } else {
        (
            event.column,
            event.row,
            u32::from(event.column).saturating_mul(model.size.cell_width_px),
            u32::from(event.row).saturating_mul(model.size.cell_height_px),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MouseRouteOwner {
    CommandOutput,
    Application,
    Sidebar,
    Workspace,
}

const fn mouse_route_owner(
    command_output: bool,
    mouse_option: bool,
    sidebar_hit: bool,
) -> MouseRouteOwner {
    if command_output {
        MouseRouteOwner::CommandOutput
    } else if !mouse_option {
        MouseRouteOwner::Application
    } else if sidebar_hit {
        MouseRouteOwner::Sidebar
    } else {
        MouseRouteOwner::Workspace
    }
}

pub(crate) fn app_mouse_forward_action(
    model: &Model,
    event: MouseEvent,
    global_column: u16,
    global_row: u16,
    global_x: u32,
    global_y: u32,
) -> Option<(zz_protocol::PaneId, TerminalViewAction)> {
    let entry = model.pane_at(global_column, global_row)?;
    let content = entry.content();
    if !content.contains(global_column, global_row) {
        return None;
    }
    let viewport = model.viewports.get(&entry.pane)?;
    if !viewport.mouse_tracking {
        return None;
    }
    let action = pane_mouse_action(
        &model.size,
        event,
        (content, entry.source),
        global_column,
        global_row,
        global_x,
        global_y,
        event.modifiers.contains(KeyModifiers::SHIFT),
    )?;
    Some((entry.pane, action))
}

#[expect(clippy::too_many_arguments)]
fn pane_mouse_action(
    size: &crate::tty::TerminalSize,
    event: MouseEvent,
    (content, source): (Rect, (u16, u16)),
    global_column: u16,
    global_row: u16,
    global_x: u32,
    global_y: u32,
    force_selection: bool,
) -> Option<TerminalViewAction> {
    let (left, top) = source;
    let column = global_column.saturating_sub(content.x).saturating_add(left);
    let row = global_row.saturating_sub(content.y).saturating_add(top);
    let x = global_x
        .saturating_sub(u32::from(content.x).saturating_mul(size.cell_width_px))
        .saturating_add(u32::from(left).saturating_mul(size.cell_width_px));
    let y = global_y
        .saturating_sub(u32::from(content.y).saturating_mul(size.cell_height_px))
        .saturating_add(u32::from(top).saturating_mul(size.cell_height_px));
    let content = Rect {
        width: content.width.saturating_add(left),
        height: content.height.saturating_add(top),
        ..content
    };
    let (phase, button) = mouse_routing(event.kind);
    let input = TerminalMouseInput::new(
        phase,
        button,
        PointerCellEvent {
            column,
            row,
            click_count: u8::from(matches!(event.kind, MouseEventKind::Down(_))),
            rectangle: false,
        },
        x,
        y,
        u32::from(content.width).saturating_mul(size.cell_width_px),
        u32::from(content.height).saturating_mul(size.cell_height_px),
        size.cell_width_px,
        size.cell_height_px,
        modifiers(event.modifiers),
        force_selection,
    );
    let scroll_lines = if force_selection { 3 } else { 1 };
    match event.kind {
        MouseEventKind::ScrollUp => Some(TerminalViewAction::ScrollWheel {
            lines: -scroll_lines,
            input,
        }),
        MouseEventKind::ScrollDown => Some(TerminalViewAction::ScrollWheel {
            lines: scroll_lines,
            input,
        }),
        MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight => None,
        _ => Some(TerminalViewAction::Mouse(input)),
    }
}

fn focus_pane(
    model: &mut Model,
    client: &InteractiveClient,
    pane: zz_protocol::PaneId,
) -> Result<(), String> {
    client
        .execute(CommandInvocation::new(
            "select-pane",
            ["-t".to_owned(), pane.to_string()],
        ))
        .map(drop)
        .map_err(|error| error.to_string())?;
    model.activate_pane(pane);
    Ok(())
}

fn send_focus(model: &mut Model, client: &InteractiveClient, focused: bool) -> Result<(), String> {
    if let Some(input) = model.client_focus_changed(focused) {
        client
            .send_input(input)
            .map_err(|error| error.to_string())?;
    }
    let active_pane = model.active_pane().and_then(|pane| {
        model
            .pane_snapshot(pane)
            .map(|snapshot| (pane, &snapshot.kind))
    });
    if let Some(input) = pane_focus_input(active_pane, focused) {
        client
            .send_input(input)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn pane_focus_input(
    active_pane: Option<(zz_protocol::PaneId, &PaneKindSnapshot)>,
    focused: bool,
) -> Option<InputMessage> {
    if let Some((pane, PaneKindSnapshot::Terminal)) = active_pane {
        Some(InputMessage::TerminalView {
            pane,
            action: TerminalViewAction::Focus(focused),
        })
    } else {
        None
    }
}

fn browser_pointer_input(
    event: MouseEvent,
    content: Rect,
    global_x: u32,
    global_y: u32,
    cell_width_px: u32,
    cell_height_px: u32,
) -> ProviderPointerInput {
    const WHEEL_STEP: i32 = 120;

    let (phase, button, wheel_delta_x, wheel_delta_y) = match event.kind {
        MouseEventKind::Down(button) => (
            ProviderPointerPhase::Down,
            Some(provider_button(button)),
            0,
            0,
        ),
        MouseEventKind::Up(button) => (
            ProviderPointerPhase::Up,
            Some(provider_button(button)),
            0,
            0,
        ),
        MouseEventKind::Drag(button) => (
            ProviderPointerPhase::Move,
            Some(provider_button(button)),
            0,
            0,
        ),
        MouseEventKind::Moved => (ProviderPointerPhase::Move, None, 0, 0),
        MouseEventKind::ScrollUp => (ProviderPointerPhase::Wheel, None, 0, WHEEL_STEP),
        MouseEventKind::ScrollDown => (ProviderPointerPhase::Wheel, None, 0, -WHEEL_STEP),
        MouseEventKind::ScrollLeft => (ProviderPointerPhase::Wheel, None, -WHEEL_STEP, 0),
        MouseEventKind::ScrollRight => (ProviderPointerPhase::Wheel, None, WHEEL_STEP, 0),
    };
    let x = global_x.saturating_sub(u32::from(content.x).saturating_mul(cell_width_px));
    let y = global_y.saturating_sub(u32::from(content.y).saturating_mul(cell_height_px));
    ProviderPointerInput {
        x: i32::try_from(x).unwrap_or(i32::MAX),
        y: i32::try_from(y).unwrap_or(i32::MAX),
        phase,
        button,
        click_count: i32::from(matches!(
            event.kind,
            MouseEventKind::Down(_) | MouseEventKind::Up(_)
        )),
        wheel_delta_x,
        wheel_delta_y,
        modifiers: ProviderModifiers::new(
            event.modifiers.contains(KeyModifiers::SHIFT),
            event.modifiers.contains(KeyModifiers::CONTROL),
            event.modifiers.contains(KeyModifiers::ALT),
            event.modifiers.contains(KeyModifiers::SUPER),
        ),
    }
}

const fn provider_button(button: MouseButton) -> ProviderPointerButton {
    match button {
        MouseButton::Left => ProviderPointerButton::Left,
        MouseButton::Middle => ProviderPointerButton::Middle,
        MouseButton::Right => ProviderPointerButton::Right,
    }
}

fn mouse_routing(kind: MouseEventKind) -> (TerminalMousePhase, Option<TerminalMouseButton>) {
    match kind {
        MouseEventKind::Down(button) => (TerminalMousePhase::Press, Some(mouse_button(button))),
        MouseEventKind::Up(button) => (TerminalMousePhase::Release, Some(mouse_button(button))),
        MouseEventKind::Drag(button) => (TerminalMousePhase::Motion, Some(mouse_button(button))),
        MouseEventKind::Moved => (TerminalMousePhase::Motion, None),
        MouseEventKind::ScrollUp => (
            TerminalMousePhase::Press,
            Some(TerminalMouseButton::ScrollUp),
        ),
        MouseEventKind::ScrollDown => (
            TerminalMousePhase::Press,
            Some(TerminalMouseButton::ScrollDown),
        ),
        MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight => {
            (TerminalMousePhase::Press, None)
        }
    }
}

const fn mouse_button(button: MouseButton) -> TerminalMouseButton {
    match button {
        MouseButton::Left => TerminalMouseButton::Left,
        MouseButton::Middle => TerminalMouseButton::Middle,
        MouseButton::Right => TerminalMouseButton::Right,
    }
}

pub(crate) fn key_input(event: KeyEvent) -> KeyInput {
    let mut input = key_chord(event);
    input.text = match event.code {
        TerminalKeyCode::Char(character) if !character.is_control() => {
            Some(character.to_string().into_boxed_str())
        }
        _ => None,
    };
    input
}

pub(crate) fn key_chord(event: KeyEvent) -> KeyInput {
    let key = key_code(event.code);
    let event_modifiers = if matches!(event.code, TerminalKeyCode::BackTab) {
        event.modifiers | KeyModifiers::SHIFT
    } else {
        event.modifiers
    };
    let unshifted_codepoint = match key {
        KeyCode::Character(character) => Some(character),
        _ => None,
    };
    KeyInput {
        action: match event.kind {
            KeyEventKind::Press => KeyAction::Press,
            KeyEventKind::Repeat => KeyAction::Repeat,
            KeyEventKind::Release => KeyAction::Release,
        },
        key,
        modifiers: modifiers(event_modifiers),
        text: None,
        unshifted_codepoint,
    }
}

fn key_code(code: TerminalKeyCode) -> KeyCode {
    match code {
        TerminalKeyCode::Char(character) => KeyCode::Character(if character.is_ascii_uppercase() {
            character.to_ascii_lowercase()
        } else {
            character
        }),
        TerminalKeyCode::Backspace => KeyCode::Backspace,
        TerminalKeyCode::Enter => KeyCode::Enter,
        TerminalKeyCode::Tab | TerminalKeyCode::BackTab => KeyCode::Tab,
        TerminalKeyCode::Esc => KeyCode::Escape,
        TerminalKeyCode::Delete => KeyCode::Delete,
        TerminalKeyCode::Insert => KeyCode::Insert,
        TerminalKeyCode::Home => KeyCode::Home,
        TerminalKeyCode::End => KeyCode::End,
        TerminalKeyCode::PageUp => KeyCode::PageUp,
        TerminalKeyCode::PageDown => KeyCode::PageDown,
        TerminalKeyCode::Up => KeyCode::ArrowUp,
        TerminalKeyCode::Down => KeyCode::ArrowDown,
        TerminalKeyCode::Left => KeyCode::ArrowLeft,
        TerminalKeyCode::Right => KeyCode::ArrowRight,
        TerminalKeyCode::F(number) => KeyCode::Function(number),
        TerminalKeyCode::Unidentified => KeyCode::Unidentified,
        TerminalKeyCode::User(number) => KeyCode::User(number),
    }
}

const fn modifiers(value: KeyModifiers) -> Modifiers {
    Modifiers::new(
        value.contains(KeyModifiers::SHIFT),
        value.contains(KeyModifiers::CONTROL),
        value.contains(KeyModifiers::ALT),
        value.contains(KeyModifiers::SUPER),
    )
}

#[cfg(test)]
mod command_output_tests;

#[cfg(test)]
mod confirm_tests;

#[cfg(test)]
mod key_parse_tests;

#[cfg(test)]
mod menu_tests;

#[cfg(test)]
mod pointer_tests;

#[cfg(test)]
mod routing_tests;

#[cfg(test)]
mod test_support;
