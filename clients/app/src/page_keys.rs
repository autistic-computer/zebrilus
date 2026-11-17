use ala_cef::BrowserKey;
use serde::Serialize;
use zz_protocol::{BrowserCommand, KeyToken, MAX_BROWSER_KEY_REPEAT};
use zz_terminal::{KeyAction, KeyCode, KeyInput};

#[derive(Debug, PartialEq, Serialize)]
#[serde(untagged)]
enum Step {
    Text {
        text: String,
    },
    Key {
        key: String,
        shift: bool,
        ctrl: bool,
        alt: bool,
        meta: bool,
    },
}

#[must_use]
pub fn script(command: &BrowserCommand) -> Option<String> {
    let (steps, count) = match command {
        BrowserCommand::SendKeys(keys) => (token_steps(keys), 1),
        BrowserCommand::SendKeysRepeated { keys, count } => {
            (token_steps(keys), (*count).min(MAX_BROWSER_KEY_REPEAT))
        }
        BrowserCommand::Key(input) => (input_step(input).into_iter().collect(), 1),
        _ => return None,
    };
    if steps.is_empty() || count == 0 {
        return None;
    }
    let steps = serde_json::to_string(&steps).ok()?;
    Some(format!("{PAGE_KEYS_SCRIPT}({steps}, {count});"))
}

fn token_steps(tokens: &[KeyToken]) -> Vec<Step> {
    tokens
        .iter()
        .filter_map(|token| match token {
            KeyToken::Literal(text) => Some(Step::Text { text: text.clone() }),
            KeyToken::Named(name) => {
                let input = ala_cef::named_key_input(name)?;
                let modifiers = input.modifiers;
                Some(key_step(
                    browser_key_name(input.key)?,
                    [
                        modifiers.shift(),
                        modifiers.control(),
                        modifiers.alt(),
                        modifiers.platform(),
                    ],
                ))
            }
            KeyToken::Raw(_) => None,
        })
        .collect()
}

fn input_step(input: &KeyInput) -> Option<Step> {
    if input.action == KeyAction::Release {
        return None;
    }
    let modifiers = input.modifiers;
    let plain = !modifiers.control() && !modifiers.alt() && !modifiers.platform();
    if plain && let Some(text) = input.text.as_deref().filter(|text| !text.is_empty()) {
        return Some(Step::Text {
            text: text.to_owned(),
        });
    }
    let key = match input.key {
        KeyCode::Character(character) => character.to_string(),
        KeyCode::Backspace => "Backspace".to_owned(),
        KeyCode::Enter => "Enter".to_owned(),
        KeyCode::Tab => "Tab".to_owned(),
        KeyCode::Escape => "Escape".to_owned(),
        KeyCode::Delete => "Delete".to_owned(),
        KeyCode::Insert => "Insert".to_owned(),
        KeyCode::Home => "Home".to_owned(),
        KeyCode::End => "End".to_owned(),
        KeyCode::PageUp => "PageUp".to_owned(),
        KeyCode::PageDown => "PageDown".to_owned(),
        KeyCode::ArrowUp => "ArrowUp".to_owned(),
        KeyCode::ArrowDown => "ArrowDown".to_owned(),
        KeyCode::ArrowLeft => "ArrowLeft".to_owned(),
        KeyCode::ArrowRight => "ArrowRight".to_owned(),
        KeyCode::Function(number) => format!("F{number}"),
        KeyCode::Unidentified | KeyCode::User(_) => return None,
    };
    Some(key_step(
        key,
        [
            modifiers.shift(),
            modifiers.control(),
            modifiers.alt(),
            modifiers.platform(),
        ],
    ))
}

fn key_step(key: String, [shift, ctrl, alt, meta]: [bool; 4]) -> Step {
    Step::Key {
        key,
        shift,
        ctrl,
        alt,
        meta,
    }
}

fn browser_key_name(key: BrowserKey) -> Option<String> {
    Some(match key {
        BrowserKey::Character(character) => character.to_string(),
        BrowserKey::Space => " ".to_owned(),
        BrowserKey::Backspace => "Backspace".to_owned(),
        BrowserKey::Tab => "Tab".to_owned(),
        BrowserKey::Enter => "Enter".to_owned(),
        BrowserKey::Escape => "Escape".to_owned(),
        BrowserKey::PageUp => "PageUp".to_owned(),
        BrowserKey::PageDown => "PageDown".to_owned(),
        BrowserKey::End => "End".to_owned(),
        BrowserKey::Home => "Home".to_owned(),
        BrowserKey::ArrowLeft => "ArrowLeft".to_owned(),
        BrowserKey::ArrowUp => "ArrowUp".to_owned(),
        BrowserKey::ArrowRight => "ArrowRight".to_owned(),
        BrowserKey::ArrowDown => "ArrowDown".to_owned(),
        BrowserKey::Insert => "Insert".to_owned(),
        BrowserKey::Delete => "Delete".to_owned(),
        BrowserKey::Function(number) => format!("F{number}"),
        BrowserKey::Unidentified => return None,
    })
}

