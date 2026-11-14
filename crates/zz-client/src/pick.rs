use std::{cell::RefCell, cmp::Ordering, ops::Range};

const MATCH: i32 = 10;
const WORD_START: i32 = 18;
const CONSECUTIVE: i32 = 18;
const BASE: i32 = 10;
const GAP: i32 = 1;
const WINDOW: i64 = 24;
const SESSION: i64 = 16;
const HOST: i64 = 8;
const CHANGED: i64 = 12;
const RECENT: f32 = 20.0;
const DEPTH: i64 = 3;
const FLOOR: f64 = 0.6;
const NONE: i32 = i32::MIN;
const SEPARATORS: [char; 8] = [' ', '/', '_', '-', '.', ':', '›', '·'];
const CRUMB_BREAKS: [char; 4] = [' ', '/', '›', '·'];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Action,
    Place,
    Chat,
    File,
    Dir,
}

impl Kind {
    const fn is_path(self) -> bool {
        matches!(self, Self::File | Self::Dir)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Closeness {
    #[default]
    Elsewhere,
    Host,
    Session,
    Window,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Prior {
    pub closeness: Closeness,
    pub changed: bool,
    pub recent: f32,
    pub depth: u32,
}

impl Prior {
    #[must_use]
    pub fn score(&self) -> i64 {
        let closeness = match self.closeness {
            Closeness::Elsewhere => 0,
            Closeness::Host => HOST,
            Closeness::Session => SESSION,
            Closeness::Window => WINDOW,
        };
        let changed = if self.changed { CHANGED } else { 0 };
        let recent = (RECENT * self.recent.clamp(0.0, 1.0)).round() as i64;
        closeness + changed + recent - DEPTH * i64::from(self.depth)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate<'text> {
    pub kind: Kind,
    pub label: &'text str,
    pub path: &'text str,
    pub prior: Prior,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Match {
    pub score: i64,
    pub label: Vec<Range<usize>>,
    pub path: Vec<Range<usize>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    pub index: usize,
    pub score: i64,
    pub label: Vec<Range<usize>>,
    pub path: Vec<Range<usize>>,
}

#[derive(Clone, Debug)]
struct Word {
    chars: Vec<char>,
    smart: bool,
}

impl Word {
    fn new(text: &str) -> Self {
        let smart = text.chars().any(char::is_uppercase);
        let chars = text
            .chars()
            .filter(|character| !character.is_whitespace())
            .map(|character| if smart { character } else { fold(character) })
            .collect();
        Self { chars, smart }
    }

    fn fold(&self, character: char) -> char {
        if self.smart {
            character
        } else {
            fold(character)
        }
    }

    fn within(&self, text: impl Iterator<Item = char>) -> bool {
        let mut wanted = self.chars.iter().peekable();
        for character in text {
            let Some(next) = wanted.peek() else {
                return true;
            };
            if self.fold(character) == **next {
                wanted.next();
            }
        }
        wanted.peek().is_none()
    }

    fn inside(&self, text: &str) -> bool {
        let length = self.chars.len();
        if text.is_ascii() && self.chars.iter().all(char::is_ascii) {
            return text.as_bytes().windows(length).any(|window| {
                window
                    .iter()
                    .zip(&self.chars)
                    .all(|(byte, wanted)| self.fold(char::from(*byte)) == *wanted)
            });
        }
        self.within(text.chars())
    }

    fn starts_within(&self, text: &str) -> bool {
        let Some((first, _)) = self.chars.split_first() else {
            return false;
        };
        let mut before = None;
        let mut characters = text.chars();
        for character in characters.by_ref() {
            let start = before.is_none_or(|before: char| {
                SEPARATORS.contains(&before)
                    || (before.is_ascii_lowercase() && character.is_ascii_uppercase())
            });
            before = Some(character);
            if start && self.fold(character) == *first {
                let mut wanted = self.chars[1..].iter().peekable();
                for character in characters {
                    let Some(next) = wanted.peek() else {
                        return true;
                    };
                    if self.fold(character) == **next {
                        wanted.next();
                    }
                }
                return wanted.peek().is_none();
            }
        }
        false
    }
}

#[derive(Clone, Debug, Default)]
struct Text {
    raw: Vec<char>,
    lower: Vec<char>,
    starts: Vec<bool>,
    offsets: Vec<usize>,
}

impl Text {
    fn fill(&mut self, parts: &[&str]) {
        self.raw.clear();
        self.offsets.clear();
        let mut base = 0;
        for part in parts {
            for (offset, character) in part.char_indices() {
                self.raw.push(character);
                self.offsets.push(base + offset);
            }
            base += part.len();
        }
        self.offsets.push(base);
        self.lower.clear();
        self.lower.extend(self.raw.iter().copied().map(fold));
        self.starts.clear();
        let raw = &self.raw;
        self.starts.extend((0..raw.len()).map(|at| {
            at == 0
                || SEPARATORS.contains(&raw[at - 1])
                || (raw[at - 1].is_ascii_lowercase() && raw[at].is_ascii_uppercase())
        }));
    }

    fn hay(&self, word: &Word) -> &[char] {
        if word.smart { &self.raw } else { &self.lower }
    }

    fn ranges(&self, mut hits: Vec<usize>) -> Vec<Range<usize>> {
        hits.sort_unstable();
        hits.dedup();
        let mut ranges: Vec<Range<usize>> = Vec::new();
        for at in hits {
            let (start, end) = (self.offsets[at], self.offsets[at + 1]);
            match ranges.last_mut() {
                Some(last) if last.end == start => last.end = end,
                _ => ranges.push(start..end),
            }
        }
        ranges
    }
}

#[derive(Clone, Debug, Default)]
struct Scratch {
    label: Text,
    path: Text,
    scores: Vec<i32>,
    links: Vec<usize>,
}

#[derive(Clone, Debug)]
pub struct Query {
    whole: Word,
    spaced: Vec<Word>,
    slashed: Vec<Word>,
    slash: bool,
    scratch: RefCell<Scratch>,
}

fn fold(character: char) -> char {
    if character.is_ascii() {
        character.to_ascii_lowercase()
    } else {
        character.to_lowercase().next().unwrap_or(character)
    }
}

fn fuzzy(
    word: &Word,
    text: &Text,
    base_from: usize,
    scores: &mut Vec<i32>,
    links: &mut Vec<usize>,
) -> Option<(i32, Vec<usize>)> {
    let hay = text.hay(word);
    let (rows, columns) = (word.chars.len(), hay.len());
    if rows == 0 || rows > columns {
        return None;
    }
    let bonus = |at: usize| {
        MATCH
            + if text.starts[at] { WORD_START } else { 0 }
            + if at >= base_from { BASE } else { 0 }
    };
    scores.clear();
    scores.resize(rows * columns, NONE);
    links.clear();
    links.resize(rows * columns, 0);
    for row in 0..rows {
        let mut run = NONE;
        let mut run_at = 0;
        for at in row..columns {
            if row > 0 && at >= 2 {
                if run != NONE {
                    run -= GAP;
                }
                let reach = scores[(row - 1) * columns + at - 2];
                if reach != NONE && reach - GAP > run {
                    run = reach - GAP;
                    run_at = at - 2;
                }
            }
            if hay[at] != word.chars[row] {
                continue;
            }
            let cell = row * columns + at;
            if row == 0 {
                if text.starts[at] {
                    scores[cell] = bonus(at);
                }
                continue;
            }
            let previous = scores[cell - columns - 1];
            if previous != NONE && (previous + CONSECUTIVE >= run || !text.starts[at]) {
                scores[cell] = bonus(at) + previous + CONSECUTIVE;
                links[cell] = at - 1;
            } else if run != NONE && text.starts[at] {
                scores[cell] = bonus(at) + run;
                links[cell] = run_at;
            }
        }
    }
    let last = (rows - 1) * columns;
    let mut end = None;
    for at in 0..columns {
        let score = scores[last + at];
        if score != NONE && end.is_none_or(|best: usize| score > scores[last + best]) {
            end = Some(at);
        }
    }
    let end = end?;
    let mut hits = vec![0; rows];
    let mut at = end;
    for row in (0..rows).rev() {
        hits[row] = at;
        at = links[row * columns + at];
    }
    Some((scores[last + end], hits))
}

fn contiguous(word: &Word, text: &Text) -> Option<(i32, Vec<usize>)> {
    let hay = text.hay(word);
    let length = word.chars.len();
    if length == 0 || length > hay.len() {
        return None;
    }
    let mut best: Option<(i32, usize)> = None;
    for at in 0..=hay.len() - length {
        if hay[at..at + length] != word.chars[..]
            || text.raw[at..at + length]
                .iter()
                .any(|character| CRUMB_BREAKS.contains(character))
        {
            continue;
        }
        let start = at == 0 || SEPARATORS.contains(&text.raw[at - 1]);
        let score = MATCH * length as i32
            + CONSECUTIVE * (length as i32 - 1)
            + if start { WORD_START } else { 0 };
        if best.is_none_or(|(top, _)| score > top) {
            best = Some((score, at));
        }
    }
    best.map(|(score, at)| (score, (at..at + length).collect()))
}

impl Query {
    #[must_use]
    pub fn new(text: &str) -> Self {
        Self {
            whole: Word::new(text),
            spaced: text.split_whitespace().map(Word::new).collect(),
            slashed: text
                .split(|character: char| character.is_whitespace() || character == '/')
                .filter(|word| !word.is_empty())
                .map(Word::new)
                .collect(),
            slash: text.contains('/'),
            scratch: RefCell::default(),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.whole.chars.is_empty()
    }

    #[must_use]
    pub fn matched(&self, candidate: &Candidate) -> Option<Match> {
        if self.is_empty() {
            return Some(Match::default());
        }
        if candidate.kind.is_path() && self.slash {
            return self.whole_path(candidate);
        }
        let words = if candidate.kind.is_path() {
            &self.spaced
        } else {
            &self.slashed
        };
        let mut crumbs = false;
        for word in words {
            let in_path = word.chars.len() > 1 && word.inside(candidate.path);
            if !in_path && !word.starts_within(candidate.label) {
                return None;
            }
            crumbs |= in_path;
        }
        let scratch = &mut *self.scratch.borrow_mut();
        let Scratch {
            label,
            path,
            scores,
            links,
        } = scratch;
        label.fill(&[candidate.label]);
        path.fill(&[if crumbs { candidate.path } else { "" }]);
        let mut score = 0;
        let (mut label_hits, mut path_hits) = (Vec::new(), Vec::new());
        for word in words {
            let in_label = fuzzy(word, label, 0, scores, links);
            let in_path = if crumbs && word.chars.len() > 1 {
                contiguous(word, path)
            } else {
                None
            };
            let (points, hits, into) = match (in_label, in_path) {
                (Some(named), Some(crumb)) if crumb.0 > named.0 => {
                    (crumb.0, crumb.1, &mut path_hits)
                }
                (Some(named), _) => (named.0, named.1, &mut label_hits),
                (None, Some(crumb)) => (crumb.0, crumb.1, &mut path_hits),
                (None, None) => return None,
            };
            score += i64::from(points);
            into.extend(hits);
        }
        Some(Match {
            score,
            label: label.ranges(label_hits),
            path: path.ranges(path_hits),
        })
    }

    fn whole_path(&self, candidate: &Candidate) -> Option<Match> {
        if !self
            .whole
            .within(candidate.path.chars().chain(candidate.label.chars()))
        {
            return None;
        }
        let scratch = &mut *self.scratch.borrow_mut();
        let Scratch {
            label: text,
            scores,
            links,
            ..
        } = scratch;
        text.fill(&[candidate.path, candidate.label]);
        let cut = candidate.path.len();
        let base_from = candidate.path.chars().count();
        let (score, hits) = fuzzy(&self.whole, text, base_from, scores, links)?;
        let (mut label, mut path) = (Vec::new(), Vec::new());
        for range in text.ranges(hits) {
            if range.start < cut {
                path.push(range.start..range.end.min(cut));
            }
            if range.end > cut {
                label.push(range.start.max(cut) - cut..range.end - cut);
            }
        }
        Some(Match {
            score: i64::from(score),
            label,
            path,
        })
    }
}

#[must_use]
pub fn rank(query: &str, candidates: &[Candidate]) -> Vec<Hit> {
    let query = Query::new(query);
    rank_by(candidates, |_, candidate| query.matched(candidate))
}

pub fn rank_by(
    candidates: &[Candidate],
    mut matcher: impl FnMut(usize, &Candidate) -> Option<Match>,
) -> Vec<Hit> {
    let mut typed = false;
    let mut found = candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            let found = matcher(index, candidate)?;
            typed |= found.score > 0;
            let hit = Hit {
                index,
                score: found.score + candidate.prior.score(),
                label: found.label,
                path: found.path,
            };
            Some((hit, candidate.label.len() + candidate.path.len()))
        })
        .collect::<Vec<_>>();
    found.sort_by(|(left, left_length), (right, right_length)| {
        order(left, *left_length, right, *right_length)
    });
    let best = found.first().map_or(0, |(hit, _)| hit.score);
    if typed {
        found.retain(|(hit, _)| keeps(hit.score, best));
    }
    found.into_iter().map(|(hit, _)| hit).collect()
}

#[must_use]
pub fn keeps(score: i64, best: i64) -> bool {
    best <= 0 || score as f64 >= best as f64 * FLOOR
}

fn order(left: &Hit, left_length: usize, right: &Hit, right_length: usize) -> Ordering {
    right
        .score
        .cmp(&left.score)
        .then(left_length.cmp(&right_length))
        .then(left.index.cmp(&right.index))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Row {
        kind: Kind,
        label: &'static str,
        path: &'static str,
        prior: Prior,
    }

    fn file(path: &'static str, changed: bool, recent: f32) -> Row {
        let trimmed = path.trim_end_matches('/');
        let cut = trimmed.rfind('/').map_or(0, |at| at + 1);
        Row {
            kind: if path.ends_with('/') {
                Kind::Dir
            } else {
                Kind::File
            },
            label: &trimmed[cut..],
            path: &path[..cut],
            prior: Prior {
                changed,
                recent,
                depth: trimmed.matches('/').count() as u32,
                ..Prior::default()
            },
        }
    }

    fn place(kind: Kind, label: &'static str, path: &'static str, closeness: Closeness) -> Row {
        Row {
            kind,
            label,
            path,
            prior: Prior {
                closeness,
                depth: 3,
                ..Prior::default()
            },
        }
    }

    fn everything() -> Vec<Row> {
        vec![
            file("crates/zz-ui/src/target.rs", true, 0.9),
            file("crates/zz-ui/src/palette.rs", true, 0.6),
            file("docs/targets.md", false, 0.0),
            file("knowledge/targeting.md", false, 0.0),
            file("libs/penna/src/blocks/palette/", false, 0.0),
            file("libs/penna/src/blocks/palette/palette.css", false, 0.0),
            file("libs/penna/src/blocks/palette/mod.rs", false, 0.0),
            file("libs/penna/src/lib.rs", false, 0.0),
            file("crates/zz-term/Cargo.toml", false, 0.0),
            file("libs/penna/Cargo.toml", false, 0.0),
            file("logs/zz.log", false, 0.0),
            place(
                Kind::Place,
                "nvim crates/zz-ui/src/target.rs",
                "mbp › zebrilus › 1",
                Closeness::Window,
            ),
            place(
                Kind::Place,
                "nvim docs/pages/palette.md",
                "mbp › penna › 1",
                Closeness::Host,
            ),
            place(
                Kind::Place,
                "tail logs/zz.log",
                "mbp › zebrilus › 3",
                Closeness::Session,
            ),
            place(
                Kind::Chat,
                "Fix daemon reattach race",
                "codex · xps",
                Closeness::Elsewhere,
            ),
        ]
    }

    fn ranked(query: &str, rows: &[Row]) -> Vec<String> {
        let candidates = rows
            .iter()
            .map(|row| Candidate {
                kind: row.kind,
                label: row.label,
                path: row.path,
                prior: row.prior,
            })
            .collect::<Vec<_>>();
        rank(query, &candidates)
            .into_iter()
            .map(|hit| format!("{}{}", rows[hit.index].path, rows[hit.index].label))
            .collect()
    }

    #[test]
    fn targ_lists_the_file_then_its_pane_then_the_docs() {
        assert_eq!(
            ranked("targ", &everything()),
            [
                "crates/zz-ui/src/target.rs",
                "mbp › zebrilus › 1nvim crates/zz-ui/src/target.rs",
                "docs/targets.md",
                "knowledge/targeting.md",
            ]
        );
    }

    #[test]
    fn pal_skips_files_that_only_sit_under_a_palette_folder() {
        assert_eq!(
            ranked("pal", &everything()),
            [
                "crates/zz-ui/src/palette.rs",
                "mbp › penna › 1nvim docs/pages/palette.md",
                "libs/penna/src/blocks/palette",
                "libs/penna/src/blocks/palette/palette.css",
            ]
        );
    }

    #[test]
    fn a_run_cannot_start_mid_word() {
        assert!(
            !ranked("cra", &everything())
                .contains(&"codex · xpsFix daemon reattach race".to_owned())
        );
    }

    #[test]
    fn closeness_and_recency_break_equal_matches() {
        let near = |closeness, recent| Row {
            kind: Kind::Place,
            label: "zsh",
            path: "",
            prior: Prior {
                closeness,
                recent,
                ..Prior::default()
            },
        };
        let rows = [
            near(Closeness::Elsewhere, 0.0),
            near(Closeness::Window, 0.0),
            near(Closeness::Elsewhere, 0.5),
        ];
        let order = rank(
            "zsh",
            &rows
                .iter()
                .map(|row| Candidate {
                    kind: row.kind,
                    label: row.label,
                    path: row.path,
                    prior: row.prior,
                })
                .collect::<Vec<_>>(),
        )
        .into_iter()
        .map(|hit| hit.index)
        .collect::<Vec<_>>();
        assert_eq!(order, [1, 2, 0]);
    }
}
