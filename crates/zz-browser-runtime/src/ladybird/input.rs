use super::{BrowserCommandSink, messages::server};
use ala_cef::{
    BrowserKey, EditCommand, KeyAction, KeyInput, Modifiers, PointerButton, PointerEvent,
    PointerPhase, WheelEvent,
};

fn modifiers(value: Modifiers) -> u32 {
    u32::from(value.alt())
        | u32::from(value.control()) << 1
        | u32::from(value.shift()) << 2
        | u32::from(value.platform()) << 3
}

pub(super) fn utf16(text: &str) -> Vec<u8> {
    let mut body = vec![0];
    body.extend((text.encode_utf16().count() as u32).to_le_bytes());
    body.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
    body
}

impl BrowserCommandSink {
    pub(super) fn command(&self, message: u32, body: Vec<u8>) {
        self.hub.command(self.id, message, body);
    }
    pub fn navigate(&self, url: &str) {
        let mut body = (url.len() as u32).to_le_bytes().to_vec();
        body.extend(url.as_bytes());
        body.extend([0, 0]);
        self.command(server::VIEW_LOAD, body);
    }
    pub fn go_back(&self) {
        self.command(
            server::VIEW_TRAVERSE_THE_HISTORY_BY_DELTA,
            (-1i32).to_le_bytes().to_vec(),
        );
    }
    pub fn go_forward(&self) {
        self.command(
            server::VIEW_TRAVERSE_THE_HISTORY_BY_DELTA,
            1i32.to_le_bytes().to_vec(),
        );
    }
    pub fn reload(&self) {
        self.command(server::VIEW_RELOAD, vec![0]);
    }
    pub fn reload_ignore_cache(&self) {
        self.command(server::VIEW_RELOAD, vec![1]);
    }
    pub fn stop_loading(&self) {
        self.command(server::VIEW_STOP_LOADING, vec![]);
    }
    pub fn find(&self, text: &str, forward: bool, match_case: bool, next: bool) {
        if next {
            self.command(
                if forward {
                    server::VIEW_FIND_IN_PAGE_NEXT_MATCH
                } else {
                    server::VIEW_FIND_IN_PAGE_PREVIOUS_MATCH
                },
                vec![],
            );
        } else {
            let mut body = utf16(text);
            body.extend(u32::from(match_case).to_le_bytes());
            body.push(1);
            self.command(server::VIEW_FIND_IN_PAGE, body);
        }
    }
    pub fn stop_finding(&self, _: bool) {
        self.command(server::VIEW_FIND_IN_PAGE_END, vec![]);
    }
    pub fn set_audio_muted(&self, _: bool) {}
    pub fn replace_misspelling(&self, _: &str) {}
    pub fn add_word_to_dictionary(&self, _: &str) {}
    pub fn edit(&self, command: EditCommand) {
        let message = match command {
            EditCommand::Undo => server::VIEW_UNDO,
            EditCommand::Redo => server::VIEW_REDO,
            EditCommand::SelectAll => server::VIEW_SELECT_ALL,
            EditCommand::Cut
            | EditCommand::Copy
            | EditCommand::Paste
            | EditCommand::PasteAndMatchStyle => return,
        };
        self.command(message, vec![]);
    }

    pub fn send_key(&self, input: KeyInput) {
        let key = match input.key {
            BrowserKey::Insert => 0x7e,
            BrowserKey::Character(c) if !c.is_ascii_alphanumeric() => 0,
            key => key.windows_key_code() as u8,
        };
        let ch = match input.key {
            BrowserKey::Character(c) => u32::from(c),
            BrowserKey::Space => 32,
            _ => 0,
        };
        let mut body = 0u64.to_le_bytes().to_vec();
        body.extend([u8::from(input.action == KeyAction::Release), key]);
        body.extend(modifiers(input.modifiers).to_le_bytes());
        body.extend(ch.to_le_bytes());
        body.extend([u8::from(input.modifiers.is_repeat()), 0, 0, 0]);
        body.extend([0; 13]);
        self.command(server::VIEW_ENQUEUE_KEY_EVENT, body);
    }
    pub fn send_text(&self, text: &str) {
        let mut body = utf16(text);
        body.extend((-1i32).to_le_bytes());
        body.extend(0i32.to_le_bytes());
        self.command(server::VIEW_COMMIT_TEXT_FROM_INPUT_METHOD, body);
    }
    pub fn commit_composition(&self, text: &str) {
        self.send_text(text);
    }
    pub fn set_composition(&self, _: &str, _: std::ops::Range<usize>) {}
    pub fn finish_composition(&self) {}
    pub fn cancel_composition(&self) {}

    pub fn send_pointer(&self, event: PointerEvent) {
        let kind = match event.phase {
            PointerPhase::Down => 0,
            PointerPhase::Up => 1,
            PointerPhase::Move => 2,
            PointerPhase::Leave => 3,
        };
        let button = match event.button {
            Some(PointerButton::Left) => 1,
            Some(PointerButton::Right) => 2,
            Some(PointerButton::Middle) => 4,
            None => 0,
        };
        self.mouse(
            kind,
            event.x,
            event.y,
            button,
            event.modifiers,
            event.click_count,
            (0., 0.),
            false,
        );
    }
    pub fn send_wheel(&self, event: WheelEvent) {
        self.mouse(
            5,
            event.x,
            event.y,
            0,
            event.modifiers,
            0,
            (-f64::from(event.delta_x), -f64::from(event.delta_y)),
            event.precise,
        );
    }
    fn mouse(
        &self,
        kind: u8,
        x: i32,
        y: i32,
        button: u8,
        mods: Modifiers,
        clicks: i32,
        delta: (f64, f64),
        precise: bool,
    ) {
        let mut body = 0u64.to_le_bytes().to_vec();
        body.push(kind);
        for value in [x, y, x, y] {
            body.extend(((value as f32 * self.scale.get()).round() as i32).to_le_bytes());
        }
        let mut held = u8::from(mods.left_mouse())
            | u8::from(mods.right_mouse()) << 1
            | u8::from(mods.middle_mouse()) << 2;
        if kind == 0 {
            held |= button;
        } else if kind == 1 {
            held &= !button;
        }
        body.extend([button, held]);
        body.extend(modifiers(mods).to_le_bytes());
        for value in [delta.0, delta.1, delta.0, delta.1] {
            body.extend(value.to_le_bytes());
        }
        body.extend([u8::from(precise), 0]);
        body.extend(clicks.to_le_bytes());
        body.extend([0; 12]);
        self.command(server::VIEW_ENQUEUE_MOUSE_EVENT, body);
    }
}
