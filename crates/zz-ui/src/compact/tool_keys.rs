use ala::{App, AppContext as _, Entity, Keystroke, Modifiers};

use crate::{
    IconName,
    touch::{
        ArrowPad, KeyRowKey, StackChip, StackGroup, StackKey, StackKeyTap, StackMenu,
        StickyModifier,
    },
};

const APPLE: bool = cfg!(any(target_os = "macos", target_os = "ios"));

#[derive(Clone)]
pub struct ToolKeys {
    pub arrows: Entity<ArrowPad>,
    pub control: Entity<StackKey>,
    pub alt: Entity<StackKey>,
    pub command: Option<Entity<StackKey>>,
    pub insert: Entity<StackKey>,
    pub prefix: Entity<StackKey>,
}

impl ToolKeys {
    pub fn new(cx: &mut App) -> Self {
        let latch = |label: &'static str,
                     modifier: StickyModifier,
                     chords: Vec<StackChip>,
                     cx: &mut App| {
            cx.new(|cx| {
                StackKey::new(label, StackKeyTap::Latch(modifier), cx)
                    .icon(modifier.icon())
                    .groups(vec![StackGroup::new(chords)], cx)
            })
        };
        Self {
            arrows: cx.new(ArrowPad::new),
            control: latch("ctrl", StickyModifier::Control, control_chords(), cx),
            alt: latch("alt", StickyModifier::Alt, alt_chords(), cx),
            command: APPLE.then(|| latch("cmd", StickyModifier::Command, command_chords(), cx)),
            insert: cx.new(|cx| {
                StackKey::new("insert", StackKeyTap::Open, cx)
                    .icon(IconName::Ellipsis)
                    .groups(vec![StackGroup::new(insert_chips())], cx)
            }),
            prefix: cx.new(|cx| StackKey::new("prefix", StackKeyTap::Open, cx)),
        }
    }

    pub fn lead(&self) -> KeyRowKey {
        KeyRowKey::Stack(self.prefix.clone())
    }

    pub fn row(&self, lead: Option<KeyRowKey>) -> Vec<KeyRowKey> {
        lead.into_iter()
            .chain([
                KeyRowKey::Stack(self.control.clone()),
                KeyRowKey::Stack(self.alt.clone()),
            ])
            .chain(self.command.clone().map(KeyRowKey::Stack))
            .chain([KeyRowKey::Stack(self.insert.clone())])
            .collect()
    }

    pub fn discs(&self) -> Vec<KeyRowKey> {
        vec![KeyRowKey::Arrows(self.arrows.clone()), KeyRowKey::Done]
    }

    fn keys(&self) -> impl Iterator<Item = &Entity<StackKey>> {
        [&self.control, &self.alt]
            .into_iter()
            .chain(self.command.as_ref())
            .chain([&self.insert, &self.prefix])
    }

    pub fn chord_menus(&self, cx: &App) -> Vec<Entity<StackMenu>> {
        [&self.control, &self.alt]
            .into_iter()
            .chain(self.command.as_ref())
            .chain([&self.insert])
            .map(|key| key.read(cx).menu().clone())
            .collect()
    }

    pub fn is_open(&self, cx: &App) -> bool {
        self.keys().any(|key| key.read(cx).is_open(cx))
    }

    pub fn close(&self, cx: &mut App) {
        for key in self.keys().cloned().collect::<Vec<_>>() {
            if key.read(cx).menu().read(cx).is_shown() {
                key.update(cx, StackKey::close);
            }
        }
    }
}

pub fn chip_keystroke(id: &str) -> Option<Keystroke> {
    let mut chars = id.chars();
    if let (Some(_), None) = (chars.next(), chars.next()) {
        return Some(Keystroke {
            modifiers: Modifiers::default(),
            key: id.to_owned(),
            key_char: Some(id.to_owned()),
        });
    }
    Keystroke::parse(id).ok()
}

fn chords(chords: &[(&'static str, &'static str)]) -> Vec<StackChip> {
    chords
        .iter()
        .map(|(keystroke, label)| StackChip::text(*keystroke, *label))
        .collect()
}

fn control_chords() -> Vec<StackChip> {
    chords(&[
        ("ctrl-c", "Interrupt"),
        ("ctrl-d", "End input"),
        ("ctrl-z", "Suspend"),
        ("ctrl-l", "Clear"),
        ("ctrl-r", "History"),
        ("ctrl-u", "Kill line"),
        ("ctrl-w", "Kill word"),
        ("ctrl-k", "Kill to end"),
        ("ctrl-a", "Line start"),
        ("ctrl-e", "Line end"),
    ])
}

fn alt_chords() -> Vec<StackChip> {
    chords(&[
        ("alt-b", "Word back"),
        ("alt-f", "Word ahead"),
        ("alt-d", "Kill word"),
        ("alt-.", "Last arg"),
    ])
}

fn command_chords() -> Vec<StackChip> {
    chords(&[
        ("cmd-v", "Paste"),
        ("cmd-c", "Copy"),
        ("cmd-f", "Find"),
        ("cmd-a", "Select all"),
    ])
}

fn insert_chips() -> Vec<StackChip> {
    chords(&[
        ("escape", "Escape"),
        ("tab", "Tab"),
        ("/", "Slash"),
        ("@", "At"),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chip_sends_one_character_as_text_and_a_name_as_a_key() {
        let slash = chip_keystroke("/").unwrap();
        assert_eq!(
            (slash.key.as_str(), slash.key_char.as_deref()),
            ("/", Some("/"))
        );
        let escape = chip_keystroke("escape").unwrap();
        assert_eq!((escape.key.as_str(), escape.key_char), ("escape", None));
        assert!(chip_keystroke("cmd-v").unwrap().modifiers.platform);
    }
}