const PAGE_KEYS_SCRIPT: &str = r"((steps, count) => {
  const fields = new Set(['text', 'search', 'url', 'tel', 'email', 'password', 'number']);
  const editable = el => el?.isContentEditable || el instanceof HTMLTextAreaElement
    || (el instanceof HTMLInputElement && fields.has(el.type));
  const codes = { Backspace: 8, Tab: 9, Enter: 13, Escape: 27, ' ': 32, PageUp: 33, PageDown: 34,
    End: 35, Home: 36, ArrowLeft: 37, ArrowUp: 38, ArrowRight: 39, ArrowDown: 40, Insert: 45, Delete: 46 };
  const fire = (el, type, step) => {
    const event = new KeyboardEvent(type, { key: step.key, shiftKey: step.shift, ctrlKey: step.ctrl,
      altKey: step.alt, metaKey: step.meta, bubbles: true, cancelable: true, composed: true });
    const code = codes[step.key] ?? (step.key.length === 1 ? step.key.toUpperCase().charCodeAt(0) : 0);
    Object.defineProperty(event, 'keyCode', { get: () => code });
    Object.defineProperty(event, 'which', { get: () => code });
    return el.dispatchEvent(event);
  };
  const scroll = (key, shift) => {
    const page = document.scrollingElement ?? document.documentElement;
    const screen = innerHeight * 0.875;
    const by = { ArrowDown: [0, 40], ArrowUp: [0, -40], ArrowRight: [40, 0], ArrowLeft: [-40, 0],
      PageDown: [0, screen], PageUp: [0, -screen], ' ': [0, shift ? -screen : screen] }[key];
    if (by) page.scrollBy(by[0], by[1]);
    else if (key === 'Home') page.scrollTo(page.scrollLeft, 0);
    else if (key === 'End') page.scrollTo(page.scrollLeft, page.scrollHeight);
  };
  const tab = back => {
    const all = [...document.querySelectorAll('a[href], button, input, select, textarea, summary, iframe, [tabindex], [contenteditable]')]
      .filter(el => el.tabIndex >= 0 && !el.disabled && el.getClientRects().length);
    if (!all.length) return;
    const at = all.indexOf(document.activeElement);
    const next = at < 0 ? (back ? all.length - 1 : 0) : (at + (back ? -1 : 1) + all.length) % all.length;
    all[next].focus();
  };
  const caret = (el, step) => {
    const selection = getSelection();
    const how = step.shift ? 'extend' : 'move';
    const moves = { ArrowLeft: ['backward', 'character'], ArrowRight: ['forward', 'character'],
      ArrowUp: ['backward', 'line'], ArrowDown: ['forward', 'line'],
      Home: ['backward', 'lineboundary'], End: ['forward', 'lineboundary'] }[step.key];
    if (!moves) return;
    if (el.isContentEditable) return selection.modify(how, ...moves);
    const length = el.value.length;
    const end = el.selectionDirection === 'backward' ? el.selectionStart : el.selectionEnd;
    const to = { ArrowLeft: end - 1, ArrowRight: end + 1, Home: 0, End: length, ArrowUp: 0, ArrowDown: length }[step.key];
    const at = Math.max(0, Math.min(length, to));
    if (!step.shift) return el.setSelectionRange(at, at);
    const anchor = el.selectionDirection === 'backward' ? el.selectionEnd : el.selectionStart;
    el.setSelectionRange(Math.min(anchor, at), Math.max(anchor, at), at < anchor ? 'backward' : 'forward');
  };
  const act = (el, step) => {
    const field = editable(el);
    if ((step.ctrl || step.meta) && step.key.toLowerCase() === 'a') return document.execCommand('selectAll');
    if (step.ctrl || step.alt || step.meta) return;
    if ([...step.key].length === 1) {
      if (field) document.execCommand('insertText', false, step.key);
      else if (step.key === ' ') scroll(' ', step.shift);
      return;
    }
    switch (step.key) {
      case 'Enter':
        if (el instanceof HTMLTextAreaElement) document.execCommand('insertText', false, '\n');
        else if (el?.isContentEditable) document.execCommand(step.shift ? 'insertLineBreak' : 'insertParagraph');
        else if (el instanceof HTMLInputElement) el.form?.requestSubmit();
        else if (el?.matches?.('a[href], button, summary')) el.click();
        return;
      case 'Backspace': if (field) document.execCommand('delete'); return;
      case 'Delete': if (field) document.execCommand('forwardDelete'); return;
      case 'Tab': return tab(step.shift);
      case 'Escape': return;
      default: return field ? caret(el, step) : scroll(step.key, step.shift);
    }
  };
  const press = step => {
    const el = document.activeElement ?? document.body;
    if (fire(el, 'keydown', step)) act(el, step);
    fire(document.activeElement ?? el, 'keyup', step);
  };
  for (let round = 0; round < count; round++) {
    for (const step of steps) {
      if (step.text === undefined) press(step);
      else for (const key of step.text) press({ key, shift: false, ctrl: false, alt: false, meta: false });
    }
  }
})";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tmux_key_names_become_page_keys() {
        let keys = vec![
            KeyToken::Literal("hi".to_owned()),
            KeyToken::Named("C-a".to_owned()),
            KeyToken::Named("BTab".to_owned()),
            KeyToken::Named("BSpace".to_owned()),
            KeyToken::Named("Space".to_owned()),
            KeyToken::Raw(3),
        ];
        let script = script(&BrowserCommand::SendKeysRepeated {
            keys,
            count: 20_000,
        })
        .expect("keys produce a script");
        let arguments = script
            .strip_prefix(PAGE_KEYS_SCRIPT)
            .expect("script starts with the page function");
        assert_eq!(
            arguments,
            concat!(
                r#"([{"text":"hi"},"#,
                r#"{"key":"a","shift":false,"ctrl":true,"alt":false,"meta":false},"#,
                r#"{"key":"Tab","shift":true,"ctrl":false,"alt":false,"meta":false},"#,
                r#"{"key":"Backspace","shift":false,"ctrl":false,"alt":false,"meta":false},"#,
                r#"{"key":" ","shift":false,"ctrl":false,"alt":false,"meta":false}], 9999);"#,
            )
        );
        assert_eq!(
            super::script(&BrowserCommand::SendKeys(vec![KeyToken::Raw(3)])),
            None
        );
    }
}
