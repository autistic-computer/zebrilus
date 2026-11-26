use super::*;

const REGEX_CACHE_ENTRIES: usize = 64;

type RegexCache = VecDeque<((String, bool), Option<Regex>)>;

static REGEX_CACHE: LazyLock<Mutex<RegexCache>> =
    LazyLock::new(|| Mutex::new(VecDeque::with_capacity(REGEX_CACHE_ENTRIES)));

fn cached_regex(pattern: &str, insensitive: bool) -> Option<Regex> {
    if let Some((_, regex)) = REGEX_CACHE
        .lock()
        .iter()
        .find(|((cached, flag), _)| *flag == insensitive && cached == pattern)
    {
        return regex.clone();
    }
    let regex = RegexBuilder::new(pattern)
        .case_insensitive(insensitive)
        .dot_matches_new_line(true)
        .build()
        .ok();
    let mut cache = REGEX_CACHE.lock();
    if cache.len() == REGEX_CACHE_ENTRIES {
        cache.pop_front();
    }
    cache.push_back(((pattern.to_owned(), insensitive), regex.clone()));
    regex
}

pub(super) fn match_value(pattern: &str, text: &str, flags: &str) -> String {
    if flags.contains('p') {
        return fuzzy_positions(pattern, text).unwrap_or_default();
    }
    if flags.contains('z') {
        return bool_string(fuzzy_positions(pattern, text).is_some()).to_owned();
    }
    let matched = if flags.contains('r') {
        cached_regex(pattern, flags.contains('i')).is_some_and(|regex| regex.is_match(text))
    } else {
        Pattern::new(pattern).is_ok_and(|pattern| {
            pattern.matches_with(
                text,
                MatchOptions {
                    case_sensitive: !flags.contains('i'),
                    require_literal_separator: false,
                    require_literal_leading_dot: false,
                },
            )
        })
    };
    bool_string(matched).to_owned()
}

#[derive(Clone, Copy)]
struct FuzzyCharacter {
    value: char,
    column: usize,
    width: usize,
    align: zz_protocol::TmuxAlign,
}

#[derive(Clone, Copy)]
struct FuzzyTerm<'a> {
    inverse: bool,
    exact: bool,
    prefix: bool,
    suffix: bool,
    text: &'a str,
}

fn fuzzy_positions(pattern: &str, text: &str) -> Option<String> {
    let characters = fuzzy_characters(text);
    if pattern
        .chars()
        .all(|character| matches!(character, ' ' | '|'))
    {
        return Some(String::new());
    }
    let (_, selected) = fuzzy_best(pattern, &characters)?;
    Some(
        characters
            .iter()
            .zip(selected)
            .filter(|(_, selected)| *selected)
            .flat_map(|(character, _)| character.column..character.column + character.width)
            .map(|column| column.to_string())
            .collect::<Vec<_>>()
            .join(","),
    )
}

fn fuzzy_best(pattern: &str, characters: &[FuzzyCharacter]) -> Option<(i32, Vec<bool>)> {
    let fold = !pattern.bytes().any(|byte| byte.is_ascii_uppercase());
    let mut best: Option<(i32, Vec<bool>)> = None;
    for group in pattern.split('|') {
        let mut any = false;
        let mut score = 0i32;
        let mut selected = vec![false; characters.len()];
        let mut matched_group = true;
        for raw in group.split(' ').filter(|term| !term.is_empty()) {
            let Some(term) = fuzzy_term(raw) else {
                matched_group = false;
                break;
            };
            any = true;
            let found = if term.exact {
                fuzzy_exact(&term, characters, fold)
            } else {
                fuzzy_subsequence(term.text, characters, fold)
            };
            if term.inverse {
                if found.is_some() {
                    matched_group = false;
                    break;
                }
                continue;
            }
            let Some((term_score, positions)) = found else {
                matched_group = false;
                break;
            };
            score = score.saturating_add(term_score);
            for position in positions {
                selected[position] = true;
            }
        }
        if !any || !matched_group {
            continue;
        }
        if best
            .as_ref()
            .is_none_or(|(best_score, _)| score > *best_score)
        {
            best = Some((score, selected));
        }
    }
    best
}

