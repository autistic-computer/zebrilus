use ala::{App, Keystroke, Modifiers};

use super::key_row::APPLE;
use crate::IconName;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StickyModifier {
    Control,
    Alt,
    Command,
}

impl StickyModifier {
    pub fn icon(self) -> IconName {
        match (self, APPLE) {
            (Self::Control, true) => IconName::KeyControl,
            (Self::Control, false) => IconName::KeyControlIso,
            (Self::Alt, true) => IconName::KeyOption,
            (Self::Alt, false) => IconName::KeyAltIso,
            (Self::Command, _) => IconName::KeyCommand,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StickyModifiers {
    pub control: bool,
    pub alt: bool,
    pub command: bool,
}

impl ala::Global for StickyModifiers {}

impl StickyModifiers {
    pub fn get(cx: &App) -> Self {
        cx.try_global::<Self>().copied().unwrap_or_default()
    }

    pub fn toggle(modifier: StickyModifier, cx: &mut App) {
        cx.haptic(ala::Haptic::Selection);
        let latched = cx.default_global::<Self>();
        match modifier {
            StickyModifier::Control => latched.control = !latched.control,
            StickyModifier::Alt => latched.alt = !latched.alt,
            StickyModifier::Command => latched.command = !latched.command,
        }
    }

    pub fn is_empty(&self) -> bool {
        !self.control && !self.alt && !self.command
    }

    pub fn is_latched(&self, modifier: StickyModifier) -> bool {
        match modifier {
            StickyModifier::Control => self.control,
            StickyModifier::Alt => self.alt,
            StickyModifier::Command => self.command,
        }
    }

    pub fn take(cx: &mut App) -> Self {
        if Self::get(cx).is_empty() {
            return Self::default();
        }
        std::mem::take(cx.default_global::<Self>())
    }

    pub fn apply(keystroke: &Keystroke, cx: &mut App) -> Keystroke {
        let latched = Self::take(cx);
        if latched.is_empty() {
            return keystroke.clone();
        }
        Keystroke {
            modifiers: Modifiers {
                control: keystroke.modifiers.control || latched.control,
                alt: keystroke.modifiers.alt || latched.alt,
                platform: keystroke.modifiers.platform || latched.command,
                ..keystroke.modifiers
            },
            key: keystroke.key.clone(),
            key_char: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use ala::TestAppContext;

    use super::*;

    #[ala::test]
    fn toggle_latches_and_unlatches(cx: &mut TestAppContext) {
        cx.update(|cx| {
            assert!(StickyModifiers::get(cx).is_empty());
            StickyModifiers::toggle(StickyModifier::Control, cx);
            assert_eq!(
                StickyModifiers::get(cx),
                StickyModifiers {
                    control: true,
                    ..StickyModifiers::default()
                }
            );
            StickyModifiers::toggle(StickyModifier::Alt, cx);
            StickyModifiers::toggle(StickyModifier::Control, cx);
            assert_eq!(
                StickyModifiers::get(cx),
                StickyModifiers {
                    alt: true,
                    ..StickyModifiers::default()
                }
            );
        });
    }

    #[ala::test]
    fn take_reads_and_clears(cx: &mut TestAppContext) {
        cx.update(|cx| {
            assert!(StickyModifiers::take(cx).is_empty());
            StickyModifiers::toggle(StickyModifier::Alt, cx);
            let taken = StickyModifiers::take(cx);
            assert!(taken.alt);
            assert!(taken.is_latched(StickyModifier::Alt));
            assert!(!taken.is_latched(StickyModifier::Control));
            assert!(StickyModifiers::get(cx).is_empty());
        });
    }

    #[ala::test]
    fn apply_merges_the_latch_once(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let plain = Keystroke::parse("c").unwrap();
            let plain = Keystroke {
                key_char: Some("c".into()),
                ..plain
            };
            assert_eq!(StickyModifiers::apply(&plain, cx), plain);

            StickyModifiers::toggle(StickyModifier::Control, cx);
            let applied = StickyModifiers::apply(&plain, cx);
            assert!(applied.modifiers.control);
            assert!(!applied.modifiers.alt);
            assert_eq!(applied.key, "c");
            assert_eq!(applied.key_char, None);
            assert!(StickyModifiers::get(cx).is_empty());
            assert_eq!(StickyModifiers::apply(&plain, cx), plain);

            StickyModifiers::toggle(StickyModifier::Alt, cx);
            let shifted = Keystroke::parse("shift-tab").unwrap();
            let applied = StickyModifiers::apply(&shifted, cx);
            assert!(applied.modifiers.alt && applied.modifiers.shift);
            assert!(!applied.modifiers.control);

            StickyModifiers::toggle(StickyModifier::Command, cx);
            let applied = StickyModifiers::apply(&plain, cx);
            assert!(
                applied.modifiers.platform && !applied.modifiers.control && !applied.modifiers.alt
            );
        });
    }
}
