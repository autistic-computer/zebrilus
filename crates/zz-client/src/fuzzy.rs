use std::ops::Range;

#[must_use]
pub fn fuzzy_match(query: &str, label: &str) -> Option<(usize, Vec<Range<usize>>)> {
    let mut needle = query
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .flat_map(char::to_lowercase)
        .peekable();
    if needle.peek().is_none() {
        return Some((0, Vec::new()));
    }
    let mut hits = Vec::new();
    let mut first = None;
    let mut previous = 0;
    let mut gaps = 0;
    for (position, (byte, character)) in label.char_indices().enumerate() {
        if character
            .to_lowercase()
            .any(|ch| Some(&ch) == needle.peek())
        {
            needle.next();
            if first.is_none() {
                first = Some(position);
            } else {
                gaps += position - previous - 1;
            }
            previous = position;
            hits.push(byte..byte + character.len_utf8());
            if needle.peek().is_none() {
                return Some((first.unwrap_or_default() + 2 * gaps, hits));
            }
        }
    }
    None
}