pub fn fuzzy_match_columns(pattern: &str, text: &str, width: usize) -> Option<(u32, Vec<usize>)> {
    use zz_protocol::TmuxAlign;
    if width == 0 {
        return None;
    }
    if pattern
        .chars()
        .all(|character| matches!(character, ' ' | '|'))
    {
        return Some((0, Vec::new()));
    }
    let characters = fuzzy_markup_characters(text);
    let (score, selected) = fuzzy_best(pattern, &characters)?;
    let total = |align: TmuxAlign| {
        characters
            .iter()
            .filter(|character| character.align == align)
            .map(|character| character.width)
            .sum::<usize>()
    };
    let (left, centre, right, absolute) = (
        total(TmuxAlign::Left),
        total(TmuxAlign::Centre),
        total(TmuxAlign::Right),
        total(TmuxAlign::AbsoluteCentre),
    );
    let (mut wl, mut wc, mut wr) = (left, centre, right);
    while wl + wc + wr > width {
        if wc > 0 {
            wc -= 1;
        } else if wr > 0 {
            wr -= 1;
        } else {
            wl -= 1;
        }
    }
    let wa = absolute.min(width);
    let area = |align: TmuxAlign| match align {
        TmuxAlign::Right => (width - wr, right - wr, wr),
        TmuxAlign::Centre => (
            (wl + (width - wr - wl) / 2).saturating_sub(wc / 2),
            (centre / 2).saturating_sub(wc / 2),
            wc,
        ),
        TmuxAlign::AbsoluteCentre => ((width - wa) / 2, 0, wa),
        TmuxAlign::Left | TmuxAlign::Default => (0, 0, wl),
    };
    let mut columns = Vec::new();
    for (character, _) in characters.iter().zip(selected).filter(|(_, hit)| *hit) {
        let (start, source, visible) = area(character.align);
        if character.column < source || character.column >= source + visible {
            continue;
        }
        let column = start + character.column - source;
        columns.extend((column..column + character.width).filter(|column| *column < width));
    }
    columns.sort_unstable();
    columns.dedup();
    Some((u32::try_from(score.max(0)).unwrap_or(0), columns))
}

fn fuzzy_characters(text: &str) -> Vec<FuzzyCharacter> {
    let mut column = 0usize;
    text.chars()
        .filter_map(|value| {
            if value.is_ascii_control() {
                return None;
            }
            let width = value.width().unwrap_or_default();
            let character = FuzzyCharacter {
                value,
                column,
                width,
                align: zz_protocol::TmuxAlign::Left,
            };
            column = column.saturating_add(width);
            Some(character)
        })
        .collect()
}

fn fuzzy_markup_characters(text: &str) -> Vec<FuzzyCharacter> {
    use zz_protocol::TmuxAlign;
    let mut characters = Vec::new();
    let mut offsets = [0usize; 5];
    let mut align = TmuxAlign::Left;
    let slot = |align: TmuxAlign| match align {
        TmuxAlign::Default | TmuxAlign::Left => 0,
        TmuxAlign::Centre => 1,
        TmuxAlign::Right => 2,
        TmuxAlign::AbsoluteCentre => 3,
    };
    let mut push = |value: char, align: TmuxAlign, characters: &mut Vec<FuzzyCharacter>| {
        let width = value.width().unwrap_or_default();
        let offset = &mut offsets[slot(align)];
        characters.push(FuzzyCharacter {
            value,
            column: *offset,
            width,
            align,
        });
        *offset += width;
    };
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'#' {
            let hashes = bytes[index..]
                .iter()
                .take_while(|byte| **byte == b'#')
                .count();
            if bytes.get(index + hashes) != Some(&b'[') {
                for _ in 0..hashes.div_ceil(2) {
                    push('#', align, &mut characters);
                }
                index += hashes;
                continue;
            }
            for _ in 0..hashes / 2 {
                push('#', align, &mut characters);
            }
            if hashes % 2 == 0 {
                push('[', align, &mut characters);
                index += hashes + 1;
                continue;
            }
            let start = index + hashes + 1;
            let Some(end) = text[start..].find(']') else {
                break;
            };
            if let Some(style) = zz_protocol::parse_style(&text[start..start + end])
                && let Some(next) = style.align
            {
                align = if next == TmuxAlign::Default {
                    TmuxAlign::Left
                } else {
                    next
                };
            }
            index = start + end + 1;
            continue;
        }
        let value = text[index..].chars().next().expect("character boundary");
        index += value.len_utf8();
        if !value.is_ascii_control() {
            push(value, align, &mut characters);
        }
    }
    characters
}

