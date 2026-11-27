use super::*;
use ala::{KeyContext, Keymap, Keystroke};

pub(super) fn bindings_for_contexts(
    keymap: &Keymap,
    source: &str,
    context_names: &[&str],
) -> Vec<ala::KeyBinding> {
    let keystroke = Keystroke::parse(source).expect("valid browser keystroke");
    let contexts = context_names
        .iter()
        .map(|context| KeyContext::parse(context).expect("valid key context"))
        .collect::<Vec<_>>();
    let (bindings, pending) = keymap.bindings_for_input(&[keystroke], &contexts);
    assert!(!pending);
    bindings.into_vec()
}

pub(super) fn browser_bindings_for(keymap: &Keymap, source: &str) -> Vec<ala::KeyBinding> {
    bindings_for_contexts(keymap, source, &["Root", BROWSER_KEY_CONTEXT])
}

pub(super) fn browser_key_bindings() -> Vec<KeyBinding> {
    let mut bindings = raw_key_bindings().to_vec();
    bindings.extend(browser_chrome_bindings(&crate::keymap::test_chords(
        BROWSER_TABLE,
    )));
    bindings
}

pub(super) fn edit_action_command(action: &dyn ala::Action) -> Option<EditCommand> {
    if let Some(action) = action.as_any().downcast_ref::<BrowserEdit>() {
        return Some(action.command);
    }
    if action.as_any().is::<crate::menus::Undo>() {
        return Some(EditCommand::Undo);
    }
    if action.as_any().is::<crate::menus::Redo>() {
        return Some(EditCommand::Redo);
    }
    if action.as_any().is::<crate::menus::Cut>() {
        return Some(EditCommand::Cut);
    }
    if action.as_any().is::<crate::menus::Copy>() {
        return Some(EditCommand::Copy);
    }
    if action.as_any().is::<crate::menus::Paste>() {
        return Some(EditCommand::Paste);
    }
    if action.as_any().is::<crate::menus::SelectAll>() {
        return Some(EditCommand::SelectAll);
    }
    None
}
