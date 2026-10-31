use ala::{App, AppContext as _, Entity, ScrollHandle};

use crate::{
    IconName,
    touch::{ArrowPad, KeyRowKey, PopoverKey, PopoverKeyItem, PopoverKeyTap, StickyModifier},
};

#[derive(Clone)]
pub struct ToolKeys {
    pub arrows: Entity<ArrowPad>,
    pub control: Entity<PopoverKey>,
    pub alt: Entity<PopoverKey>,
    pub prefix: Entity<PopoverKey>,
    pub scroll: ScrollHandle,
}

impl ToolKeys {
    pub fn new(cx: &mut App) -> Self {
        let scroll = ScrollHandle::new();
        let latch = |label: &'static str,
                     modifier: StickyModifier,
                     items: Vec<PopoverKeyItem>,
                     cx: &mut App| {
            cx.new(|cx| {
                PopoverKey::new(label, PopoverKeyTap::Latch(modifier), cx)
                    .icon(modifier.icon())
                    .items(items)
                    .columns(2)
                    .scrolls(scroll.clone())
            })
        };
        Self {
            arrows: cx.new(ArrowPad::new),
            control: latch("ctrl", StickyModifier::Control, control_chords(), cx),
            alt: latch("alt", StickyModifier::Alt, alt_chords(), cx),
            prefix: cx.new(|cx| PopoverKey::new("prefix", PopoverKeyTap::Press, cx)),
            scroll,
        }
    }

    pub fn lead(&self) -> KeyRowKey {
        KeyRowKey::Popover(self.prefix.clone())
    }

    pub fn row(&self) -> Vec<KeyRowKey> {
        vec![
            KeyRowKey::Popover(self.control.clone()),
            KeyRowKey::Popover(self.alt.clone()),
            KeyRowKey::tab(),
            KeyRowKey::named("esc", "escape").icon(IconName::KeyEscape),
            KeyRowKey::text("/"),
            KeyRowKey::text("@"),
            KeyRowKey::Arrows(self.arrows.clone()),
            KeyRowKey::Hide,
        ]
    }

    pub fn close(&self, cx: &mut App) {
        for key in [&self.control, &self.alt, &self.prefix] {
            if key.read(cx).is_open() {
                key.update(cx, PopoverKey::close);
            }
        }
    }
}

fn chords(chords: &[(&'static str, &'static str, &'static str)]) -> Vec<PopoverKeyItem> {
    chords
        .iter()
        .map(|(keystroke, cap, label)| PopoverKeyItem::new(*keystroke, *label).cap(*cap))
        .collect()
}

fn control_chords() -> Vec<PopoverKeyItem> {
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

fn alt_chords() -> Vec<PopoverKeyItem> {
    chords(&[
        ("alt-b", "M-b", "Word back"),
        ("alt-f", "M-f", "Word ahead"),
        ("alt-d", "M-d", "Kill word"),
        ("alt-.", "M-.", "Last arg"),
    ])
}