fn fuzzy_term(raw: &str) -> Option<FuzzyTerm<'_>> {
    let mut text = raw;
    let inverse = text.starts_with('!');
    if inverse {
        text = &text[1..];
    }
    if text.is_empty() {
        return None;
    }
    let mut exact = false;
    let mut prefix = false;
    if text.starts_with('\'') {
        exact = true;
        text = &text[1..];
    } else if text.starts_with('^') {
        exact = true;
        prefix = true;
        text = &text[1..];
    }
    if text.is_empty() {
        return None;
    }
    let suffix = text.ends_with('$');
    if suffix {
        exact = true;
        text = &text[..text.len() - 1];
    }
    if text.is_empty() {
        return None;
    }
    Some(FuzzyTerm {
        inverse,
        exact: exact || inverse,
        prefix,
        suffix,
        text,
    })
}

fn fuzzy_subsequence(
    pattern: &str,
    text: &[FuzzyCharacter],
    fold: bool,
) -> Option<(i32, Vec<usize>)> {
    let pattern = pattern.chars().collect::<Vec<_>>();
    if pattern.is_empty() || text.is_empty() {
        return None;
    }
    let mut positions = Vec::with_capacity(pattern.len());
    let mut text_index = 0usize;
    for wanted in &pattern {
        while text_index < text.len() && !fuzzy_equal(*wanted, text[text_index].value, fold) {
            text_index += 1;
        }
        if text_index == text.len() {
            return None;
        }
        positions.push(text_index);
        text_index += 1;
    }
    let mut candidate = *positions.last()?;
    for pattern_index in (0..pattern.len()).rev() {
        loop {
            if fuzzy_equal(pattern[pattern_index], text[candidate].value, fold) {
                positions[pattern_index] = candidate;
                break;
            }
            candidate = candidate.checked_sub(1)?;
        }
        if pattern_index != 0 {
            candidate = candidate.checked_sub(1)?;
        }
    }
    let score = fuzzy_subsequence_score(&positions, text);
    Some((score, positions))
}

fn fuzzy_exact(
    term: &FuzzyTerm<'_>,
    text: &[FuzzyCharacter],
    fold: bool,
) -> Option<(i32, Vec<usize>)> {
    let pattern = term.text.chars().collect::<Vec<_>>();
    if pattern.is_empty() || pattern.len() > text.len() {
        return None;
    }
    let starts = if term.prefix && term.suffix {
        if pattern.len() != text.len() {
            return None;
        }
        0..1
    } else if term.prefix {
        0..1
    } else if term.suffix {
        let start = text.len() - pattern.len();
        start..start + 1
    } else {
        0..text.len() - pattern.len() + 1
    };
    let mut best: Option<(i32, Vec<usize>)> = None;
    for start in starts {
        if !pattern
            .iter()
            .enumerate()
            .all(|(offset, wanted)| fuzzy_equal(*wanted, text[start + offset].value, fold))
        {
            continue;
        }
        let score = fuzzy_exact_score(start, pattern.len(), text, term.prefix, term.suffix);
        if best
            .as_ref()
            .is_none_or(|(best_score, _)| score > *best_score)
        {
            best = Some((score, (start..start + pattern.len()).collect()));
        }
    }
    best
}

