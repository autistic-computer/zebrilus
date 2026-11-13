use ala::{App, AppContext as _, Entity, ScrollHandle};

use crate::{
    IconName,
    touch::{
        ArrowPad, KeyRowKey, StackChip, StackGroup, StackKey, StackKeyTap, StackMenu,
        StickyModifier,
    },
};

#[derive(Clone)]
pub struct ToolKeys {
    pub arrows: Entity<ArrowPad>,
    pub control: Entity<StackKey>,
    pub alt: Entity<StackKey>,
    pub prefix: Entity<StackKey>,
    pub scroll: ScrollHandle,
}

impl ToolKeys {
    pub fn new(cx: &mut App) -> Self {
        let scroll = ScrollHandle::new();
        let latch = |label: &'static str,
                     modifier: StickyModifier,
                     chords: Vec<StackChip>,
                     cx: &mut App| {
            cx.new(|cx| {
                StackKey::new(label, StackKeyTap::Latch(modifier), cx)
                    .icon(modifier.icon())
                    .scrolls(scroll.clone())
                    .groups(vec![StackGroup::new(chords)], cx)
            })
        };
        Self {
            arrows: cx.new(ArrowPad::new),
            control: latch("ctrl", StickyModifier::Control, control_chords(), cx),
            alt: latch("alt", StickyModifier::Alt, alt_chords(), cx),
            prefix: cx.new(|cx| StackKey::new("prefix", StackKeyTap::Open, cx)),
            scroll,
        }
    }

    pub fn lead(&self) -> KeyRowKey {
        KeyRowKey::Stack(self.prefix.clone())
    }

    pub fn row(&self) -> Vec<KeyRowKey> {
        vec![
            KeyRowKey::Stack(self.control.clone()),
            KeyRowKey::Stack(self.alt.clone()),
            KeyRowKey::tab(),
            KeyRowKey::named("esc", "escape").icon(IconName::KeyEscape),
            KeyRowKey::text("/"),
            KeyRowKey::text("@"),
            KeyRowKey::Arrows(self.arrows.clone()),
            KeyRowKey::Hide,
        ]
    }

    pub fn menus(&self, cx: &App) -> [Entity<StackMenu>; 3] {
        [&self.control, &self.alt, &self.prefix].map(|key| key.read(cx).menu().clone())
    }

    pub fn is_open(&self, cx: &App) -> bool {
        [&self.control, &self.alt, &self.prefix]
            .iter()
            .any(|key| key.read(cx).is_open(cx))
    }

    pub fn close(&self, cx: &mut App) {
        for key in [&self.control, &self.alt, &self.prefix] {
            if key.read(cx).menu().read(cx).is_shown() {
                key.update(cx, StackKey::close);
            }
        }
    }
}

fn chords(chords: &[(&'static str, &'static str, &'static str)]) -> Vec<StackChip> {
    chords
        .iter()
        .map(|(keystroke, chord, label)| StackChip::chord(*keystroke, *chord, *label))
        .collect()
}

fn control_chords() -> Vec<StackChip> {
    chords(&[
        ("ctrl-c", "^C", "Interrupt"),
        ("ctrl-d", "^D", "End input"),
        ("ctrl-z", "^Z", "Suspend"),
        ("ctrl-l", "^L", "Clear"),
        ("ctrl-r", "^R", "History"),
        ("ctrl-u", "^U", "Kill line"),
        ("ctrl-w", "^W", "Kill word"),
        ("ctrl-k", "^K", "Kill to end"),
        ("ctrl-a", "^A", "Line start"),
        ("ctrl-e", "^E", "Line end"),
    ])
}

fn alt_chords() -> Vec<StackChip> {
    chords(&[
        ("alt-b", "M-b", "Word back"),
        ("alt-f", "M-f", "Word ahead"),
        ("alt-d", "M-d", "Kill word"),
        ("alt-.", "M-.", "Last arg"),
    ])
}
