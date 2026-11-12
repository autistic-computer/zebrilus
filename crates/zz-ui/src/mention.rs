use std::ops::Range;

use ala::{App, FontWeight, HighlightStyle};

use crate::{ActiveTheme as _, Emphasis, Tone};

pub(crate) fn starts_ref(text: &str, at: usize) -> bool {
    text[..at]
        .chars()
        .next_back()
        .is_none_or(|before| before.is_whitespace() || "([{'\"*_".contains(before))
}

pub(crate) fn ref_end(text: &str, start: usize) -> usize {
    let mut end = text.len();
    let mut escaped = false;
    for (offset, character) in text[start + 1..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
            continue;
        }
        if character.is_whitespace() || "`)]}<>\"',".contains(character) {
            end = start + 1 + offset;
            break;
        }
    }
    let trimmed = text[..end].trim_end_matches(['.', ';', ':', '!', '?']);
    trimmed.len().max(start + 1)
}

#[must_use]
pub fn ref_spans(text: &str) -> Vec<Range<usize>> {
    let mut spans = Vec::new();
    let mut from = 0;
    while let Some(found) = text[from..].find('@') {
        let start = from + found;
        if starts_ref(text, start) {
            let end = ref_end(text, start);
            if end > start + 1 {
                spans.push(start..end);
                from = end;
                continue;
            }
        }
        from = start + 1;
    }
    spans
}

pub(crate) fn ref_highlight(cx: &App) -> HighlightStyle {
    let paint = cx.theme().paint(Tone::Primary, Emphasis::Soft);
    HighlightStyle {
        color: Some(paint.text),
        background_color: Some(paint.fill),
        font_weight: Some(FontWeight::MEDIUM),
        ..HighlightStyle::default()
    }
}

#[derive(Clone, Debug, Default)]
pub struct Mentions {
    text: String,
    names: Vec<Name>,
}

#[derive(Clone, Debug)]
struct Name {
    range: Range<usize>,
    shown: String,
    raw: String,
}

impl Mentions {
    pub fn follow(&mut self, text: &str) -> Range<usize> {
        if self.text == text {
            return 0..0;
        }
        let prefix: usize = self
            .text
            .chars()
            .zip(text.chars())
            .take_while(|(old, new)| old == new)
            .map(|(old, _)| old.len_utf8())
            .sum();
        let room = self.text.len().min(text.len()) - prefix;
        let mut suffix = 0;
        for (old, new) in self.text.chars().rev().zip(text.chars().rev()) {
            if old != new || suffix + old.len_utf8() > room {
                break;
            }
            suffix += old.len_utf8();
        }
        let removed = prefix..self.text.len() - suffix;
        let added = prefix..text.len() - suffix;
        self.shift(&removed, added.len());
        text.clone_into(&mut self.text);
        added
    }

    pub fn place(&mut self, range: Range<usize>, raw: &str, label: &str, tail: &str) -> String {
        let shown = self.shown(raw, label);
        let insert = format!("{shown}{tail}");
        let at = range.start;
        self.splice(range, &insert);
        self.add(at, shown, raw);
        insert
    }

    pub fn name_refs(
        &mut self,
        range: Range<usize>,
        resolve: impl Fn(&str) -> Option<String>,
    ) -> Option<String> {
        let spans: Vec<Range<usize>> = ref_spans(&self.text)
            .into_iter()
            .filter(|span| span.start >= range.start && span.end <= range.end)
            .collect();
        let (mut grew, mut shrank) = (0, 0);
        for span in spans {
            let found = span.start + grew - shrank..span.end + grew - shrank;
            let raw = self.text[found.clone()].to_owned();
            let Some(label) = resolve(&raw[1..]) else {
                continue;
            };
            let shown = self.shown(&raw, &label);
            grew += shown.len();
            shrank += raw.len();
            let at = found.start;
            self.splice(found, &shown);
            self.add(at, shown, &raw);
        }
        (grew > 0).then(|| self.text[range.start..range.end + grew - shrank].to_owned())
    }

    #[must_use]
    pub fn raw(&self) -> String {
        let mut raw = String::with_capacity(self.text.len());
        let mut last = 0;
        for name in self.live() {
            raw.push_str(&self.text[last..name.range.start]);
            raw.push_str(&name.raw);
            last = name.range.end;
        }
        raw.push_str(&self.text[last..]);
        raw
    }