fn fuzzy_subsequence_score(positions: &[usize], text: &[FuzzyCharacter]) -> i32 {
    let Some(first) = positions.first().copied() else {
        return 0;
    };
    let mut score = 0i32;
    if first == 0 {
        score += 12;
    } else {
        if fuzzy_boundary(text[first - 1].value) {
            score += 8;
        }
        score -= fuzzy_score_number(first.min(10));
    }
    for pair in positions.windows(2) {
        if pair[1] == pair[0] + 1 {
            score += 6;
        } else if fuzzy_boundary(text[pair[1] - 1].value) {
            score += 8;
        }
    }
    let span = positions[positions.len() - 1] - first + 1;
    score.saturating_sub(fuzzy_score_number(span - positions.len()))
}

fn fuzzy_exact_score(
    start: usize,
    length: usize,
    text: &[FuzzyCharacter],
    prefix: bool,
    suffix: bool,
) -> i32 {
    let mut score = 1_000i32.saturating_add(fuzzy_score_number(length).saturating_mul(6));
    if prefix {
        score = score.saturating_add(200);
    }
    if suffix {
        score = score.saturating_add(100);
    }
    if start == 0 {
        score = score.saturating_add(12);
    } else if fuzzy_boundary(text[start - 1].value) {
        score = score.saturating_add(8);
    }
    score = score.saturating_sub(fuzzy_score_number(start.min(10)));
    if !prefix && !suffix {
        score = score.saturating_sub(fuzzy_score_number(text.len() - (start + length)));
    }
    score
}

fn fuzzy_equal(left: char, right: char, fold: bool) -> bool {
    if fold && left.is_ascii() && right.is_ascii() {
        left.eq_ignore_ascii_case(&right)
    } else {
        left == right
    }
}

fn fuzzy_boundary(character: char) -> bool {
    character.is_ascii() && " -_/.:".contains(character)
}

fn fuzzy_score_number(value: usize) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

pub(super) fn substitute(
    text: &str,
    pattern: &str,
    replacement: &str,
    insensitive: bool,
) -> String {
    if text.is_empty() || pattern.is_empty() {
        return text.to_owned();
    }
    let Some(regex) = cached_regex(pattern, insensitive) else {
        return text.to_owned();
    };
    let mut output = String::with_capacity(text.len());
    let mut start = 0usize;
    let mut last = 0usize;
    let mut empty = false;
    while start <= text.len() {
        let Some(captures) = regex.captures(&text[start..]) else {
            output.push_str(&text[start..]);
            break;
        };
        let matched = captures.get(0).expect("a regex capture has a whole match");
        let match_start = start + matched.start();
        let match_end = start + matched.end();
        output.push_str(&text[last..match_start]);
        if pattern.starts_with('^') {
            output.push_str(&expand_substitution(replacement, &captures));
            output.push_str(&text[match_end..]);
            break;
        }
        if empty || match_start != last || !matched.is_empty() {
            output.push_str(&expand_substitution(replacement, &captures));
            last = match_end;
            start = match_end;
            empty = false;
        } else {
            last = match_end;
            let Some(character) = text[match_end..].chars().next() else {
                break;
            };
            start = match_end + character.len_utf8();
            empty = true;
        }
    }
    output
}

fn expand_substitution(replacement: &str, captures: &Captures<'_>) -> String {
    let mut output = String::with_capacity(replacement.len());
    let mut characters = replacement.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            output.push(character);
            continue;
        }
        let Some(next) = characters.next() else {
            output.push('\\');
            break;
        };
        if let Some(index) = next.to_digit(10).map(|index| index as usize)
            && let Some(value) = captures.get(index)
            && !value.as_str().is_empty()
        {
            output.push_str(value.as_str());
        } else {
            output.push(next);
        }
    }
    output
}
