use zz_gpui::Keystroke;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PaletteVimMode {
    #[default]
    Insert,
    Normal,
}

impl PaletteVimMode {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Insert => "INSERT",
            Self::Normal => "NORMAL",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Twin {
    Down,
    Up,
    Into,
    Out,
    Drop,
    CopyAddress,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VimCommand {
    Pass,
    Swallow,
    Twin(Twin),
    First,
    Last,
    Insert { after: bool },
    ClearAndInsert,
    InsertWith(char),
    Normal,
    Close,
}

#[derive(Debug, Default)]
pub(crate) struct PaletteVim {
    mode: PaletteVimMode,
    pending: Option<char>,
}

impl PaletteVim {
    pub const fn mode(&self) -> PaletteVimMode {
        self.mode
    }

    pub const fn pending(&self) -> Option<char> {
        self.pending
    }

    pub fn escape(&mut self) -> VimCommand {
        self.pending = None;
        match self.mode {
            PaletteVimMode::Insert => {
                self.mode = PaletteVimMode::Normal;
                VimCommand::Normal
            }
            PaletteVimMode::Normal => VimCommand::Close,
        }
    }

    pub fn insert(&mut self) {
        self.mode = PaletteVimMode::Insert;
        self.pending = None;
    }

    pub fn key(&mut self, keystroke: &Keystroke, is_sigil: impl Fn(char) -> bool) -> VimCommand {
        let modifiers = keystroke.modifiers;
        if keystroke.key == "escape" && !modifiers.modified()
            || keystroke.key == "[" && modifiers.control && !modifiers.platform && !modifiers.alt
        {
            return self.escape();
        }
        if self.mode == PaletteVimMode::Insert {
            return VimCommand::Pass;
        }
        if modifiers.control || modifiers.platform || modifiers.function {
            return VimCommand::Pass;
        }
        let Some(typed) = typed(keystroke) else {
            return VimCommand::Pass;
        };
        match (self.pending.take(), typed) {
            (Some('g'), 'g') => return VimCommand::First,
            (Some('y'), 'y') => return VimCommand::Twin(Twin::CopyAddress),
            _ => {}
        }
        match typed {
            'j' => VimCommand::Twin(Twin::Down),
            'k' => VimCommand::Twin(Twin::Up),
            'l' => VimCommand::Twin(Twin::Into),
            'h' => VimCommand::Twin(Twin::Out),
            'x' => VimCommand::Twin(Twin::Drop),
            'G' => VimCommand::Last,
            'g' | 'y' => {
                self.pending = Some(typed);
                VimCommand::Swallow
            }
            'i' | 'a' => {
                self.insert();
                VimCommand::Insert {
                    after: typed == 'a',
                }
            }
            '/' => {
                self.insert();
                VimCommand::ClearAndInsert
            }
            'q' => VimCommand::Close,
            sigil if is_sigil(sigil) => {
                self.insert();
                VimCommand::InsertWith(sigil)
            }
            _ => VimCommand::Swallow,
        }
    }
}

fn typed(keystroke: &Keystroke) -> Option<char> {
    let single = |text: &str| {
        let mut chars = text.chars();
        chars.next().filter(|_| chars.next().is_none())
    };
    keystroke
        .key_char
        .as_deref()
        .and_then(single)
        .filter(char::is_ascii)
        .or_else(|| {
            single(&keystroke.key)
                .filter(char::is_ascii_graphic)
                .map(|key| {
                    if keystroke.modifiers.shift {
                        key.to_ascii_uppercase()
                    } else {
                        key
                    }
                })
        })
}

#[cfg(test)]
mod tests {
    use zz_client::place::is_sigil;
    use zz_gpui::{Keystroke, Modifiers};

    use super::*;

    fn press(vim: &mut PaletteVim, keys: &str) -> Vec<VimCommand> {
        keys.split(' ')
            .map(|key| {
                let keystroke = Keystroke::parse(key).expect("a valid keystroke");
                vim.key(&keystroke, is_sigil)
            })
            .collect()
    }

    fn normal() -> PaletteVim {
        let mut vim = PaletteVim::default();
        vim.escape();
        vim
    }

    #[test]
    fn a_new_palette_is_in_insert_and_lets_keys_through() {
        let mut vim = PaletteVim::default();
        assert_eq!(vim.mode(), PaletteVimMode::Insert);
        assert_eq!(
            press(&mut vim, "j x q $ enter"),
            [VimCommand::Pass; 5],
            "insert mode leaves every key but escape to the field"
        );
    }

    #[test]
    fn escape_goes_to_normal_then_closes() {
        let mut vim = PaletteVim::default();
        assert_eq!(press(&mut vim, "escape"), [VimCommand::Normal]);
        assert_eq!(vim.mode(), PaletteVimMode::Normal);
        assert_eq!(press(&mut vim, "escape"), [VimCommand::Close]);

        let mut vim = PaletteVim::default();
        assert_eq!(
            press(&mut vim, "ctrl-[ ctrl-["),
            [VimCommand::Normal, VimCommand::Close]
        );
    }

    #[test]
    fn each_normal_key_maps_to_its_command() {
        for (key, command) in [
            ("j", VimCommand::Twin(Twin::Down)),
            ("k", VimCommand::Twin(Twin::Up)),
            ("l", VimCommand::Twin(Twin::Into)),
            ("h", VimCommand::Twin(Twin::Out)),
            ("x", VimCommand::Twin(Twin::Drop)),
            ("shift-g", VimCommand::Last),
            ("q", VimCommand::Close),
            ("i", VimCommand::Insert { after: false }),
            ("a", VimCommand::Insert { after: true }),
            ("/", VimCommand::ClearAndInsert),
            ("$", VimCommand::InsertWith('$')),
            ("?", VimCommand::InsertWith('?')),
            ("z", VimCommand::Swallow),
            ("5", VimCommand::Swallow),
            ("cmd-k", VimCommand::Pass),
            ("ctrl-a", VimCommand::Pass),
            ("tab", VimCommand::Pass),
        ] {
            let mut vim = normal();
            assert_eq!(press(&mut vim, key), [command], "{key}");
        }
    }

    #[test]
    fn insert_commands_leave_normal_mode() {
        for key in ["i", "a", "/", "@"] {
            let mut vim = normal();
            press(&mut vim, key);
            assert_eq!(vim.mode(), PaletteVimMode::Insert, "{key}");
        }
        let mut vim = normal();
        press(&mut vim, "j shift-g x q");
        assert_eq!(vim.mode(), PaletteVimMode::Normal);
    }

    #[test]
    fn gg_and_yy_wait_for_their_second_key() {
        let mut vim = normal();
        assert_eq!(press(&mut vim, "g"), [VimCommand::Swallow]);
        assert_eq!(vim.pending(), Some('g'));
        assert_eq!(press(&mut vim, "g"), [VimCommand::First]);
        assert_eq!(vim.pending(), None);
        assert_eq!(
            press(&mut vim, "y y"),
            [VimCommand::Swallow, VimCommand::Twin(Twin::CopyAddress)]
        );
    }

    #[test]
    fn another_key_cancels_a_pending_one_and_still_counts() {
        let mut vim = normal();
        assert_eq!(
            press(&mut vim, "g j"),
            [VimCommand::Swallow, VimCommand::Twin(Twin::Down)]
        );
        assert_eq!(
            press(&mut vim, "y g g"),
            [VimCommand::Swallow, VimCommand::Swallow, VimCommand::First]
        );
        press(&mut vim, "g");
        assert_eq!(press(&mut vim, "escape"), [VimCommand::Close]);
        assert_eq!(vim.pending(), None);
    }

    #[test]
    fn a_sigil_typed_with_option_still_starts_insert() {
        let mut vim = normal();
        let at = Keystroke {
            modifiers: Modifiers::alt(),
            key: "l".into(),
            key_char: Some("@".into()),
        };
        assert_eq!(vim.key(&at, is_sigil), VimCommand::InsertWith('@'));
    }

    #[test]
    fn letters_on_a_non_latin_layout_match_by_their_key() {
        let mut vim = normal();
        let j = Keystroke {
            modifiers: Modifiers::none(),
            key: "j".into(),
            key_char: Some("о".into()),
        };
        assert_eq!(vim.key(&j, is_sigil), VimCommand::Twin(Twin::Down));
    }
}