    #[must_use]
    pub fn spans(&self) -> Vec<Range<usize>> {
        self.live().map(|name| name.range.clone()).collect()
    }

    #[must_use]
    pub fn covers(&self, at: usize) -> bool {
        self.names
            .iter()
            .any(|name| name.range.start < at && at <= name.range.end)
    }

    fn live(&self) -> impl Iterator<Item = &Name> {
        self.names.iter().filter(|name| self.stands(name))
    }

    fn stands(&self, name: &Name) -> bool {
        let rest = &self.text[name.range.end..];
        let tail = &rest[..rest.find(char::is_whitespace).unwrap_or(rest.len())];
        starts_ref(&self.text, name.range.start)
            && (tail.is_empty() || ref_end(&format!("{}{tail}", name.raw), 0) == name.raw.len())
    }

    fn shown(&self, raw: &str, label: &str) -> String {
        if let Some(name) = self.names.iter().find(|name| name.raw == raw) {
            return name.shown.clone();
        }
        let base = format!(
            "@{}",
            label.split_whitespace().collect::<Vec<_>>().join(" ")
        );
        let mut shown = base.clone();
        let mut count = 1;
        while self.names.iter().any(|name| name.shown == shown) {
            count += 1;
            shown = format!("{base} ({count})");
        }
        shown
    }

    fn add(&mut self, at: usize, shown: String, raw: &str) {
        let index = self.names.partition_point(|name| name.range.start < at);
        self.names.insert(
            index,
            Name {
                range: at..at + shown.len(),
                shown,
                raw: raw.to_owned(),
            },
        );
    }

    fn splice(&mut self, range: Range<usize>, insert: &str) {
        self.shift(&range, insert.len());
        self.text.replace_range(range, insert);
    }

    fn shift(&mut self, removed: &Range<usize>, added: usize) {
        self.names.retain_mut(|name| {
            if removed.end <= name.range.start {
                name.range = name.range.start - removed.len() + added
                    ..name.range.end - removed.len() + added;
                true
            } else {
                removed.start >= name.range.end
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::Mentions;

    fn resolve(body: &str) -> Option<String> {
        matches!(body, "macbook:%6" | "macbook:%7").then(|| "build\nlogs".to_owned())
    }

    #[test]
    fn a_picked_target_shows_its_name_and_sends_its_reference() {
        let mut mentions = Mentions::default();
        mentions.follow("compare ");
        assert_eq!(
            mentions.place(8..8, "@macbook:%6", "build logs", " "),
            "@build logs "
        );
        assert_eq!(
            mentions.place(20..20, "@macbook:%7", "build logs", " "),
            "@build logs (2) "
        );
        assert_eq!(
            mentions.place(36..36, "@macbook:%6", "build logs", " "),
            "@build logs "
        );
        let shown = "compare @build logs @build logs (2) @build logs and @macbook:%6";
        mentions.follow(shown);
        assert_eq!(mentions.spans(), [8..19, 20..35, 36..47]);
        assert_eq!(
            mentions.raw(),
            "compare @macbook:%6 @macbook:%7 @macbook:%6 and @macbook:%6"
        );
        for typed in [
            "compare @build lgs @build logs (2) @build logs and @macbook:%6",
            "compare @build lgs @build logs (2), @build logs and @macbook:%6",
            "compare @build lgs @build logs (2), @build logsx and @macbook:%6",
        ] {
            mentions.follow(typed);
        }
        assert_eq!(
            mentions.raw(),
            "compare @build lgs @macbook:%7, @build logsx and @macbook:%6"
        );
    }

    #[test]
    fn raw_text_entering_the_composer_shows_names_and_leaves_without_them() {
        let raw = "look at @macbook:%6, then @gone:%1 and (@macbook:%7).";
        let mut mentions = Mentions::default();
        mentions.follow("draft\n");
        let changed = mentions.follow(&format!("draft\n{raw}"));
        assert_eq!(changed, 6..6 + raw.len());
        assert_eq!(
            mentions.name_refs(changed, resolve).as_deref(),
            Some("look at @build logs, then @gone:%1 and (@build logs (2)).")
        );
        assert_eq!(mentions.raw(), format!("draft\n{raw}"));
    }
}
