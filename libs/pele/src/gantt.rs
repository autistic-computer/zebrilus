use crate::{LinkInfo, PeleError, RenderOptions, RenderResult, svg, text};
use indexmap::IndexMap;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use std::sync::LazyLock;

const SECOND: f64 = 1000.0;
const MINUTE: f64 = 60.0 * SECOND;
const HOUR: f64 = 60.0 * MINUTE;
const DAY: f64 = 24.0 * HOUR;
const WEEK: f64 = 7.0 * DAY;
const MONTH: f64 = 30.0 * DAY;
const YEAR: f64 = 365.0 * DAY;

pub const NOW: f64 = 0.0;
pub const MAX_TASKS: usize = 20_000;

const WORK_LIMIT: f64 = 4e8;
const DAY_STEP: f64 = 400.0;
const DATED_STEP: f64 = 200.0;
const FORMAT_STEP: f64 = 400.0;

const WIDTH: f64 = 800.0;
const MIN_PLOT: f64 = 240.0;
const LABEL_GAP: f64 = 8.0;

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
const DAY_NAMES: [&str; 7] = [
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
];
const WEEK_STARTS: [&str; 7] = [
    "sunday",
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
];

static BREAK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)<br[\t-\r \u{a0}\u{1680}\u{2000}-\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}\u{feff}]*/?>")
        .expect("valid break pattern")
});

fn is_space(ch: char) -> bool {
    matches!(
        ch,
        '\t'..='\r'
            | ' '
            | '\u{a0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200a}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202f}'
            | '\u{205f}'
            | '\u{3000}'
            | '\u{feff}'
    )
}

fn js_trim(value: &str) -> &str {
    value.trim_matches(is_space)
}

fn js_round(value: f64) -> f64 {
    let floor = value.floor();
    if value - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    }
}

fn js_string(value: f64) -> String {
    if value.is_nan() {
        "NaN".into()
    } else if value.is_infinite() {
        if value > 0.0 { "Infinity" } else { "-Infinity" }.into()
    } else if value == 0.0 {
        "0".into()
    } else if value.fract() == 0.0 && value.abs() < 1e21 {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}

fn js_number(value: &str) -> f64 {
    let value = js_trim(value);
    if value.is_empty() {
        return 0.0;
    }
    for (prefix, radix) in [
        ("0x", 16),
        ("0X", 16),
        ("0o", 8),
        ("0O", 8),
        ("0b", 2),
        ("0B", 2),
    ] {
        if let Some(digits) = value.strip_prefix(prefix) {
            if digits.is_empty() || !digits.chars().all(|ch| ch.is_digit(radix)) {
                return f64::NAN;
            }
            return digits.chars().fold(0.0, |total, ch| {
                total * f64::from(radix) + f64::from(ch.to_digit(radix).unwrap_or(0))
            });
        }
    }
    let unsigned = value.strip_prefix(['+', '-']).unwrap_or(value);
    if unsigned == "Infinity" {
        return if value.starts_with('-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    let bytes = unsigned.as_bytes();
    let mut at = 0;
    let digits = |at: &mut usize| {
        let start = *at;
        while bytes.get(*at).is_some_and(u8::is_ascii_digit) {
            *at += 1;
        }
        *at - start
    };
    let mut count = digits(&mut at);
    if bytes.get(at) == Some(&b'.') {
        at += 1;
        count += digits(&mut at);
    }
    if count == 0 {
        return f64::NAN;
    }
    if matches!(bytes.get(at), Some(b'e' | b'E')) {
        at += 1;
        if matches!(bytes.get(at), Some(b'+' | b'-')) {
            at += 1;
        }
        if digits(&mut at) == 0 {
            return f64::NAN;
        }
    }
    if at != bytes.len() {
        return f64::NAN;
    }
    value.parse::<f64>().unwrap_or(f64::NAN)
}

fn time_clip(time: f64) -> f64 {
    if time.is_finite() && time.abs() <= 8.64e15 {
        time.trunc() + 0.0
    } else {
        f64::NAN
    }
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let shifted = (month + 9) % 12;
    let day_of_year = (153 * shifted + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month + 2) / 5 + 1;
    let month = if month < 10 { month + 3 } else { month - 9 };
    (year_of_era + era * 400 + i64::from(month <= 2), month, day)
}

fn make_day(year: f64, month: f64, date: f64) -> f64 {
    if !(year.is_finite() && month.is_finite() && date.is_finite()) {
        return f64::NAN;
    }
    let (year, month) = (year.trunc(), month.trunc());
    if !(-1e6..=1e6).contains(&year) || !(-1e7..=1e7).contains(&month) {
        return f64::NAN;
    }
    let month = month as i64;
    let year = year as i64 + month.div_euclid(12);
    days_from_civil(year, month.rem_euclid(12) + 1, 1) as f64 + date.trunc() - 1.0
}

fn make_time(hours: f64, minutes: f64, seconds: f64, ms: f64) -> f64 {
    if !(hours.is_finite() && minutes.is_finite() && seconds.is_finite() && ms.is_finite()) {
        return f64::NAN;
    }
    hours.trunc() * HOUR + minutes.trunc() * MINUTE + seconds.trunc() * SECOND + ms.trunc()
}

fn compose(
    year: f64,
    month: f64,
    date: f64,
    hours: f64,
    minutes: f64,
    seconds: f64,
    ms: f64,
) -> f64 {
    time_clip(make_day(year, month, date) * DAY + make_time(hours, minutes, seconds, ms))
}

fn local_date(
    year: f64,
    month: f64,
    date: f64,
    hours: f64,
    minutes: f64,
    seconds: f64,
    ms: f64,
) -> f64 {
    let whole = year.trunc();
    let year = if (0.0..=99.0).contains(&whole) {
        1900.0 + whole
    } else {
        year
    };
    compose(year, month, date, hours, minutes, seconds, ms)
}

#[derive(Clone, Copy)]
struct Parts {
    year: f64,
    month: f64,
    date: f64,
    hours: f64,
    minutes: f64,
    seconds: f64,
    ms: f64,
    weekday: f64,
}

fn parts(time: f64) -> Parts {
    if time.is_nan() {
        return Parts {
            year: f64::NAN,
            month: f64::NAN,
            date: f64::NAN,
            hours: f64::NAN,
            minutes: f64::NAN,
            seconds: f64::NAN,
            ms: f64::NAN,
            weekday: f64::NAN,
        };
    }
    let day = (time / DAY).floor();
    let within = time - day * DAY;
    let (year, month, date) = civil_from_days(day as i64);
    Parts {
        year: year as f64,
        month: (month - 1) as f64,
        date: date as f64,
        hours: (within / HOUR).floor(),
        minutes: ((within % HOUR) / MINUTE).floor(),
        seconds: ((within % MINUTE) / SECOND).floor(),
        ms: within % SECOND,
        weekday: (day as i64 + 4).rem_euclid(7) as f64,
    }
}

fn set_date(time: f64, date: f64) -> f64 {
    let p = parts(time);
    compose(p.year, p.month, date, p.hours, p.minutes, p.seconds, p.ms)
}

fn set_month(time: f64, month: f64, date: Option<f64>) -> f64 {
    let p = parts(time);
    compose(
        p.year,
        month,
        date.unwrap_or(p.date),
        p.hours,
        p.minutes,
        p.seconds,
        p.ms,
    )
}

fn set_full_year(time: f64, year: f64) -> f64 {
    let p = parts(if time.is_nan() { 0.0 } else { time });
    compose(year, p.month, p.date, p.hours, p.minutes, p.seconds, p.ms)
}

fn set_hours(time: f64, hours: f64, minutes: f64, seconds: f64, ms: f64) -> f64 {
    let p = parts(time);
    compose(p.year, p.month, p.date, hours, minutes, seconds, ms)
}

fn start_of_day(time: f64) -> f64 {
    set_hours(time, 0.0, 0.0, 0.0, 0.0)
}

fn add_days(time: f64, days: f64) -> f64 {
    set_date(time, parts(time).date + days)
}

fn pad(value: f64, length: usize) -> String {
    let digits = js_string(value);
    let count = digits.encode_utf16().count();
    if count >= length {
        digits
    } else {
        "0".repeat(length - count) + &digits
    }
}

fn ordinal(value: f64) -> String {
    let suffixes = ["th", "st", "nd", "rd"];
    let rest = value % 100.0;
    let tens = (rest - 20.0) % 10.0;
    let suffix = if (0.0..=3.0).contains(&tens) {
        suffixes[tens as usize]
    } else if (0.0..=3.0).contains(&rest) && rest.fract() == 0.0 {
        suffixes[rest as usize]
    } else {
        "th"
    };
    format!("{}{suffix}", js_string(value))
}

fn iso_weekday(time: f64) -> f64 {
    let weekday = parts(time).weekday;
    if weekday == 0.0 { 7.0 } else { weekday }
}

fn week_thursday(time: f64) -> f64 {
    add_days(time, 4.0 - iso_weekday(time))
}

fn iso_week(time: f64) -> f64 {
    let thursday = week_thursday(time);
    let first_day = local_date(parts(thursday).year, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0);
    let weekday = iso_weekday(first_day);
    let first = add_days(
        first_day,
        4.0 - weekday + if weekday > 4.0 { 7.0 } else { 0.0 },
    );
    let weeks = (thursday - first) / WEEK;
    (if weeks < 0.0 {
        weeks.ceil() + 0.0
    } else {
        weeks.floor()
    }) + 1.0
}

fn month_name(month: f64) -> &'static str {
    MONTHS.get(month as usize).copied().unwrap_or("")
}

fn weekday_name(weekday: f64) -> &'static str {
    WEEKDAYS.get(weekday as usize).copied().unwrap_or("")
}

fn prefix(value: &str, count: usize) -> &str {
    value.get(..count).unwrap_or(value)
}

fn iso_date(time: f64) -> String {
    let p = parts(time);
    let year = if (1000.0..=9999.0).contains(&p.year) {
        js_string(p.year)
    } else {
        pad(p.year, 4)
    };
    format!(
        "{year}{}{}{}{}",
        if p.month + 1.0 < 10.0 { "-0" } else { "-" },
        js_string(p.month + 1.0),
        if p.date < 10.0 { "-0" } else { "-" },
        js_string(p.date)
    )
}

fn bracket_literal(chars: &[char], at: usize) -> Option<usize> {
    if chars.get(at) != Some(&'[') {
        return None;
    }
    let close = chars[at + 1..].iter().position(|ch| *ch == ']')? + at + 1;
    (close > at + 1).then_some(close + 1)
}

fn starts_with(chars: &[char], at: usize, token: &str) -> bool {
    (at..)
        .zip(token.chars())
        .all(|(index, ch)| chars.get(index) == Some(&ch))
}

fn run(chars: &[char], at: usize, ch: char, max: usize) -> usize {
    chars[at..]
        .iter()
        .take(max)
        .take_while(|value| **value == ch)
        .count()
}

const EARLY_TOKENS: [&str; 16] = [
    "Q", "wo", "ww", "w", "WW", "W", "zzz", "z", "gggg", "GGGG", "Do", "X", "x", "kk", "k", "S",
];

#[derive(Clone, Debug)]
enum Piece {
    Text(String),
    Early(&'static str),
    Late(&'static str),
}

#[derive(Clone, Debug)]
enum DateFormat {
    Iso,
    Unsupported,
    Ordinal(Vec<char>),
    Pieces(Vec<Piece>),
}

fn early_token(chars: &[char], at: usize) -> Option<&'static str> {
    EARLY_TOKENS
        .into_iter()
        .find(|token| starts_with(chars, at, token))
}

fn late_token(chars: &[char], at: usize) -> Option<(&'static str, usize)> {
    for token in ["YYYY", "YY", "SSS"] {
        if starts_with(chars, at, token) {
            return Some((token, token.len()));
        }
    }
    let names: &[&'static str] = match chars[at] {
        'M' => &["M", "MM", "MMM", "MMMM"],
        'D' => &["D", "DD"],
        'd' => &["d", "dd", "ddd", "dddd"],
        'H' => &["H", "HH"],
        'h' => &["h", "hh"],
        'a' => &["a"],
        'A' => &["A"],
        'm' => &["m", "mm"],
        's' => &["s", "ss"],
        'Z' => &["Z", "ZZ"],
        _ => return None,
    };
    let length = run(chars, at, chars[at], names.len());
    Some((names[length - 1], length))
}

fn push_text(pieces: &mut Vec<Piece>, text: &[char]) {
    if let Some(Piece::Text(last)) = pieces.last_mut() {
        last.extend(text);
    } else {
        pieces.push(Piece::Text(text.iter().collect()));
    }
}

fn late_pieces(chars: &[char], pieces: &mut Vec<Piece>) {
    let mut at = 0;
    while at < chars.len() {
        if let Some(end) = bracket_literal(chars, at) {
            push_text(pieces, &chars[at + 1..end - 1]);
            at = end;
        } else if let Some((token, length)) = late_token(chars, at) {
            pieces.push(Piece::Late(token));
            at += length;
        } else {
            push_text(pieces, &chars[at..=at]);
            at += 1;
        }
    }
}

fn early_value(token: &str, time: f64, p: &Parts) -> String {
    let hours = if p.hours == 0.0 { 24.0 } else { p.hours };
    match token {
        "Q" => js_string(((p.month + 1.0) / 3.0).ceil()),
        "Do" => format!("[{}]", ordinal(p.date)),
        "GGGG" => js_string(parts(week_thursday(time)).year),
        "W" => js_string(iso_week(time)),
        "WW" => pad(iso_week(time), 2),
        "k" => js_string(hours),
        "kk" => pad(hours, 2),
        "X" => js_string((time / 1000.0).floor()),
        "x" => js_string(time),
        _ => token.to_owned(),
    }
}

fn late_value(token: &str, p: &Parts) -> String {
    let twelve = if p.hours % 12.0 == 0.0 {
        12.0
    } else {
        p.hours % 12.0
    };
    match token {
        "YYYY" => pad(p.year, 4),
        "YY" => {
            let year: Vec<char> = js_string(p.year).chars().collect();
            year[year.len().saturating_sub(2)..].iter().collect()
        }
        "SSS" => pad(p.ms, 3),
        "M" => js_string(p.month + 1.0),
        "MM" => pad(p.month + 1.0, 2),
        "MMM" => prefix(month_name(p.month), 3).to_owned(),
        "MMMM" => month_name(p.month).to_owned(),
        "D" => js_string(p.date),
        "DD" => pad(p.date, 2),
        "d" => js_string(p.weekday),
        "dd" => prefix(weekday_name(p.weekday), 2).to_owned(),
        "ddd" => prefix(weekday_name(p.weekday), 3).to_owned(),
        "dddd" => weekday_name(p.weekday).to_owned(),
        "H" => js_string(p.hours),
        "HH" => pad(p.hours, 2),
        "h" => js_string(twelve),
        "hh" => pad(twelve, 2),
        "a" => if p.hours < 12.0 { "am" } else { "pm" }.to_owned(),
        "A" => if p.hours < 12.0 { "AM" } else { "PM" }.to_owned(),
        "m" => js_string(p.minutes),
        "mm" => pad(p.minutes, 2),
        "s" => js_string(p.seconds),
        "ss" => pad(p.seconds, 2),
        "Z" => "+00:00".to_owned(),
        _ => "+0000".to_owned(),
    }
}

impl DateFormat {
    fn new(template: &str) -> Self {
        let template = if template.is_empty() {
            "YYYY-MM-DDTHH:mm:ssZ"
        } else {
            template
        };
        if template == "YYYY-MM-DD" {
            return Self::Iso;
        }
        let chars: Vec<char> = template.chars().collect();
        let mut pieces = Vec::new();
        let mut text = Vec::new();
        let mut at = 0;
        while at < chars.len() {
            if let Some(end) = bracket_literal(&chars, at) {
                text.extend_from_slice(&chars[at..end]);
                at = end;
                continue;
            }
            match early_token(&chars, at) {
                Some("gggg" | "wo" | "w" | "ww" | "z" | "zzz") => return Self::Unsupported,
                Some("Do") => return Self::Ordinal(chars),
                Some("S") | None => {
                    text.push(chars[at]);
                    at += 1;
                }
                Some(token) => {
                    late_pieces(&text, &mut pieces);
                    text.clear();
                    pieces.push(Piece::Early(token));
                    at += token.len();
                }
            }
        }
        late_pieces(&text, &mut pieces);
        Self::Pieces(pieces)
    }

    fn format(&self, time: f64) -> Option<String> {
        if time.is_nan() {
            return Some("Invalid Date".into());
        }
        let p = parts(time);
        let owned;
        let pieces = match self {
            Self::Iso => return Some(iso_date(time)),
            Self::Unsupported => return None,
            Self::Pieces(pieces) => pieces,
            Self::Ordinal(chars) => {
                let mut advanced = Vec::new();
                let mut at = 0;
                while at < chars.len() {
                    if let Some(end) = bracket_literal(chars, at) {
                        advanced.extend_from_slice(&chars[at..end]);
                        at = end;
                    } else if let Some(token) = early_token(chars, at) {
                        if matches!(token, "gggg" | "wo" | "w" | "ww" | "z" | "zzz") {
                            return None;
                        }
                        advanced.extend(early_value(token, time, &p).chars());
                        at += token.len();
                    } else {
                        advanced.push(chars[at]);
                        at += 1;
                    }
                }
                let mut pieces = Vec::new();
                late_pieces(&advanced, &mut pieces);
                owned = pieces;
                &owned
            }
        };
        let mut output = String::new();
        for piece in pieces {
            match piece {
                Piece::Text(text) => output.push_str(text),
                Piece::Early(token) => output.push_str(&early_value(token, time, &p)),
                Piece::Late(token) => output.push_str(&late_value(token, &p)),
            }
        }
        Some(output)
    }
}

pub fn format_date(time: f64, template: &str) -> Option<String> {
    DateFormat::new(template).format(time)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Millisecond,
    Second,
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Year,
}

fn set_calendar(time: f64, apply: impl Fn(f64) -> f64) -> f64 {
    let next = apply(set_date(time, 1.0));
    let last = start_of_day(local_date(
        parts(next).year,
        parts(next).month + 1.0,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
    ));
    set_date(next, parts(time).date.min(parts(last).date))
}

pub fn add(time: f64, amount: f64, unit: Unit) -> f64 {
    let p = parts(time);
    match unit {
        Unit::Month => set_calendar(time, |next| set_month(next, p.month + amount, None)),
        Unit::Year => set_calendar(time, |next| set_full_year(next, p.year + amount)),
        Unit::Day => add_days(time, js_round(amount)),
        Unit::Week => add_days(time, js_round(7.0 * amount)),
        Unit::Hour => time_clip(time + amount * HOUR),
        Unit::Minute => time_clip(time + amount * MINUTE),
        Unit::Second => time_clip(time + amount * SECOND),
        Unit::Millisecond => time_clip(time + amount),
    }
}

pub fn parse_duration(value: &str) -> Option<(f64, Unit)> {
    let value = js_trim(value);
    let bytes = value.as_bytes();
    let mut at = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
    if at == 0 {
        return None;
    }
    if bytes.get(at) == Some(&b'.') {
        let fraction = bytes[at + 1..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count();
        if fraction > 0 {
            at += 1 + fraction;
        }
    }
    let unit = match &value[at..] {
        "ms" => Unit::Millisecond,
        "s" => Unit::Second,
        "m" => Unit::Minute,
        "h" => Unit::Hour,
        "d" => Unit::Day,
        "w" => Unit::Week,
        "M" => Unit::Month,
        "y" => Unit::Year,
        _ => return None,
    };
    Some((value[..at].parse().ok()?, unit))
}

#[derive(Clone, Debug)]
enum Step {
    Literal(usize),
    Token(&'static str),
}

const PARSE_TOKENS: [&str; 29] = [
    "A", "a", "Q", "YYYY", "YY", "Y", "ww", "w", "MMMM", "MMM", "MM", "M", "Do", "DD", "D", "hh",
    "h", "HH", "H", "mm", "m", "ss", "s", "SSS", "SS", "S", "z", "ZZ", "Z",
];

fn compile(template: &str) -> Option<Vec<Step>> {
    let chars: Vec<char> = template.chars().collect();
    let mut plain = String::new();
    let mut at = 0;
    while at < chars.len() {
        if let Some(end) = bracket_literal(&chars, at) {
            at = end;
        } else {
            plain.push(chars[at]);
            at += 1;
        }
    }
    if plain.contains(['L', 'l']) {
        return None;
    }
    let mut steps = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        if chars[at] == '[' {
            let open = chars[at + 1..]
                .iter()
                .position(|ch| *ch == '[')
                .map_or(chars.len(), |index| at + 1 + index);
            if let Some(close) = chars[at + 1..open].iter().rposition(|ch| *ch == ']') {
                let close = at + 1 + close;
                let inner: String = chars[at + 1..close].iter().collect();
                steps.push(Step::Literal(inner.chars().count()));
                at = close + 1;
                continue;
            }
        }
        let separators = chars[at..]
            .iter()
            .take_while(|ch| {
                matches!(ch, '-' | '_' | ':' | '/' | '.' | ',' | '(' | ')') || is_space(**ch)
            })
            .count();
        if separators > 0 {
            steps.push(Step::Literal(separators));
            at += separators;
            continue;
        }
        if let Some(token) = PARSE_TOKENS
            .into_iter()
            .find(|token| starts_with(&chars, at, token))
        {
            steps.push(if token == "z" {
                Step::Literal(1)
            } else {
                Step::Token(token)
            });
            at += token.len();
            continue;
        }
        at += 1;
    }
    (!steps.is_empty()).then_some(steps)
}

#[derive(Clone, Copy)]
enum Pattern {
    One,
    Two,
    Three,
    Four,
    OneToTwo,
    Signed,
    Offset,
    Word,
}

fn pattern(token: &str) -> Pattern {
    match token {
        "A" | "a" | "Do" | "MMM" | "MMMM" => Pattern::Word,
        "Q" | "S" => Pattern::One,
        "SS" | "DD" | "ww" | "MM" | "YY" => Pattern::Two,
        "SSS" => Pattern::Three,
        "YYYY" => Pattern::Four,
        "Y" => Pattern::Signed,
        "Z" | "ZZ" => Pattern::Offset,
        _ => Pattern::OneToTwo,
    }
}

fn word_char(ch: char) -> bool {
    !ch.is_ascii_digit() && !matches!(ch, '-' | '_' | ':' | '/' | ',' | '(' | ')') && !is_space(ch)
}

fn digit_run(chars: &[char], from: usize) -> usize {
    chars.get(from..).map_or(0, |rest| {
        rest.iter().take_while(|ch| ch.is_ascii_digit()).count()
    })
}

fn match_at(pattern: Pattern, chars: &[char]) -> Option<usize> {
    let digits = digit_run(chars, 0);
    match pattern {
        Pattern::One => (digits >= 1).then_some(1),
        Pattern::Two => (digits >= 2).then_some(2),
        Pattern::Three => (digits >= 3).then_some(3),
        Pattern::Four => (digits >= 4).then_some(4),
        Pattern::OneToTwo => (digits >= 1).then_some(digits.min(2)),
        Pattern::Signed => {
            let sign = usize::from(matches!(chars.first(), Some('+' | '-')));
            let count = digit_run(chars, sign);
            (count > 0).then_some(sign + count)
        }
        Pattern::Offset => {
            if matches!(chars.first(), Some('+' | '-')) && digit_run(chars, 1) >= 2 {
                let mut length = 3;
                if chars.get(length) == Some(&':') {
                    length += 1;
                }
                if digit_run(chars, length) >= 2 {
                    length += 2;
                }
                Some(length)
            } else {
                (chars.first() == Some(&'Z')).then_some(1)
            }
        }
        Pattern::Word => None,
    }
}

fn search(pattern: Pattern, chars: &[char]) -> Option<(usize, usize)> {
    if let Pattern::Word = pattern {
        let mut digits = None;
        for (index, ch) in chars.iter().enumerate() {
            if ch.is_ascii_digit() {
                digits.get_or_insert(index);
            } else if word_char(*ch) {
                let start = digits.unwrap_or(index);
                let end = index
                    + chars[index..]
                        .iter()
                        .take_while(|ch| word_char(**ch))
                        .count();
                return Some((start, end - start));
            } else {
                digits = None;
            }
        }
        return None;
    }
    (0..chars.len())
        .find_map(|start| match_at(pattern, &chars[start..]).map(|length| (start, length)))
}

#[derive(Default)]
struct Fields {
    year: Option<f64>,
    month: Option<f64>,
    day: Option<f64>,
    day_given: bool,
    hours: Option<f64>,
    minutes: Option<f64>,
    seconds: Option<f64>,
    milliseconds: Option<f64>,
    week: Option<f64>,
    offset: Option<f64>,
    afternoon: Option<bool>,
}

fn truthy(value: Option<f64>) -> bool {
    value.is_some_and(|value| value != 0.0 && !value.is_nan())
}

fn or_zero(value: Option<f64>) -> f64 {
    if truthy(value) {
        value.unwrap_or(0.0)
    } else {
        0.0
    }
}

fn read(token: &str, fields: &mut Fields, input: &str) -> bool {
    let number = js_number(input);
    match token {
        "A" => fields.afternoon = Some(input == "PM"),
        "a" => fields.afternoon = Some(input == "pm"),
        "Q" => fields.month = Some((number - 1.0) * 3.0 + 1.0),
        "S" => fields.milliseconds = Some(number * 100.0),
        "SS" => fields.milliseconds = Some(number * 10.0),
        "SSS" => fields.milliseconds = Some(number),
        "s" | "ss" => fields.seconds = Some(number),
        "m" | "mm" => fields.minutes = Some(number),
        "H" | "h" | "HH" | "hh" => fields.hours = Some(number),
        "D" | "DD" => fields.day = Some(number),
        "w" | "ww" => fields.week = Some(number),
        "M" | "MM" => fields.month = Some(number),
        "Y" | "YYYY" => fields.year = Some(number),
        "YY" => fields.year = Some(number + if number > 68.0 { 1900.0 } else { 2000.0 }),
        "Do" => {
            let digits: String = input
                .chars()
                .skip_while(|ch| !ch.is_ascii_digit())
                .take_while(char::is_ascii_digit)
                .collect();
            if digits.is_empty() {
                return false;
            }
            fields.day = Some(js_number(&digits));
            fields.day_given = true;
            for day in 1..=31 {
                if ordinal(f64::from(day)) == input {
                    fields.day = Some(f64::from(day));
                }
            }
        }
        "MMM" | "MMMM" => {
            let Some(index) = MONTHS
                .iter()
                .position(|name| if token == "MMM" { prefix(name, 3) } else { name } == input)
            else {
                return false;
            };
            fields.month = Some((index + 1) as f64);
        }
        _ => {
            if input == "Z" {
                fields.offset = Some(0.0);
                return true;
            }
            let mut pieces = Vec::new();
            let chars: Vec<char> = input.chars().collect();
            let mut at = 0;
            while at < chars.len() {
                if matches!(chars[at], '+' | '-') {
                    pieces.push(chars[at].to_string());
                    at += 1;
                } else if digit_run(&chars, at) >= 2 {
                    pieces.push(chars[at..at + 2].iter().collect::<String>());
                    at += 2;
                } else {
                    at += 1;
                }
            }
            let piece = |index: usize| pieces.get(index).map_or(f64::NAN, |value| js_number(value));
            let minutes = piece(1) * 60.0 + {
                let extra = piece(2);
                if extra.is_nan() || extra == 0.0 {
                    0.0
                } else {
                    extra
                }
            };
            fields.offset = Some(if minutes == 0.0 {
                0.0
            } else if pieces.first().is_some_and(|sign| sign == "+") {
                -minutes
            } else {
                minutes
            });
        }
    }
    true
}

fn find_chars(haystack: &[char], needle: &[char]) -> Option<usize> {
    let mut fallback = vec![0; needle.len()];
    let mut matched = 0;
    for index in 1..needle.len() {
        while matched > 0 && needle[index] != needle[matched] {
            matched = fallback[matched - 1];
        }
        if needle[index] == needle[matched] {
            matched += 1;
        }
        fallback[index] = matched;
    }
    matched = 0;
    for (index, ch) in haystack.iter().enumerate() {
        while matched > 0 && *ch != needle[matched] {
            matched = fallback[matched - 1];
        }
        if *ch == needle[matched] {
            matched += 1;
        }
        if matched == needle.len() {
            return Some(index + 1 - matched);
        }
    }
    None
}

fn loose(input: &str, steps: &[Step], now: f64) -> Option<f64> {
    let mut fields = Fields::default();
    let mut rest: Vec<char> = input.chars().collect();
    let mut start = 0;
    for step in steps {
        let token = match step {
            Step::Literal(length) => {
                start += length;
                continue;
            }
            Step::Token(token) => *token,
        };
        let tail = &rest[start.min(rest.len())..];
        let (at, length) = search(pattern(token), tail)?;
        let matched: Vec<char> = tail[at..at + length].to_vec();
        if !read(token, &mut fields, &matched.iter().collect::<String>()) {
            return None;
        }
        if let Some(found) = find_chars(&rest, &matched) {
            rest.drain(found..found + matched.len());
        }
    }
    if let (Some(afternoon), Some(hours)) = (fields.afternoon, fields.hours) {
        if afternoon {
            if hours < 12.0 {
                fields.hours = Some(hours + 12.0);
            }
        } else if hours == 12.0 {
            fields.hours = Some(0.0);
        }
    }
    let n = parts(now);
    let day = if fields.day_given || truthy(fields.day) {
        fields.day.unwrap_or(f64::NAN)
    } else if !truthy(fields.year) && !truthy(fields.month) {
        n.date
    } else {
        1.0
    };
    let year = if truthy(fields.year) {
        fields.year.unwrap_or(f64::NAN)
    } else {
        n.year
    };
    let month = if truthy(fields.year) && !truthy(fields.month) {
        0.0
    } else if fields.month.is_some_and(|month| month > 0.0) {
        fields.month.unwrap_or(f64::NAN) - 1.0
    } else {
        n.month
    };
    let hours = or_zero(fields.hours);
    let minutes = or_zero(fields.minutes);
    let seconds = or_zero(fields.seconds);
    let ms = or_zero(fields.milliseconds);
    if let Some(offset) = fields.offset {
        return Some(local_date(
            year,
            month,
            day,
            hours,
            minutes,
            seconds,
            ms + offset * MINUTE,
        ));
    }
    if truthy(fields.week) {
        return None;
    }
    Some(local_date(year, month, day, hours, minutes, seconds, ms))
}

struct Template {
    steps: Option<Vec<Step>>,
    format: DateFormat,
}

impl Template {
    fn new(template: &str) -> Self {
        Self {
            steps: compile(template),
            format: DateFormat::new(template),
        }
    }

    fn parse(&self, input: &str, template: &str, now: f64) -> Option<f64> {
        let time = if template == "x" || template == "X" {
            time_clip(js_number(input) * if template == "X" { 1000.0 } else { 1.0 })
        } else {
            loose(input, self.steps.as_deref()?, now)?
        };
        (!time.is_nan() && self.format.format(time).as_deref() == Some(input)).then_some(time)
    }
}

pub fn parse_date(input: &str, template: &str, now: f64) -> Option<f64> {
    Template::new(template).parse(input, template, now)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Keyword {
    Invalid,
    Month,
    Zone,
    Separator,
    AmPm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DateToken {
    Number(i64, usize),
    Symbol(char),
    Space,
    Word(Keyword, i64, usize),
    Unknown,
    End,
    Invalid,
}

const DATE_KEYWORDS: [(&str, Keyword, i64); 27] = [
    ("jan", Keyword::Month, 1),
    ("feb", Keyword::Month, 2),
    ("mar", Keyword::Month, 3),
    ("apr", Keyword::Month, 4),
    ("may", Keyword::Month, 5),
    ("jun", Keyword::Month, 6),
    ("jul", Keyword::Month, 7),
    ("aug", Keyword::Month, 8),
    ("sep", Keyword::Month, 9),
    ("oct", Keyword::Month, 10),
    ("nov", Keyword::Month, 11),
    ("dec", Keyword::Month, 12),
    ("am", Keyword::AmPm, 0),
    ("pm", Keyword::AmPm, 12),
    ("ut", Keyword::Zone, 0),
    ("utc", Keyword::Zone, 0),
    ("z", Keyword::Zone, 0),
    ("gmt", Keyword::Zone, 0),
    ("cdt", Keyword::Zone, -5),
    ("cst", Keyword::Zone, -6),
    ("edt", Keyword::Zone, -4),
    ("est", Keyword::Zone, -5),
    ("mdt", Keyword::Zone, -6),
    ("mst", Keyword::Zone, -7),
    ("pdt", Keyword::Zone, -7),
    ("pst", Keyword::Zone, -8),
    ("t", Keyword::Separator, 0),
];

struct DateScanner {
    chars: Vec<char>,
    at: usize,
    next: DateToken,
}

impl DateScanner {
    fn new(value: &str) -> Self {
        let mut scanner = Self {
            chars: value.chars().collect(),
            at: 0,
            next: DateToken::End,
        };
        scanner.next = scanner.scan();
        scanner
    }

    fn next(&mut self) -> DateToken {
        let token = self.next;
        self.next = self.scan();
        token
    }

    fn peek(&self) -> DateToken {
        self.next
    }

    fn skip(&mut self, symbol: char) -> bool {
        if self.next == DateToken::Symbol(symbol) {
            self.next = self.scan();
            true
        } else {
            false
        }
    }

    fn scan(&mut self) -> DateToken {
        let Some(&ch) = self.chars.get(self.at) else {
            return DateToken::End;
        };
        let start = self.at;
        if ch.is_ascii_digit() {
            while self.chars.get(self.at) == Some(&'0') {
                self.at += 1;
            }
            let mut value = 0;
            let mut count = 0;
            while let Some(digit) = self.chars.get(self.at).and_then(|ch| ch.to_digit(10)) {
                if count < 9 {
                    value = value * 10 + i64::from(digit);
                }
                count += 1;
                self.at += 1;
            }
            return DateToken::Number(value, self.at - start);
        }
        if matches!(ch, ':' | '-' | '+' | '.' | ')') {
            self.at += 1;
            return DateToken::Symbol(ch);
        }
        let wordy =
            |ch: char| ch >= 'A' && (!is_space(ch) || matches!(ch, '\u{2028}' | '\u{2029}'));
        if wordy(ch) {
            let mut word = String::new();
            let mut length = 0;
            while let Some(&ch) = self.chars.get(self.at)
                && wordy(ch)
            {
                if length < 3 {
                    word.push(char::from_u32(ch as u32 | 0x20).unwrap_or(ch));
                }
                length += 1;
                self.at += 1;
            }
            let (kind, value) = DATE_KEYWORDS
                .iter()
                .find(|(name, kind, _)| *name == word && (length <= 3 || *kind == Keyword::Month))
                .map_or((Keyword::Invalid, 0), |(_, kind, value)| (*kind, *value));
            return DateToken::Word(kind, value, length);
        }
        if is_space(ch) {
            self.at += 1;
            return DateToken::Space;
        }
        if ch == '(' {
            let mut balance = 0;
            loop {
                match self.chars.get(self.at) {
                    Some(')') => balance -= 1,
                    Some('(') => balance += 1,
                    _ => {}
                }
                self.at += 1;
                if balance <= 0 || self.at >= self.chars.len() {
                    break;
                }
            }
            return DateToken::Unknown;
        }
        self.at += 1;
        DateToken::Unknown
    }
}

#[derive(Default)]
struct DayComposer {
    comp: [i64; 3],
    index: usize,
    named_month: Option<i64>,
    iso: bool,
}

impl DayComposer {
    fn add(&mut self, value: i64) -> bool {
        if self.index == 3 {
            return false;
        }
        self.comp[self.index] = value;
        self.index += 1;
        true
    }

    fn write(&mut self) -> Option<(i64, i64, i64)> {
        if self.index < 1 {
            return None;
        }
        while self.index < 3 {
            self.comp[self.index] = 1;
            self.index += 1;
        }
        let is_day = |value: i64| (1..=31).contains(&value);
        let (mut year, month, day) = match self.named_month {
            None if self.iso || !is_day(self.comp[0]) => (self.comp[0], self.comp[1], self.comp[2]),
            None => (self.comp[2], self.comp[0], self.comp[1]),
            Some(named) if !is_day(self.comp[0]) => (self.comp[0], named, self.comp[1]),
            Some(named) => (self.comp[1], named, self.comp[0]),
        };
        if !self.iso {
            if (0..=49).contains(&year) {
                year += 2000;
            } else if (50..=99).contains(&year) {
                year += 1900;
            }
        }
        ((-(1 << 30)..(1 << 30)).contains(&year) && (1..=12).contains(&month) && is_day(day))
            .then_some((year, month - 1, day))
    }
}

#[derive(Default)]
struct TimeComposer {
    comp: [i64; 4],
    index: usize,
    hour_offset: Option<i64>,
}

impl TimeComposer {
    fn add(&mut self, value: i64) -> bool {
        if self.index == 4 {
            return false;
        }
        self.comp[self.index] = value;
        self.index += 1;
        true
    }

    fn add_final(&mut self, value: i64) -> bool {
        if !self.add(value) {
            return false;
        }
        while self.index < 4 {
            self.comp[self.index] = 0;
            self.index += 1;
        }
        true
    }

    fn expecting(&self, value: i64) -> bool {
        (self.index == 1 && (0..=59).contains(&value))
            || (self.index == 2 && (0..=59).contains(&value))
            || (self.index == 3 && (0..=999).contains(&value))
    }

    fn write(&mut self) -> Option<[i64; 4]> {
        while self.index < 4 {
            self.comp[self.index] = 0;
            self.index += 1;
        }
        let [mut hour, minute, second, ms] = self.comp;
        if let Some(offset) = self.hour_offset {
            if !(0..=12).contains(&hour) {
                return None;
            }
            hour = hour % 12 + offset;
        }
        let valid = (0..=23).contains(&hour)
            && (0..=59).contains(&minute)
            && (0..=59).contains(&second)
            && (0..=999).contains(&ms);
        (valid || (hour == 24 && minute == 0 && second == 0 && ms == 0))
            .then_some([hour, minute, second, ms])
    }
}

#[derive(Default)]
struct ZoneComposer {
    sign: Option<i64>,
    hour: Option<i64>,
    minute: Option<i64>,
}

impl ZoneComposer {
    fn set(&mut self, hours: i64) {
        let sign = if hours < 0 { -1 } else { 1 };
        self.sign = Some(sign);
        self.hour = Some(hours * sign);
        self.minute = Some(0);
    }

    fn expecting(&self, value: i64) -> bool {
        self.hour.is_some() && self.minute.is_none() && (0..=59).contains(&value)
    }

    fn utc(&self) -> bool {
        self.hour == Some(0) && self.minute == Some(0)
    }

    fn write(&self) -> Option<Option<i64>> {
        let Some(sign) = self.sign else {
            return Some(None);
        };
        let seconds = self.hour.unwrap_or(0) * 3600 + self.minute.unwrap_or(0) * 60;
        (seconds < (1 << 30)).then_some(Some(seconds * sign))
    }
}

fn read_ms(token: DateToken) -> i64 {
    let DateToken::Number(mut value, length) = token else {
        return -1;
    };
    match length {
        1 => value * 100,
        2 => value * 10,
        3 => value,
        _ => {
            for _ in 3..length.min(9) {
                value /= 10;
            }
            value
        }
    }
}

fn is_z(token: DateToken) -> bool {
    matches!(token, DateToken::Word(Keyword::Zone, _, 1))
}

fn es5(
    scanner: &mut DateScanner,
    day: &mut DayComposer,
    time: &mut TimeComposer,
    zone: &mut ZoneComposer,
) -> DateToken {
    if let DateToken::Symbol(sign @ ('+' | '-')) = scanner.peek() {
        let token = scanner.next();
        let DateToken::Number(year, 6) = scanner.peek() else {
            return token;
        };
        scanner.next();
        if sign == '-' && year == 0 {
            return token;
        }
        day.add(if sign == '-' { -year } else { year });
    } else if let DateToken::Number(year, 4) = scanner.peek() {
        scanner.next();
        day.add(year);
    } else {
        return scanner.next();
    }
    if scanner.skip('-') {
        match scanner.peek() {
            DateToken::Number(month, 2) if (1..=12).contains(&month) => {
                scanner.next();
                day.add(month);
            }
            _ => return scanner.next(),
        }
        if scanner.skip('-') {
            match scanner.peek() {
                DateToken::Number(date, 2) if (1..=31).contains(&date) => {
                    scanner.next();
                    day.add(date);
                }
                _ => return scanner.next(),
            }
        }
    }
    if matches!(scanner.peek(), DateToken::Word(Keyword::Separator, _, _)) {
        scanner.next();
        let DateToken::Number(hour, 2) = scanner.peek() else {
            return DateToken::Invalid;
        };
        if !(0..=24).contains(&hour) {
            return DateToken::Invalid;
        }
        let midnight = hour == 24;
        scanner.next();
        time.add(hour);
        if !scanner.skip(':') {
            return DateToken::Invalid;
        }
        match scanner.peek() {
            DateToken::Number(minute, 2)
                if (0..=59).contains(&minute) && !(midnight && minute > 0) =>
            {
                scanner.next();
                time.add(minute);
            }
            _ => return DateToken::Invalid,
        }
        if scanner.skip(':') {
            match scanner.peek() {
                DateToken::Number(second, 2)
                    if (0..=59).contains(&second) && !(midnight && second > 0) =>
                {
                    scanner.next();
                    time.add(second);
                }
                _ => return DateToken::Invalid,
            }
            if scanner.skip('.') {
                match scanner.peek() {
                    DateToken::Number(value, _) if !(midnight && value > 0) => {
                        let token = scanner.next();
                        time.add(read_ms(token));
                    }
                    _ => return DateToken::Invalid,
                }
            }
        }
        if is_z(scanner.peek()) {
            scanner.next();
            zone.set(0);
        } else if let DateToken::Symbol(sign @ ('+' | '-')) = scanner.peek() {
            scanner.next();
            zone.sign = Some(if sign == '+' { 1 } else { -1 });
            if let DateToken::Number(value, 4) = scanner.peek() {
                scanner.next();
                let (hours, minutes) = (value / 100, value % 100);
                if !(0..=23).contains(&hours) || !(0..=59).contains(&minutes) {
                    return DateToken::Invalid;
                }
                zone.hour = Some(hours);
                zone.minute = Some(minutes);
            } else {
                let DateToken::Number(hours, 2) = scanner.peek() else {
                    return DateToken::Invalid;
                };
                if !(0..=23).contains(&hours) {
                    return DateToken::Invalid;
                }
                scanner.next();
                zone.hour = Some(hours);
                if !scanner.skip(':') {
                    return DateToken::Invalid;
                }
                let DateToken::Number(minutes, 2) = scanner.peek() else {
                    return DateToken::Invalid;
                };
                if !(0..=59).contains(&minutes) {
                    return DateToken::Invalid;
                }
                scanner.next();
                zone.minute = Some(minutes);
            }
        }
        if scanner.peek() != DateToken::End {
            return DateToken::Invalid;
        }
    } else if scanner.peek() != DateToken::End {
        return scanner.next();
    }
    if zone.hour.is_none() && time.index == 0 {
        zone.set(0);
    }
    day.iso = true;
    DateToken::End
}

fn legacy(
    scanner: &mut DateScanner,
    first: DateToken,
    day: &mut DayComposer,
    time: &mut TimeComposer,
    zone: &mut ZoneComposer,
) -> Option<()> {
    let mut has_number = day.index > 0;
    let mut token = first;
    while token != DateToken::End {
        match token {
            DateToken::Number(value, _) => {
                has_number = true;
                if scanner.skip(':') {
                    if scanner.skip(':') {
                        if time.index > 0 {
                            return None;
                        }
                        time.add(value);
                        time.add(0);
                    } else {
                        if !time.add(value) {
                            return None;
                        }
                        if scanner.peek() == DateToken::Symbol('.') {
                            scanner.next();
                        }
                    }
                } else if scanner.skip('.') && time.expecting(value) {
                    time.add(value);
                    if !matches!(scanner.peek(), DateToken::Number(..)) {
                        return None;
                    }
                    let ms = read_ms(scanner.next());
                    if ms < 0 {
                        return None;
                    }
                    time.add_final(ms);
                } else if zone.expecting(value) {
                    zone.minute = Some(value);
                } else if time.expecting(value) {
                    time.add_final(value);
                    let peek = scanner.peek();
                    if !matches!(
                        peek,
                        DateToken::End | DateToken::Space | DateToken::Symbol('+' | '-')
                    ) && !is_z(peek)
                    {
                        return None;
                    }
                } else {
                    if !day.add(value) {
                        return None;
                    }
                    scanner.skip('-');
                }
            }
            DateToken::Word(kind, value, _) => {
                if kind == Keyword::AmPm && time.index > 0 {
                    time.hour_offset = Some(value);
                } else if kind == Keyword::Month {
                    day.named_month = Some(value);
                    scanner.skip('-');
                } else if kind == Keyword::Zone && has_number {
                    zone.set(value);
                } else if has_number || matches!(scanner.peek(), DateToken::Number(..)) {
                    return None;
                }
            }
            DateToken::Symbol(sign @ ('+' | '-')) if zone.utc() || time.index > 0 => {
                zone.sign = Some(if sign == '-' { -1 } else { 1 });
                let (mut value, mut length) = (0, 0);
                if let DateToken::Number(number, digits) = scanner.peek() {
                    scanner.next();
                    value = number;
                    length = digits;
                }
                has_number = true;
                if scanner.peek() == DateToken::Symbol(':') {
                    zone.hour = Some(value);
                    zone.minute = None;
                } else if length == 1 || length == 2 {
                    zone.hour = Some(value);
                    zone.minute = Some(0);
                } else if length == 3 || length == 4 {
                    zone.hour = Some(value / 100);
                    zone.minute = Some(value % 100);
                } else {
                    return None;
                }
            }
            DateToken::Symbol('+' | '-' | ')') if has_number => return None,
            _ => {}
        }
        token = scanner.next();
    }
    Some(())
}

pub fn parse_js_date(value: &str) -> f64 {
    let mut scanner = DateScanner::new(value);
    let mut day = DayComposer::default();
    let mut time = TimeComposer::default();
    let mut zone = ZoneComposer::default();
    let first = es5(&mut scanner, &mut day, &mut time, &mut zone);
    if first == DateToken::Invalid
        || legacy(&mut scanner, first, &mut day, &mut time, &mut zone).is_none()
    {
        return f64::NAN;
    }
    let (Some((year, month, date)), Some([hours, minutes, seconds, ms]), Some(offset)) =
        (day.write(), time.write(), zone.write())
    else {
        return f64::NAN;
    };
    let local = make_day(year as f64, month as f64, date as f64) * DAY
        + make_time(hours as f64, minutes as f64, seconds as f64, ms as f64);
    time_clip(local - offset.map_or(0.0, |seconds| seconds as f64 * SECOND))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TickUnit {
    Millisecond,
    Second,
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Year,
}

#[derive(Clone, Copy)]
struct Interval {
    unit: TickUnit,
    step: f64,
    week_start: f64,
    epoch: f64,
}

fn span(start: f64, end: f64, unit: f64) -> f64 {
    ((end - start) / unit).floor()
}

impl Interval {
    fn new(unit: TickUnit, step: f64, week_start: f64) -> Self {
        let mut interval = Self {
            unit,
            step,
            week_start,
            epoch: 0.0,
        };
        interval.epoch = interval.base_floor(0.0);
        interval
    }

    fn cycle(&self) -> f64 {
        match self.unit {
            TickUnit::Second | TickUnit::Minute => 60.0,
            TickUnit::Hour => 24.0,
            TickUnit::Day => 31.0,
            TickUnit::Month => 12.0,
            _ => f64::INFINITY,
        }
    }

    fn ms(&self) -> f64 {
        match self.unit {
            TickUnit::Millisecond => 1.0,
            TickUnit::Second => SECOND,
            TickUnit::Minute => MINUTE,
            TickUnit::Hour => HOUR,
            TickUnit::Day => DAY,
            TickUnit::Week => WEEK,
            TickUnit::Month => MONTH,
            TickUnit::Year => YEAR,
        }
    }

    fn field(&self, time: f64) -> f64 {
        let p = parts(time);
        match self.unit {
            TickUnit::Second => p.seconds,
            TickUnit::Minute => p.minutes,
            TickUnit::Hour => p.hours,
            TickUnit::Day => p.date - 1.0,
            TickUnit::Month => p.month,
            _ => f64::NAN,
        }
    }

    fn base_floor(&self, time: f64) -> f64 {
        let p = parts(time);
        match self.unit {
            TickUnit::Millisecond => time,
            TickUnit::Second => time_clip(time - p.ms),
            TickUnit::Minute => time_clip(time - p.ms - p.seconds * SECOND),
            TickUnit::Hour => time_clip(time - p.ms - p.seconds * SECOND - p.minutes * MINUTE),
            TickUnit::Day => start_of_day(time),
            TickUnit::Week => start_of_day(set_date(
                time,
                p.date - (p.weekday + 7.0 - self.week_start) % 7.0,
            )),
            TickUnit::Month => start_of_day(set_date(time, 1.0)),
            TickUnit::Year => start_of_day(set_month(time, 0.0, Some(1.0))),
        }
    }

    fn base_offset(&self, time: f64, step: f64) -> f64 {
        let p = parts(time);
        match self.unit {
            TickUnit::Millisecond => time_clip(time + step),
            TickUnit::Second => time_clip(time + step * SECOND),
            TickUnit::Minute => time_clip(time + step * MINUTE),
            TickUnit::Hour => time_clip(time + step * HOUR),
            TickUnit::Day => set_date(time, p.date + step),
            TickUnit::Week => set_date(time, p.date + step * 7.0),
            TickUnit::Month => set_month(time, p.month + step, None),
            TickUnit::Year => set_full_year(time, p.year + step),
        }
    }

    fn test(&self, time: f64) -> bool {
        self.field(time) % self.step == 0.0
    }

    fn floor(&self, time: f64) -> f64 {
        if self.step <= 1.0 {
            return self.base_floor(time);
        }
        match self.unit {
            TickUnit::Millisecond => time_clip((time / self.step).floor() * self.step),
            TickUnit::Year => self.base_floor(set_full_year(
                time,
                (parts(time).year / self.step).floor() * self.step,
            )),
            TickUnit::Week => {
                let floored = self.base_floor(time);
                let behind = span(self.epoch, floored, WEEK);
                self.base_offset(floored, -(((behind % self.step) + self.step) % self.step))
            }
            _ => {
                let mut floored = self.base_floor(time);
                while floored.is_finite() && !self.test(floored) {
                    floored = self.base_floor(time_clip(floored - 1.0));
                }
                floored
            }
        }
    }

    fn offset(&self, time: f64, count: f64) -> f64 {
        if self.step <= 1.0 {
            return self.base_offset(time, count);
        }
        match self.unit {
            TickUnit::Millisecond => time_clip(time + count * self.step),
            TickUnit::Year => set_full_year(time, parts(time).year + count * self.step),
            TickUnit::Week => self.base_offset(time, count * self.step),
            _ => {
                let mut next = time;
                let mut index = 0.0;
                while index < count && next.is_finite() {
                    next = self.base_offset(next, 1.0);
                    while next.is_finite() && !self.test(next) {
                        next = self.base_offset(next, 1.0);
                    }
                    index += 1.0;
                }
                next
            }
        }
    }
}

fn range(interval: &Interval, start: f64, stop: f64, limit: f64) -> Option<Vec<f64>> {
    let mut time = interval.floor(time_clip(start - 1.0));
    time = interval.floor(interval.offset(time, 1.0));
    let mut ticks = Vec::new();
    let mut previous = f64::NEG_INFINITY;
    while time <= stop && time > previous {
        if ticks.len() as f64 >= limit {
            return None;
        }
        previous = time;
        ticks.push(time);
        time = interval.floor(interval.offset(time, 1.0));
    }
    Some(ticks)
}

fn tick_step(start: f64, stop: f64, count: f64) -> f64 {
    let step = (stop - start) / count.max(0.0);
    let power = step.log10().floor();
    let error = step / 10f64.powf(power);
    let factor = if error >= 50f64.sqrt() {
        10.0
    } else if error >= 10f64.sqrt() {
        5.0
    } else if error >= std::f64::consts::SQRT_2 {
        2.0
    } else {
        1.0
    };
    10f64.powf(power) * factor
}

const AUTO: [(TickUnit, f64, f64); 18] = [
    (TickUnit::Second, 1.0, SECOND),
    (TickUnit::Second, 5.0, 5.0 * SECOND),
    (TickUnit::Second, 15.0, 15.0 * SECOND),
    (TickUnit::Second, 30.0, 30.0 * SECOND),
    (TickUnit::Minute, 1.0, MINUTE),
    (TickUnit::Minute, 5.0, 5.0 * MINUTE),
    (TickUnit::Minute, 15.0, 15.0 * MINUTE),
    (TickUnit::Minute, 30.0, 30.0 * MINUTE),
    (TickUnit::Hour, 1.0, HOUR),
    (TickUnit::Hour, 3.0, 3.0 * HOUR),
    (TickUnit::Hour, 6.0, 6.0 * HOUR),
    (TickUnit::Hour, 12.0, 12.0 * HOUR),
    (TickUnit::Day, 1.0, DAY),
    (TickUnit::Day, 2.0, 2.0 * DAY),
    (TickUnit::Week, 1.0, WEEK),
    (TickUnit::Month, 1.0, MONTH),
    (TickUnit::Month, 3.0, 3.0 * MONTH),
    (TickUnit::Year, 1.0, YEAR),
];

pub fn auto_ticks(start: f64, stop: f64, count: f64) -> Vec<f64> {
    let target = (stop - start) / count;
    let index = AUTO
        .iter()
        .position(|(_, _, ms)| *ms > target)
        .unwrap_or(AUTO.len());
    let interval = if index == AUTO.len() {
        Interval::new(
            TickUnit::Year,
            tick_step(start / YEAR, stop / YEAR, count).max(1.0),
            0.0,
        )
    } else if index == 0 {
        Interval::new(
            TickUnit::Millisecond,
            tick_step(start, stop, count).max(1.0),
            0.0,
        )
    } else {
        let pick = if target / AUTO[index - 1].2 < AUTO[index].2 / target {
            index - 1
        } else {
            index
        };
        Interval::new(AUTO[pick].0, AUTO[pick].1, 0.0)
    };
    range(&interval, start, stop, 4.0 * count + 8.0).unwrap_or_default()
}

pub fn interval_ticks(
    interval: &str,
    weekday: &str,
    start: f64,
    stop: f64,
    limit: f64,
) -> Option<Vec<f64>> {
    let digits = interval.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 || interval.starts_with('0') {
        return None;
    }
    let unit = match &interval[digits..] {
        "millisecond" => TickUnit::Millisecond,
        "second" => TickUnit::Second,
        "minute" => TickUnit::Minute,
        "hour" => TickUnit::Hour,
        "day" => TickUnit::Day,
        "week" => TickUnit::Week,
        "month" => TickUnit::Month,
        _ => return None,
    };
    let step: f64 = interval[..digits].parse().ok()?;
    let week_start = WEEK_STARTS
        .iter()
        .position(|name| *name == weekday)
        .unwrap_or(0) as f64;
    let calendar = Interval::new(unit, step, week_start);
    if (stop - start) / (step.min(calendar.cycle()) * calendar.ms()) > limit {
        return None;
    }
    range(&calendar, start, stop, limit)
}

fn pad_with(value: f64, fill: &str, width: usize) -> String {
    let sign = if value < 0.0 { "-" } else { "" };
    let digits = js_string(if value < 0.0 { -value } else { value });
    let count = digits.encode_utf16().count();
    if count < width {
        format!("{sign}{}{digits}", fill.repeat(width - count))
    } else {
        format!("{sign}{digits}")
    }
}

fn week_of_year(time: f64, week_start: f64) -> f64 {
    let week = Interval::new(TickUnit::Week, 1.0, week_start);
    let year_start = Interval::new(TickUnit::Year, 1.0, 0.0).base_floor(time);
    span(
        week.base_floor(time_clip(year_start - 1.0)),
        week.base_floor(time),
        WEEK,
    )
}

fn iso_thursday(time: f64) -> f64 {
    let thursday = Interval::new(TickUnit::Week, 1.0, 4.0);
    let day = parts(time).weekday;
    if day >= 4.0 || day == 0.0 {
        thursday.base_floor(time)
    } else {
        let floored = thursday.base_floor(time_clip(time - 1.0));
        thursday.base_floor(thursday.base_offset(floored, 1.0))
    }
}

fn directive(code: Option<char>, time: f64, fill: &str) -> String {
    let p = parts(time);
    let day_of = |time| Interval::new(TickUnit::Day, 1.0, 0.0).base_floor(time);
    let year_of = |time| Interval::new(TickUnit::Year, 1.0, 0.0).base_floor(time);
    match code {
        Some('a') => prefix(weekday_name(p.weekday), 3).to_owned(),
        Some('A') => weekday_name(p.weekday).to_owned(),
        Some('b') => prefix(month_name(p.month), 3).to_owned(),
        Some('B') => month_name(p.month).to_owned(),
        Some('c') => time_format("%x, %X", time),
        Some('d' | 'e') => pad_with(p.date, fill, 2),
        Some('f') => pad_with(p.ms, fill, 3) + "000",
        Some('g') => pad_with(parts(iso_thursday(time)).year % 100.0, fill, 2),
        Some('G') => pad_with(parts(iso_thursday(time)).year % 10000.0, fill, 4),
        Some('H') => pad_with(p.hours, fill, 2),
        Some('I') => pad_with(
            if p.hours % 12.0 == 0.0 {
                12.0
            } else {
                p.hours % 12.0
            },
            fill,
            2,
        ),
        Some('j') => pad_with(1.0 + span(year_of(time), day_of(time), DAY), fill, 3),
        Some('L') => pad_with(p.ms, fill, 3),
        Some('m') => pad_with(p.month + 1.0, fill, 2),
        Some('M') => pad_with(p.minutes, fill, 2),
        Some('p') => if p.hours >= 12.0 { "PM" } else { "AM" }.to_owned(),
        Some('q') => js_string(1.0 + (p.month / 3.0).floor()),
        Some('Q') => js_string(time),
        Some('s') => js_string((time / 1000.0).floor()),
        Some('S') => pad_with(p.seconds, fill, 2),
        Some('u') => js_string(if p.weekday == 0.0 { 7.0 } else { p.weekday }),
        Some('U') => pad_with(week_of_year(time, 0.0), fill, 2),
        Some('V') => {
            let thursday = iso_thursday(time);
            let year_start = year_of(thursday);
            let first = Interval::new(TickUnit::Week, 1.0, 4.0).base_floor(year_start);
            let extra = if parts(year_start).weekday == 4.0 {
                1.0
            } else {
                0.0
            };
            pad_with(span(first, thursday, WEEK) + extra, fill, 2)
        }
        Some('w') => js_string(p.weekday),
        Some('W') => pad_with(week_of_year(time, 1.0), fill, 2),
        Some('x') => time_format("%-m/%-d/%Y", time),
        Some('X') => time_format("%-I:%M:%S %p", time),
        Some('y') => pad_with(p.year % 100.0, fill, 2),
        Some('Y') => pad_with(p.year % 10000.0, fill, 4),
        Some('Z') => "+0000".to_owned(),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

pub fn time_format(specifier: &str, time: f64) -> String {
    let chars: Vec<char> = specifier.chars().collect();
    let mut output = String::new();
    let mut at = 0;
    while at < chars.len() {
        if chars[at] != '%' {
            output.push(chars[at]);
            at += 1;
            continue;
        }
        at += 1;
        let mut code = chars.get(at).copied();
        let fill = match code {
            Some('-') => Some(""),
            Some('_') => Some(" "),
            Some('0') => Some("0"),
            _ => None,
        };
        let fill = if let Some(fill) = fill {
            at += 1;
            code = chars.get(at).copied();
            fill
        } else if code == Some('e') {
            " "
        } else {
            "0"
        };
        output.push_str(&directive(code, time, fill));
        at += 1;
    }
    output
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    End,
    Error,
    OpenDirective,
    AccTitle,
    AccTitleValue,
    AccDescr,
    AccDescrValue,
    AccDescrMultilineValue,
    Nl,
    Href,
    Callbackname,
    Callbackargs,
    Click,
    Gantt,
    DateFormat,
    InclusiveEndDates,
    TopAxis,
    AxisFormat,
    TickInterval,
    Includes,
    Excludes,
    TodayMarker,
    Weekday(usize),
    WeekendFriday,
    WeekendSaturday,
    Date,
    Title,
    AccDescription,
    Section,
    TaskTxt,
    TaskData,
    Colon,
    Eof,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::End => "1",
            Self::Error => "INVALID",
            Self::OpenDirective => "open_directive",
            Self::AccTitle => "acc_title",
            Self::AccTitleValue => "acc_title_value",
            Self::AccDescr => "acc_descr",
            Self::AccDescrValue => "acc_descr_value",
            Self::AccDescrMultilineValue => "acc_descr_multiline_value",
            Self::Nl => "NL",
            Self::Href => "href",
            Self::Callbackname => "callbackname",
            Self::Callbackargs => "callbackargs",
            Self::Click => "click",
            Self::Gantt => "gantt",
            Self::DateFormat => "dateFormat",
            Self::InclusiveEndDates => "inclusiveEndDates",
            Self::TopAxis => "topAxis",
            Self::AxisFormat => "axisFormat",
            Self::TickInterval => "tickInterval",
            Self::Includes => "includes",
            Self::Excludes => "excludes",
            Self::TodayMarker => "todayMarker",
            Self::Weekday(day) => [
                "weekday_monday",
                "weekday_tuesday",
                "weekday_wednesday",
                "weekday_thursday",
                "weekday_friday",
                "weekday_saturday",
                "weekday_sunday",
            ][day],
            Self::WeekendFriday => "weekend_friday",
            Self::WeekendSaturday => "weekend_saturday",
            Self::Date => "date",
            Self::Title => "title",
            Self::AccDescription => "accDescription",
            Self::Section => "section",
            Self::TaskTxt => "taskTxt",
            Self::TaskData => "taskData",
            Self::Colon => ":",
            Self::Eof => "EOF",
        }
    }

    fn starts_line(self) -> bool {
        matches!(
            self,
            Self::Weekday(_)
                | Self::WeekendFriday
                | Self::WeekendSaturday
                | Self::Eof
                | Self::Nl
                | Self::DateFormat
                | Self::InclusiveEndDates
                | Self::TopAxis
                | Self::AxisFormat
                | Self::TickInterval
                | Self::Excludes
                | Self::Includes
                | Self::TodayMarker
                | Self::Title
                | Self::AccTitle
                | Self::AccDescr
                | Self::AccDescrMultilineValue
                | Self::Section
                | Self::TaskTxt
                | Self::Click
        )
    }
}

#[derive(Clone, Copy, Debug)]
struct Token {
    kind: Kind,
    start: usize,
    end: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Initial,
    AccTitle,
    AccDescr,
    AccDescrMultiline,
    Href,
    Callbackname,
    Callbackargs,
    Click,
}

fn char_at(source: &str, at: usize) -> Option<char> {
    source.get(at..).and_then(|rest| rest.chars().next())
}

fn word_at(source: &str, at: usize, word: &str) -> Option<usize> {
    let end = at + word.len();
    source
        .as_bytes()
        .get(at..end)
        .is_some_and(|bytes| bytes.eq_ignore_ascii_case(word.as_bytes()))
        .then_some(end)
}

fn skip_spaces(source: &str, mut at: usize) -> usize {
    while let Some(ch) = char_at(source, at)
        && is_space(ch)
    {
        at += ch.len_utf8();
    }
    at
}

fn boundary(source: &str, at: usize) -> bool {
    !source
        .as_bytes()
        .get(at)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
}

fn statement(
    source: &str,
    at: usize,
    word: &str,
    line_space: bool,
    stops: &[char],
) -> Option<usize> {
    let mut at = word_at(source, at, word)?;
    let ch = char_at(source, at)?;
    if !is_space(ch) || (line_space && matches!(ch, '\r' | '\n')) {
        return None;
    }
    at += ch.len_utf8();
    let start = at;
    while let Some(ch) = char_at(source, at)
        && !stops.contains(&ch)
    {
        at += ch.len_utf8();
    }
    (at > start).then_some(at)
}

fn bare_word(source: &str, at: usize, word: &str) -> Option<usize> {
    let end = word_at(source, at, word)?;
    boundary(source, end).then_some(end)
}

fn named_day(source: &str, at: usize, keyword: &str, names: &[&str]) -> Option<(usize, usize)> {
    let after = word_at(source, at, keyword)?;
    let at = skip_spaces(source, after);
    if at == after {
        return None;
    }
    names.iter().enumerate().find_map(|(index, name)| {
        let end = word_at(source, at, name)?;
        boundary(source, end).then_some((index, end))
    })
}

const STOP_HASH: &[char] = &['#', '\n', ';'];

fn keyword(source: &str, at: usize, lower: char) -> Option<(Option<Kind>, usize, State)> {
    let token = |kind, end| Some((Some(kind), end, State::Initial));
    match lower {
        'a' => {
            for (word, ending, kind, state) in [
                ("accTitle", ':', Some(Kind::AccTitle), State::AccTitle),
                ("accDescr", ':', Some(Kind::AccDescr), State::AccDescr),
                ("accDescr", '{', None, State::AccDescrMultiline),
            ] {
                if let Some(end) = word_at(source, at, word) {
                    let end = skip_spaces(source, end);
                    if char_at(source, end) == Some(ending) {
                        return Some((kind, skip_spaces(source, end + 1), state));
                    }
                }
            }
        }
        'h' => {
            if let Some(end) = word_at(source, at, "href") {
                let spaced = skip_spaces(source, end);
                if spaced > end && char_at(source, spaced) == Some('"') {
                    return Some((None, spaced + 1, State::Href));
                }
            }
        }
        'c' => {
            for (word, state) in [("call", State::Callbackname), ("click", State::Click)] {
                if let Some(end) = word_at(source, at, word) {
                    let spaced = skip_spaces(source, end);
                    if spaced > end {
                        return Some((None, spaced, state));
                    }
                }
            }
        }
        't' => {
            if let Some(mut end) = word_at(source, at, "title") {
                while let Some(ch) = char_at(source, end)
                    && is_space(ch)
                    && !matches!(ch, '\r' | '\n')
                {
                    end += ch.len_utf8();
                }
                if matches!(char_at(source, end), None | Some('\r' | '\n')) {
                    return Some((None, end, State::Initial));
                }
            }
        }
        'w' => {
            if let Some((index, end)) = named_day(source, at, "weekday", &DAY_NAMES) {
                return token(Kind::Weekday(index), end);
            }
            if let Some((index, end)) = named_day(source, at, "weekend", &["friday", "saturday"]) {
                return token(
                    if index == 0 {
                        Kind::WeekendFriday
                    } else {
                        Kind::WeekendSaturday
                    },
                    end,
                );
            }
        }
        _ => {}
    }
    let found = match lower {
        'a' => statement(source, at, "axisFormat", false, STOP_HASH)
            .map(|end| (Kind::AxisFormat, end))
            .or_else(|| {
                statement(source, at, "accDescription", false, STOP_HASH)
                    .map(|end| (Kind::AccDescription, end))
            }),
        'g' => bare_word(source, at, "gantt").map(|end| (Kind::Gantt, end)),
        'd' => {
            statement(source, at, "dateFormat", false, STOP_HASH).map(|end| (Kind::DateFormat, end))
        }
        'i' => bare_word(source, at, "inclusiveEndDates")
            .map(|end| (Kind::InclusiveEndDates, end))
            .or_else(|| {
                statement(source, at, "includes", false, STOP_HASH).map(|end| (Kind::Includes, end))
            }),
        't' => bare_word(source, at, "topAxis")
            .map(|end| (Kind::TopAxis, end))
            .or_else(|| {
                statement(source, at, "tickInterval", false, STOP_HASH)
                    .map(|end| (Kind::TickInterval, end))
            })
            .or_else(|| {
                statement(source, at, "todayMarker", false, &['\n', ';'])
                    .map(|end| (Kind::TodayMarker, end))
            })
            .or_else(|| {
                statement(source, at, "title", true, &['\n']).map(|end| (Kind::Title, end))
            }),
        'e' => statement(source, at, "excludes", false, STOP_HASH).map(|end| (Kind::Excludes, end)),
        's' => statement(source, at, "section", false, &['\n']).map(|end| (Kind::Section, end)),
        _ => None,
    };
    found.and_then(|(kind, end)| token(kind, end))
}

fn tokenize(source: &str) -> Vec<Token> {
    let n = source.len();
    let mut tokens = Vec::new();
    let mut state = State::Initial;
    let mut at = 0;
    let until = |ch: char, from: usize| source[from..].find(ch).map_or(n, |index| from + index);
    loop {
        if at >= n {
            let kind = match state {
                State::Initial => Kind::Eof,
                State::AccTitle => Kind::AccTitleValue,
                State::AccDescr => Kind::AccDescrValue,
                State::AccDescrMultiline => Kind::AccDescrMultilineValue,
                State::Href => Kind::Href,
                State::Callbackname => Kind::Callbackname,
                State::Callbackargs => Kind::Callbackargs,
                State::Click => Kind::Click,
            };
            tokens.push(Token {
                kind,
                start: n,
                end: n,
            });
            break;
        }
        let ch = char_at(source, at).unwrap_or('\0');
        let mut emit = |kind, start: usize, end: usize, at: &mut usize| {
            tokens.push(Token { kind, start, end });
            *at = end;
        };
        match state {
            State::Initial => {
                if ch == '%' {
                    if char_at(source, at + 1) != Some('%') {
                        let mut end = until('\n', at + 1);
                        while source.as_bytes().get(end) == Some(&b'\n') {
                            end += 1;
                        }
                        at = end;
                    } else if char_at(source, at + 2) == Some('{') {
                        emit(Kind::OpenDirective, at, at + 3, &mut at);
                        let start = at - 3;
                        emit(Kind::Error, start, start, &mut at);
                        tokens.push(Token {
                            kind: Kind::End,
                            start: n,
                            end: n,
                        });
                        return tokens;
                    } else {
                        at = until('\n', at + 2);
                    }
                } else if ch == '\n' {
                    let mut end = at + 1;
                    while source.as_bytes().get(end) == Some(&b'\n') {
                        end += 1;
                    }
                    emit(Kind::Nl, at, end, &mut at);
                } else if is_space(ch) {
                    at = skip_spaces(source, at);
                } else if let Some((kind, end, next)) = keyword(
                    source,
                    at,
                    if ch.is_ascii() {
                        ch.to_ascii_lowercase()
                    } else {
                        '\0'
                    },
                ) {
                    match kind {
                        Some(kind) => emit(kind, at, end, &mut at),
                        None => at = end,
                    }
                    state = next;
                } else if ch.is_ascii_digit() && is_date(source, at) {
                    emit(Kind::Date, at, at + 10, &mut at);
                } else if ch != ':' {
                    let next = at + ch.len_utf8();
                    let end = source[next..]
                        .find([':', '\n'])
                        .map_or(n, |index| next + index);
                    emit(Kind::TaskTxt, at, end, &mut at);
                } else {
                    let end = source[at + 1..]
                        .find(['#', '\n', ';'])
                        .map_or(n, |index| at + 1 + index);
                    emit(
                        if end > at + 1 {
                            Kind::TaskData
                        } else {
                            Kind::Colon
                        },
                        at,
                        end,
                        &mut at,
                    );
                }
            }
            State::AccTitle | State::AccDescr => {
                let kind = if state == State::AccTitle {
                    Kind::AccTitleValue
                } else {
                    Kind::AccDescrValue
                };
                state = State::Initial;
                emit(kind, at, until('\n', at), &mut at);
            }
            State::AccDescrMultiline => {
                if ch == '}' {
                    state = State::Initial;
                    at += 1;
                } else {
                    emit(Kind::AccDescrMultilineValue, at, until('}', at), &mut at);
                }
            }
            State::Href => {
                if ch == '"' {
                    state = State::Initial;
                    at += 1;
                } else {
                    emit(Kind::Href, at, until('"', at), &mut at);
                }
            }
            State::Callbackname => {
                if ch == '(' {
                    let inner = skip_spaces(source, at + 1);
                    if char_at(source, inner) == Some(')') {
                        state = State::Initial;
                        at = inner + 1;
                    } else {
                        state = State::Callbackargs;
                        at += 1;
                    }
                } else {
                    emit(Kind::Callbackname, at, until('(', at), &mut at);
                }
            }
            State::Callbackargs => {
                if ch == ')' {
                    state = State::Initial;
                    at += 1;
                } else {
                    emit(Kind::Callbackargs, at, until(')', at), &mut at);
                }
            }
            State::Click => {
                if is_space(ch) {
                    state = State::Initial;
                    at += ch.len_utf8();
                } else {
                    let end = source[at..]
                        .char_indices()
                        .find(|(_, ch)| is_space(*ch))
                        .map_or(n, |(index, _)| at + index);
                    emit(Kind::Click, at, end, &mut at);
                }
            }
        }
    }
    tokens.push(Token {
        kind: Kind::End,
        start: n,
        end: n,
    });
    tokens
}

fn is_date(source: &str, at: usize) -> bool {
    let bytes = source.as_bytes();
    let Some(date) = bytes.get(at..at + 10) else {
        return false;
    };
    date.iter().enumerate().all(|(index, byte)| {
        if index == 4 || index == 7 {
            *byte == b'-'
        } else {
            byte.is_ascii_digit()
        }
    }) && boundary(source, at + 10)
}

type Edits = Vec<(usize, usize, String)>;

struct Encoded {
    text: String,
    origin: Vec<usize>,
}

fn rewrite(source: &str, edits: &[(usize, usize, String)]) -> (String, Vec<usize>) {
    let mut text = String::with_capacity(source.len());
    let mut origin = Vec::with_capacity(source.len() + 1);
    let mut last = 0;
    for (from, to, replacement) in edits {
        origin.extend(last..*from);
        text.push_str(&source[last..*from]);
        for (index, ch) in replacement.chars().enumerate() {
            let mapped = from + index.min(to - from - 1);
            origin.extend(std::iter::repeat_n(mapped, ch.len_utf8()));
        }
        text.push_str(replacement);
        last = *to;
    }
    origin.extend(last..=source.len());
    text.push_str(&source[last..]);
    (text, origin)
}

fn color_semicolons(source: &str, keyword: &str) -> Edits {
    let mut edits = Vec::new();
    let mut from = source.find(keyword);
    while let Some(start) = from {
        let line_end = source[start..]
            .find(['\n', '\r', '\u{2028}', '\u{2029}'])
            .map_or(source.len(), |index| start + index);
        let mut after_colon = false;
        let mut hash = None;
        for (index, ch) in source[start + keyword.len()..line_end].char_indices() {
            if ch == ':' {
                after_colon = true;
            } else if ch == '#' {
                if after_colon {
                    hash = Some(start + keyword.len() + index);
                    break;
                }
            } else if is_space(ch) {
                after_colon = false;
            }
        }
        if let Some(hash) = hash
            && let Some(semi) = source[..line_end].rfind(';')
            && semi > hash
        {
            edits.push((semi, semi + 1, String::new()));
        }
        from = if line_end >= source.len() {
            None
        } else {
            let next = line_end + char_at(source, line_end).map_or(1, char::len_utf8);
            source[next..].find(keyword).map(|index| next + index)
        };
    }
    edits
}

fn entity_codes(source: &str) -> Edits {
    let bytes = source.as_bytes();
    let mut edits = Vec::new();
    let mut at = 0;
    while let Some(index) = source[at..].find('#') {
        let start = at + index;
        let inner = bytes[start + 1..]
            .iter()
            .take_while(|byte| byte.is_ascii_alphanumeric() || **byte == b'_')
            .count();
        if inner > 0 && bytes.get(start + 1 + inner) == Some(&b';') {
            let code = &source[start + 1..start + 1 + inner];
            let marker = if code.bytes().all(|byte| byte.is_ascii_digit()) {
                "ﬂ°°"
            } else {
                "ﬂ°"
            };
            edits.push((start, start + inner + 2, format!("{marker}{code}¶ß")));
            at = start + inner + 2;
        } else {
            at = start + 1;
        }
    }
    edits
}

fn encode(source: &str) -> Encoded {
    let mut text = source.to_owned();
    let mut origin: Vec<usize> = (0..=source.len()).collect();
    if source.contains('#') {
        let steps: [&dyn Fn(&str) -> Edits; 3] = [
            &|text| color_semicolons(text, "style"),
            &|text| color_semicolons(text, "classDef"),
            &entity_codes,
        ];
        for step in steps {
            let edits = step(&text);
            if edits.is_empty() {
                continue;
            }
            let (next, map) = rewrite(&text, &edits);
            origin = map.into_iter().map(|index| origin[index]).collect();
            text = next;
        }
    }
    text.push('\n');
    origin.push(source.len());
    Encoded { text, origin }
}

fn locate(source: &str, encoded: &Encoded, offset: usize) -> (usize, usize) {
    let end = encoded.text.trim_end_matches(is_space).len();
    let at = if end > 0 && offset >= end {
        let last = encoded.text[..end]
            .chars()
            .next_back()
            .map_or(1, char::len_utf8);
        let mapped = encoded.origin[end - last];
        mapped + char_at(source, mapped).map_or(1, char::len_utf8)
    } else {
        encoded.origin[offset.min(encoded.origin.len() - 1)]
    }
    .min(source.len());
    let line_start = source[..at].rfind('\n').map_or(0, |index| index + 1);
    (
        source[..at].matches('\n').count() + 1,
        source[line_start..at].encode_utf16().count() + 1,
    )
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GanttTask {
    pub id: String,
    pub task: String,
    pub section: String,
    pub start: Option<f64>,
    pub end: Option<f64>,
    pub render_end: Option<f64>,
    pub manual_end: bool,
    pub processed: bool,
    pub active: bool,
    pub done: bool,
    pub crit: bool,
    pub milestone: bool,
    pub vert: bool,
    pub order: Option<usize>,
    pub classes: Vec<String>,
    pub previous: Option<String>,
    pub raw_start: Option<String>,
    pub raw_end: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GanttModel {
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub date_format: String,
    pub axis_format: String,
    pub tick_interval: Option<String>,
    pub today_marker: String,
    pub includes: Vec<String>,
    pub excludes: Vec<String>,
    pub inclusive_end_dates: bool,
    pub top_axis: bool,
    pub display_mode: String,
    pub weekday: Option<String>,
    pub weekend: String,
    pub sections: Vec<String>,
    pub links: IndexMap<String, String>,
    pub tasks: Vec<GanttTask>,
    by_id: HashMap<String, usize>,
    current_section: String,
    task_count: usize,
    last_order: usize,
    last_task_id: Option<String>,
}

fn merge_tokens(existing: &mut Vec<String>, value: &str) {
    for token in value
        .to_lowercase()
        .split(|ch: char| ch == ',' || is_space(ch))
        .filter(|token| !token.is_empty())
    {
        if !existing.iter().any(|known| known == token) {
            existing.push(token.to_owned());
        }
    }
}

fn tag(value: &str) -> Option<&'static str> {
    ["active", "done", "crit", "milestone", "vert"]
        .into_iter()
        .find(|tag| js_trim(value) == *tag)
}

impl GanttModel {
    fn new() -> Self {
        Self {
            weekend: "saturday".into(),
            ..Self::default()
        }
    }

    fn add_task(&mut self, descr: &str, data: &str) -> Result<(), PeleError> {
        if self.tasks.len() >= MAX_TASKS {
            return Err(PeleError::limit(format!(
                "Gantt chart exceeds the limit of {MAX_TASKS} tasks."
            )));
        }
        let mut fields: Vec<&str> = data.strip_prefix(':').unwrap_or(data).split(',').collect();
        let mut task = GanttTask {
            task: descr.to_owned(),
            section: self.current_section.clone(),
            previous: self.last_task_id.clone(),
            ..GanttTask::default()
        };
        while let Some(found) = fields.first().and_then(|field| tag(field)) {
            match found {
                "active" => task.active = true,
                "done" => task.done = true,
                "crit" => task.crit = true,
                "milestone" => task.milestone = true,
                _ => task.vert = true,
            }
            fields.remove(0);
        }
        let name = js_trim(descr);
        if fields.is_empty() {
            return Err(PeleError::semantic(format!(
                "Task \"{name}\" needs a duration or an end date."
            )));
        }
        if fields.len() > 3 {
            return Err(PeleError::semantic(format!(
                "Task \"{name}\" has too many fields. A task takes an id, a start, and an end or duration."
            )));
        }
        let fields: Vec<&str> = fields.into_iter().map(js_trim).collect();
        if fields.len() == 3 {
            task.id = fields[0].to_owned();
        } else {
            self.task_count += 1;
            task.id = format!("task{}", self.task_count);
        }
        if fields.len() > 1 {
            task.raw_start = Some(fields[fields.len() - 2].to_owned());
        }
        task.raw_end = fields[fields.len() - 1].to_owned();
        if !task.vert {
            task.order = Some(self.last_order);
            self.last_order += 1;
        }
        self.last_task_id = Some(task.id.clone());
        self.by_id.insert(task.id.clone(), self.tasks.len());
        self.tasks.push(task);
        Ok(())
    }

    fn set_class(&mut self, ids: &str, class: &str) {
        for id in ids.split(',') {
            if let Some(&index) = self.by_id.get(id) {
                self.tasks[index].classes.push(class.to_owned());
            }
        }
    }

    fn set_link(&mut self, ids: &str, link: &str) {
        if let Some(href) = safe_href(link) {
            for id in ids.split(',') {
                if self.by_id.contains_key(id) {
                    self.links.insert(id.to_owned(), href.clone());
                }
            }
        }
        self.set_class(ids, "clickable");
    }

    pub fn schedule(&self, now: f64) -> Result<Vec<GanttTask>, PeleError> {
        let mut tasks = self.tasks.clone();
        Calendar::new(self, now)
            .schedule(&mut tasks, &self.by_id)
            .map_err(written)?;
        Ok(tasks)
    }
}

fn written_codes(value: &str) -> String {
    static CODE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"ﬂ°°?([A-Za-z0-9_]+)¶ß").expect("valid code pattern"));
    CODE.replace_all(value, "#$1;").into_owned()
}

fn safe_href(value: &str) -> Option<String> {
    let original = text::decode_entities(value).trim().to_owned();
    if original.is_empty() {
        return None;
    }
    let mut decoded = original.clone();
    for _ in 0..32 {
        if decoded.starts_with("//")
            || decoded.contains('\\')
            || decoded
                .chars()
                .any(|ch| ch.is_control() || matches!(ch, '\u{2000}'..='\u{200f}' | '\u{feff}'))
        {
            return None;
        }
        if let Some(colon) = decoded.find(':') {
            let prefix = &decoded[..colon];
            if !prefix.contains(['/', '?', '#'])
                && !matches!(
                    prefix.to_ascii_lowercase().as_str(),
                    "http" | "https" | "mailto" | "tel"
                )
            {
                return None;
            }
        }
        let bytes = decoded.as_bytes();
        let mut unwrapped = Vec::with_capacity(bytes.len());
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] == b'%' && index + 2 < bytes.len() {
                let high = (bytes[index + 1] as char).to_digit(16);
                let low = (bytes[index + 2] as char).to_digit(16);
                if let (Some(high), Some(low)) = (high, low) {
                    unwrapped.push((high * 16 + low) as u8);
                    index += 3;
                    continue;
                }
            }
            unwrapped.push(bytes[index]);
            index += 1;
        }
        let unwrapped = text::decode_entities(std::str::from_utf8(&unwrapped).ok()?);
        if unwrapped == decoded {
            return Some(original);
        }
        decoded = unwrapped;
    }
    None
}

const LINE: &str = "'EOF', 'SPACE', 'NL', 'weekday_monday', 'weekday_tuesday', 'weekday_wednesday', 'weekday_thursday', 'weekday_friday', 'weekday_saturday', 'weekday_sunday', 'weekend_friday', 'weekend_saturday', 'dateFormat', 'inclusiveEndDates', 'topAxis', 'axisFormat', 'tickInterval', 'excludes', 'includes', 'todayMarker', 'title', 'acc_title', 'acc_descr', 'acc_descr_multiline_value', 'section', 'taskTxt', 'click'";

fn skip_chars(value: &str, count: usize) -> &str {
    value
        .char_indices()
        .nth(count)
        .map_or("", |(index, _)| &value[index..])
}

fn collapse_lines(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        output.push(ch);
        if ch == '\n' {
            while chars.next_if(|next| is_space(*next)).is_some() {}
        }
    }
    output
}

fn written(mut error: PeleError) -> PeleError {
    error.message = written_codes(&error.message);
    error
}

pub fn parse(source: &str) -> Result<GanttModel, PeleError> {
    parse_model(source).map_err(written)
}

fn parse_model(source: &str) -> Result<GanttModel, PeleError> {
    let encoded = encode(source);
    let tokens = tokenize(&encoded.text);
    let text = |index: usize| &encoded.text[tokens[index].start..tokens[index].end];
    let fail = |index: usize, expected: &str| {
        let (line, column) = locate(source, &encoded, tokens[index].start);
        PeleError::syntax(
            format!("Expecting {expected}, got '{}'", tokens[index].kind.name()),
            line,
            column,
        )
    };
    let end = |index: usize, also: &str| {
        if tokens[index].kind.starts_line() {
            Ok(())
        } else {
            Err(fail(index, &format!("{LINE}{also}")))
        }
    };
    let mut model = GanttModel::new();
    if tokens[0].kind != Kind::Gantt {
        return Err(fail(0, "'gantt'"));
    }
    let mut i = 1;
    loop {
        let kind = tokens[i].kind;
        let value = text(i);
        if !kind.starts_line() {
            return Err(fail(i, LINE));
        }
        i += 1;
        match kind {
            Kind::Eof => {
                if tokens[i].kind == Kind::End {
                    return Ok(model);
                }
                end(i, "")?;
            }
            Kind::Nl => {}
            Kind::DateFormat => {
                end(i, "")?;
                model.date_format = skip_chars(value, 11).to_owned();
            }
            Kind::InclusiveEndDates => {
                end(i, "")?;
                model.inclusive_end_dates = true;
            }
            Kind::TopAxis => {
                end(i, "")?;
                model.top_axis = true;
            }
            Kind::AxisFormat => {
                end(i, "")?;
                model.axis_format = skip_chars(value, 11).to_owned();
            }
            Kind::TickInterval => {
                end(i, "")?;
                model.tick_interval = Some(skip_chars(value, 13).to_owned());
            }
            Kind::Excludes => {
                end(i, "")?;
                merge_tokens(&mut model.excludes, skip_chars(value, 9));
            }
            Kind::Includes => {
                end(i, "")?;
                merge_tokens(&mut model.includes, skip_chars(value, 9));
            }
            Kind::TodayMarker => {
                end(i, "")?;
                model.today_marker = skip_chars(value, 12).to_owned();
            }
            Kind::WeekendFriday => {
                end(i, "")?;
                model.weekend = "friday".into();
            }
            Kind::WeekendSaturday => {
                end(i, "")?;
                model.weekend = "saturday".into();
            }
            Kind::Title => {
                end(i, "")?;
                model.title = Some(skip_chars(value, 6).to_owned());
            }
            Kind::AccTitle | Kind::AccDescr => {
                let expected = if kind == Kind::AccTitle {
                    Kind::AccTitleValue
                } else {
                    Kind::AccDescrValue
                };
                if tokens[i].kind != expected {
                    return Err(fail(i, &format!("'{}'", expected.name())));
                }
                let content = js_trim(text(i)).to_owned();
                i += 1;
                end(i, "")?;
                if kind == Kind::AccTitle {
                    model.acc_title = Some(content);
                } else {
                    model.acc_descr = Some(collapse_lines(&content));
                }
            }
            Kind::AccDescrMultilineValue => {
                end(i, "")?;
                model.acc_descr = Some(collapse_lines(js_trim(value)));
            }
            Kind::Section => {
                end(i, "")?;
                model.current_section = skip_chars(value, 8).to_owned();
                model.sections.push(model.current_section.clone());
            }
            Kind::TaskTxt => {
                if tokens[i].kind != Kind::TaskData {
                    return Err(fail(i, "'taskData'"));
                }
                let data = text(i);
                i += 1;
                end(i, "")?;
                model.add_task(value, data)?;
            }
            Kind::Click => {
                let mut name = None;
                let mut href = None;
                if tokens[i].kind == Kind::Callbackname {
                    name = Some(text(i));
                    i += 1;
                    let mut args = false;
                    if tokens[i].kind == Kind::Callbackargs {
                        args = true;
                        i += 1;
                    }
                    if tokens[i].kind == Kind::Href {
                        href = Some(text(i));
                        i += 1;
                        end(i, "")?;
                    } else {
                        end(
                            i,
                            if args {
                                ", 'href'"
                            } else {
                                ", 'callbackargs', 'href'"
                            },
                        )?;
                    }
                } else if tokens[i].kind == Kind::Href {
                    href = Some(text(i));
                    i += 1;
                    if tokens[i].kind == Kind::Callbackname {
                        name = Some(text(i));
                        i += 1;
                        if tokens[i].kind == Kind::Callbackargs {
                            i += 1;
                            end(i, "")?;
                        } else {
                            end(i, ", 'callbackargs'")?;
                        }
                    } else {
                        end(i, ", 'callbackname'")?;
                    }
                } else {
                    return Err(fail(i, "'callbackname', 'href'"));
                }
                if name.is_some() {
                    model.set_class(value, "clickable");
                }
                if let Some(href) = href {
                    model.set_link(value, href);
                }
            }
            Kind::Weekday(day) => {
                end(i, "")?;
                model.weekday = Some(DAY_NAMES[day].to_owned());
            }
            _ => return Err(fail(i - 1, LINE)),
        }
    }
}

fn greater(left: Option<f64>, right: Option<f64>) -> bool {
    matches!((left, right), (Some(left), Some(right)) if left > right)
}

fn reference_list<'a>(value: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = value.strip_prefix(keyword)?;
    let spaces: Vec<(usize, char)> = rest
        .char_indices()
        .take_while(|(_, ch)| is_space(*ch))
        .collect();
    let &(last, last_char) = spaces.last()?;
    let class = |ch: char| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | ' ');
    let after = last + last_char.len_utf8();
    let begin = if rest[after..].chars().next().is_some_and(class) {
        after
    } else {
        spaces[1..].iter().rev().find(|(_, ch)| *ch == ' ')?.0
    };
    let end = rest[begin..]
        .char_indices()
        .find(|(_, ch)| !class(*ch))
        .map_or(rest.len(), |(index, _)| begin + index);
    Some(&rest[begin..end])
}

struct Calendar {
    template: String,
    now: f64,
    work: f64,
    includes: HashSet<String>,
    excludes: HashSet<String>,
    has_excludes: bool,
    inclusive: bool,
    excluded_days: [bool; 7],
    dated: bool,
    templates: HashMap<String, Template>,
}

impl Calendar {
    fn new(model: &GanttModel, now: f64) -> Self {
        let weekend_start = match model.weekend.as_str() {
            "friday" => 5,
            "saturday" => 6,
            _ => -10,
        };
        let excludes: HashSet<String> = model.excludes.iter().cloned().collect();
        let weekends = excludes.contains("weekends");
        let mut excluded_days = [false; 7];
        for (day, excluded) in excluded_days.iter_mut().enumerate() {
            let iso = if day == 0 { 7 } else { day as i32 };
            *excluded = (weekends && (iso == weekend_start || iso == weekend_start + 1))
                || excludes.contains(&WEEKDAYS[day].to_lowercase());
        }
        let template = js_trim(&model.date_format).to_owned();
        let numeric = template.chars().all(|ch| "YMDHhmsSZXx-/.:, T".contains(ch));
        let dated = !model.includes.is_empty()
            || !numeric
            || model
                .excludes
                .iter()
                .any(|token| token != "weekends" && !DAY_NAMES.contains(&token.as_str()));
        Self {
            template,
            now,
            work: 0.0,
            includes: model.includes.iter().cloned().collect(),
            excludes,
            has_excludes: !model.excludes.is_empty(),
            inclusive: model.inclusive_end_dates,
            excluded_days,
            dated,
            templates: HashMap::new(),
        }
    }

    fn spend(&mut self, units: f64) -> Result<(), PeleError> {
        self.work += units;
        if self.work > WORK_LIMIT {
            return Err(PeleError::limit(
                "The gantt chart takes too much work to schedule.",
            ));
        }
        Ok(())
    }

    fn parse(&mut self, input: &str, template: &str) -> Result<Option<f64>, PeleError> {
        let units = |value: &str| value.encode_utf16().count() as f64;
        self.spend((units(template) + 1.0) * (units(input) + 1.0))?;
        let now = self.now;
        Ok(self
            .templates
            .entry(template.to_owned())
            .or_insert_with(|| Template::new(template))
            .parse(input, template, now))
    }

    fn today(&self) -> f64 {
        start_of_day(self.now)
    }

    fn invalid(&mut self, time: f64) -> Result<bool, PeleError> {
        let weekday = parts(time).weekday;
        let weekday = !weekday.is_nan() && self.excluded_days[weekday as usize];
        if !self.dated {
            return Ok(weekday);
        }
        let date = iso_date(time);
        self.spend(DATED_STEP)?;
        let formatted = if self.template == "YYYY-MM-DD" {
            Some(date.clone())
        } else {
            self.spend(FORMAT_STEP + self.template.encode_utf16().count() as f64 * 8.0)?;
            let template = self.template.clone();
            self.templates
                .entry(template)
                .or_insert_with_key(|template| Template::new(template))
                .format
                .format(time)
        };
        let listed = |set: &HashSet<String>| {
            set.contains(&date) || formatted.as_ref().is_some_and(|value| set.contains(value))
        };
        if listed(&self.includes) {
            return Ok(false);
        }
        Ok(weekday || listed(&self.excludes))
    }

    fn schedule(
        &mut self,
        tasks: &mut [GanttTask],
        by_id: &HashMap<String, usize>,
    ) -> Result<(), PeleError> {
        for task in tasks.iter_mut() {
            task.start = None;
            task.end = None;
            task.render_end = None;
            task.processed = false;
            task.manual_end = false;
        }
        for _ in 0..11 {
            let mut all = true;
            for index in 0..tasks.len() {
                self.compile_task(tasks, by_id, index)?;
                all = all && tasks[index].processed;
            }
            if all {
                break;
            }
        }
        Ok(())
    }

    fn references(by_id: &HashMap<String, usize>, list: &str) -> Vec<usize> {
        list.split(' ')
            .filter(|id| !id.is_empty())
            .filter_map(|id| by_id.get(id).copied())
            .collect()
    }

    fn compile_task(
        &mut self,
        tasks: &mut [GanttTask],
        by_id: &HashMap<String, usize>,
        index: usize,
    ) -> Result<(), PeleError> {
        match tasks[index].raw_start.clone() {
            None => {
                let previous = tasks[index].previous.as_ref().and_then(|id| by_id.get(id));
                tasks[index].start = match previous {
                    Some(&previous) => tasks[previous].end,
                    None => Some(self.today()),
                };
            }
            Some(raw) => {
                if let Some(start) = self.start_date(tasks, by_id, &raw)? {
                    tasks[index].start = Some(start);
                }
            }
        }
        let Some(start) = tasks[index].start else {
            return Ok(());
        };
        let raw_end = tasks[index].raw_end.clone();
        let end = self.end_date(tasks, by_id, start, &raw_end)?;
        tasks[index].end = end;
        if end.is_none() {
            return Ok(());
        }
        tasks[index].processed = true;
        tasks[index].manual_end = self.parse(&raw_end, "YYYY-MM-DD")?.is_some();
        self.check_task_dates(&mut tasks[index])
    }

    fn start_date(
        &mut self,
        tasks: &[GanttTask],
        by_id: &HashMap<String, usize>,
        raw: &str,
    ) -> Result<Option<f64>, PeleError> {
        let value = js_trim(raw);
        let template = self.template.clone();
        if (template == "x" || template == "X")
            && !value.is_empty()
            && value.bytes().all(|byte| byte.is_ascii_digit())
        {
            let time = time_clip(js_number(value));
            if time.is_nan() {
                return Err(PeleError::semantic(format!("Invalid date:{value}")));
            }
            return Ok(Some(time));
        }
        if let Some(list) = reference_list(value, "after") {
            let mut latest: Option<usize> = None;
            for found in Self::references(by_id, list) {
                if latest.is_none_or(|latest| greater(tasks[found].end, tasks[latest].end)) {
                    latest = Some(found);
                }
            }
            return Ok(match latest {
                Some(latest) => tasks[latest].end,
                None => Some(self.today()),
            });
        }
        if let Some(time) = self.parse(value, &template)? {
            return Ok(Some(time));
        }
        let time = parse_js_date(value);
        let year = parts(time).year;
        if time.is_nan() || !(-10000.0..=10000.0).contains(&year) {
            return Err(PeleError::semantic(format!("Invalid date:{value}")));
        }
        Ok(Some(time))
    }

    fn end_date(
        &mut self,
        tasks: &[GanttTask],
        by_id: &HashMap<String, usize>,
        start: f64,
        raw: &str,
    ) -> Result<Option<f64>, PeleError> {
        let value = js_trim(raw);
        if let Some(list) = reference_list(value, "until") {
            let mut earliest: Option<usize> = None;
            for found in Self::references(by_id, list) {
                if earliest
                    .is_none_or(|earliest| greater(tasks[earliest].start, tasks[found].start))
                {
                    earliest = Some(found);
                }
            }
            return Ok(match earliest {
                Some(earliest) => tasks[earliest].start,
                None => Some(self.today()),
            });
        }
        let template = self.template.clone();
        if let Some(time) = self.parse(value, &template)? {
            return Ok(Some(if self.inclusive {
                add_days(time, 1.0)
            } else {
                time
            }));
        }
        let Some((amount, unit)) = parse_duration(value) else {
            return Ok(Some(start));
        };
        let end = add(start, amount, unit);
        Ok(Some(if end.is_nan() { start } else { end }))
    }

    fn check_task_dates(&mut self, task: &mut GanttTask) -> Result<(), PeleError> {
        let (Some(start), Some(mut end)) = (task.start, task.end) else {
            return Ok(());
        };
        if !self.has_excludes || task.manual_end {
            return Ok(());
        }
        let mut cursor = add_days(start, 1.0);
        let max_end = add_days(end, 10000.0);
        let mut invalid = false;
        let mut render_end = None;
        while cursor <= end {
            self.spend(DAY_STEP)?;
            if !invalid {
                render_end = Some(end);
            }
            invalid = self.invalid(cursor)?;
            if invalid {
                end = add_days(end, 1.0);
                if end > max_end {
                    return Err(PeleError::semantic(
                        "Failed to find a valid date that was not excluded by `excludes` after 10,000 iterations.",
                    ));
                }
            }
            cursor = add_days(cursor, 1.0);
        }
        task.end = Some(end);
        task.render_end = render_end;
        Ok(())
    }
}

fn scheduled(task: &GanttTask) -> bool {
    task.start.is_some_and(f64::is_finite) && task.end.is_some_and(f64::is_finite)
}

fn pack_rows(tasks: &[GanttTask], group: &[usize], offset: usize, row_of: &mut [usize]) -> usize {
    let mut sorted: Vec<usize> = group
        .iter()
        .copied()
        .filter(|index| scheduled(&tasks[*index]))
        .collect();
    sorted.sort_by(|left, right| {
        let (left, right) = (&tasks[*left], &tasks[*right]);
        left.start
            .unwrap_or(0.0)
            .total_cmp(&right.start.unwrap_or(0.0))
            .then(left.order.cmp(&right.order))
    });
    let mut free: Vec<f64> = Vec::new();
    let mut placed = HashSet::new();
    for &index in &sorted {
        let start = tasks[index].start.unwrap_or(0.0);
        let end = tasks[index].end.unwrap_or(0.0);
        let row = free.iter().take_while(|busy| start < **busy).count();
        if row == free.len() {
            free.push(end);
        } else {
            free[row] = end;
        }
        row_of[index] = offset + row;
        placed.insert(index);
    }
    let mut rows = free.len();
    for &index in group {
        if !placed.contains(&index) {
            row_of[index] = offset + rows;
            rows += 1;
        }
    }
    rows.max(1)
}

fn one_line(raw: &str, measurer: &dyn text::TextMeasurer, size: f64) -> text::Label {
    let flat = BREAK.replace_all(raw, " ");
    let mut label = text::layout_label(&flat, false, measurer, size, 4000.0, 0);
    if label.lines.len() > 1 {
        label.lines.truncate(1);
        label.widths.truncate(1);
        label.width = label.widths[0];
        label.height = label.line_height;
    }
    label
}

struct Bar {
    index: usize,
    label: text::Label,
    row: usize,
    x0: f64,
    x1: f64,
    label_x: f64,
    inside: bool,
}

pub fn render(source: &str, options: &RenderOptions) -> Result<RenderResult, PeleError> {
    let mut model = parse(source)?;
    if let Some(mode) = options.config["gantt"]["displayMode"].as_str() {
        mode.clone_into(&mut model.display_mode);
    }
    if model.title.is_none() {
        model.title = options.title.clone().filter(|title| !title.is_empty());
    }
    draw(&model, options).map_err(written)
}

fn draw(model: &GanttModel, options: &RenderOptions) -> Result<RenderResult, PeleError> {
    let fallback = text::MetricsMeasurer;
    let measurer = options.measurer.as_deref().unwrap_or(&fallback);
    let label_svg = |label: &text::Label, x: f64, y: f64, attrs: &str| {
        svg::label_with_options(label, x, y, attrs, options)
    };
    let theme = &options.theme;
    let size = options.font_size;
    let pad = options.padding;
    let config = &options.config["gantt"];
    let setting = |key: &str, min: f64, max: f64| {
        config[key]
            .as_f64()
            .filter(|value| value.is_finite())
            .map(|value| value.max(min).min(max))
    };
    let label_size = setting("fontSize", 4.0, 72.0).unwrap_or_else(|| js_round(size * 0.8125));
    let section_size = setting("sectionFontSize", 4.0, 72.0).unwrap_or(label_size);
    let tick_size = js_round(size * 0.75);
    let bar_height =
        setting("barHeight", 4.0, 400.0).unwrap_or_else(|| js_round(label_size * 1.85));
    let bar_gap = setting("barGap", 0.0, 400.0).unwrap_or_else(|| js_round(bar_height / 4.0));
    let row_height = bar_height + bar_gap;
    let section_styles = js_round(setting("numberSectionStyles", 1.0, 1000.0).unwrap_or(4.0));
    let compact = model.display_mode == "compact";
    let top_axis = model.top_axis || config["topAxis"] == true;

    let mut calendar = Calendar::new(model, NOW);
    let mut tasks = model.tasks.clone();
    calendar.schedule(&mut tasks, &model.by_id)?;
    let rows: Vec<usize> = (0..tasks.len())
        .filter(|index| !tasks[*index].vert)
        .collect();
    let mut verts: Vec<usize> = (0..tasks.len())
        .filter(|index| tasks[*index].vert && scheduled(&tasks[*index]))
        .collect();

    let mut categories: IndexMap<&str, usize> = IndexMap::new();
    for &index in &rows {
        let next = categories.len();
        categories
            .entry(tasks[index].section.as_str())
            .or_insert(next);
    }

    let mut row_of = vec![0; tasks.len()];
    let mut row_section: Vec<&str> = Vec::new();
    if compact {
        let mut groups: IndexMap<&str, Vec<usize>> = IndexMap::new();
        for &index in &rows {
            groups
                .entry(tasks[index].section.as_str())
                .or_default()
                .push(index);
        }
        for (section, group) in &groups {
            let count = pack_rows(&tasks, group, row_section.len(), &mut row_of);
            row_section.extend(std::iter::repeat_n(*section, count));
        }
    } else {
        for &index in &rows {
            row_of[index] = row_section.len();
            row_section.push(&tasks[index].section);
        }
    }
    let row_count = row_section.len();

    let mut start = f64::INFINITY;
    let mut stop = f64::NEG_INFINITY;
    for task in tasks.iter().filter(|task| scheduled(task)) {
        start = start.min(task.start.unwrap_or(start));
        stop = stop.max(task.end.unwrap_or(stop));
    }

    let title = text::layout_label(
        model.title.as_deref().unwrap_or(""),
        false,
        measurer,
        size,
        4000.0,
        text::BOLD,
    );
    let title_height = if title.height > 0.0 {
        title.height + 16.0
    } else {
        0.0
    };
    let title_attrs = " class=\"pele-title\" font-weight=\"bold\"";
    let mut document_options = options.clone();
    document_options.title.clone_from(&model.title);
    document_options.acc_title = model.acc_title.clone().or(document_options.acc_title);
    document_options.acc_descr = model.acc_descr.clone().or(document_options.acc_descr);

    if start > stop {
        let width = (title.width.max(1.0) + 2.0 * pad).ceil();
        let height = (title.height.max(1.0) + 2.0 * pad).ceil();
        let content = label_svg(&title, width / 2.0, pad + title.height / 2.0, title_attrs);
        return Ok(RenderResult::new(
            "gantt",
            svg::document("gantt", width, height, &document_options, &content),
            width,
            height,
        ));
    }

    let mut section_labels: HashMap<&str, text::Label> = HashMap::new();
    let mut section_width: f64 = 0.0;
    for section in categories.keys() {
        let label = text::layout_label(section, false, measurer, section_size, 180.0, 0);
        section_width = section_width.max(label.width);
        section_labels.insert(section, label);
    }

    let axis_format = if !model.axis_format.is_empty() {
        model.axis_format.clone()
    } else if model.date_format == "D" {
        "%d".to_owned()
    } else {
        config["axisFormat"]
            .as_str()
            .unwrap_or("%Y-%m-%d")
            .to_owned()
    };
    let tick_label = |time: f64| time_format(&axis_format, time);
    let sample_width = measurer
        .width(&tick_label(start), tick_size, 0)
        .max(measurer.width(&tick_label(stop), tick_size, 0));

    let left = pad
        + setting("leftPadding", 0.0, 2000.0).unwrap_or_else(|| {
            (if section_width > 0.0 {
                section_width + 24.0
            } else {
                0.0
            })
            .max(sample_width / 2.0)
        });
    let right_padding =
        setting("rightPadding", 0.0, 2000.0).unwrap_or_else(|| (sample_width / 2.0).max(16.0));
    let natural = setting("useWidth", 100.0, 20000.0).unwrap_or(WIDTH);
    let target = options
        .max_width
        .filter(|available| *available > 0.0)
        .map_or(natural, |available| {
            natural.min(available).max(natural.min(280.0))
        });
    let plot_width = MIN_PLOT.max(target - left - right_padding - pad);
    let right = left + plot_width;
    let scale = if stop > start {
        plot_width / (stop - start)
    } else {
        0.0
    };
    let x = |time: f64| {
        if scale == 0.0 {
            left + plot_width / 2.0
        } else {
            left + (time - start) * scale
        }
    };

    let tick_height = js_round(tick_size * 1.75);
    let top_padding = setting("topPadding", 0.0, 2000.0);
    let grid_start = setting("gridLineStartPadding", 0.0, 2000.0);
    let head_height = pad + title_height + if top_axis { tick_height } else { 0.0 };
    let plot_top = head_height.max(top_padding.unwrap_or(0.0));
    let grid_lead = match (top_padding, grid_start) {
        (Some(top), Some(grid)) => (top - grid).min(plot_top - head_height).max(0.0),
        _ => 0.0,
    };
    let plot_height = row_count.max(1) as f64 * row_height;
    let plot_bottom = plot_top + plot_height;

    let mut bars = Vec::new();
    let mut extent = right + right_padding;
    for &index in &rows {
        let task = &tasks[index];
        let label = one_line(&task.task, measurer, label_size);
        let row = row_of[index];
        let times = scheduled(task).then(|| (task.start.unwrap_or(0.0), task.end.unwrap_or(0.0)));
        let Some((task_start, task_end)) = times else {
            let label_x = left + LABEL_GAP + label.width / 2.0;
            extent = extent.max(label_x + label.width / 2.0);
            bars.push(Bar {
                index,
                label,
                row,
                x0: f64::NAN,
                x1: f64::NAN,
                label_x,
                inside: false,
            });
            continue;
        };
        let mut x0 = x(task_start);
        let mut x1 =
            x(task.render_end.unwrap_or(task_end)).max(x0 + if task.milestone { 0.0 } else { 2.0 });
        if task.milestone {
            let mid = x0 + (x(task_end) - x0) / 2.0;
            x0 = mid - bar_height / 2.0;
            x1 = mid + bar_height / 2.0;
        }
        let inside = !task.milestone && label.width + 2.0 * LABEL_GAP <= x1 - x0;
        let mut label_x = f64::midpoint(x0, x1);
        if !inside {
            let fits_right = x1 + LABEL_GAP + label.width <= right + right_padding;
            let fits_left = x0 - LABEL_GAP - label.width >= left;
            label_x = if fits_right || !fits_left {
                x1 + LABEL_GAP + label.width / 2.0
            } else {
                x0 - LABEL_GAP - label.width / 2.0
            };
            extent = extent.max(label_x + label.width / 2.0);
        }
        bars.push(Bar {
            index,
            label,
            row,
            x0,
            x1,
            label_x,
            inside,
        });
    }

    let interval = model
        .tick_interval
        .as_deref()
        .filter(|value| !value.is_empty())
        .or_else(|| {
            config["tickInterval"]
                .as_str()
                .filter(|value| !value.is_empty())
        })
        .unwrap_or("");
    let weekday = model
        .weekday
        .as_deref()
        .or_else(|| config["weekday"].as_str())
        .unwrap_or("sunday");
    let count = (plot_width / (sample_width + 24.0))
        .floor()
        .clamp(2.0, 10.0);
    let ticks = interval_ticks(interval, weekday, start, stop, (plot_width / 3.0).ceil())
        .unwrap_or_else(|| auto_ticks(start, stop, count));

    let n = svg::num;
    let mut grid = String::new();
    let mut axis_bottom = String::new();
    let mut axis_top = String::new();
    let mut last_right = f64::NEG_INFINITY;
    let mut last_label = String::new();
    let bottom_y = plot_bottom + tick_height / 2.0 + 2.0;
    let top_y = plot_top - grid_lead - tick_height / 2.0 - 2.0;
    for &time in &ticks {
        let tx = x(time);
        let _ = write!(
            grid,
            "M{},{}V{}",
            n(tx),
            n(plot_top - grid_lead),
            n(plot_bottom)
        );
        let label = tick_label(time);
        let width = measurer.width(&label, tick_size, 0);
        if tx - width / 2.0 < last_right + 8.0 || label == last_label {
            continue;
        }
        last_right = tx + width / 2.0;
        extent = extent.max(last_right);
        let body = format!("text-anchor=\"middle\">{}</text>", svg::esc(&label));
        last_label = label;
        let _ = write!(
            axis_bottom,
            "<text x=\"{}\" y=\"{}\" {body}",
            n(tx),
            n(bottom_y + tick_size * 0.35)
        );
        if top_axis {
            let _ = write!(
                axis_top,
                "<text x=\"{}\" y=\"{}\" {body}",
                n(tx),
                n(top_y + tick_size * 0.35)
            );
        }
    }

    let mut vert_lines = String::new();
    let mut vert_labels = String::new();
    let mut vert_ends: Vec<f64> = Vec::new();
    let vert_top = plot_bottom + tick_height + 4.0;
    let vert_line = js_round(label_size * 1.5);
    verts.sort_by(|left, right| {
        tasks[*left]
            .start
            .unwrap_or(0.0)
            .total_cmp(&tasks[*right].start.unwrap_or(0.0))
    });
    for &index in &verts {
        let task = &tasks[index];
        let vx = x(task.start.unwrap_or(start));
        let label = one_line(&task.task, measurer, label_size);
        let cx = (pad + label.width / 2.0).max(vx);
        let line = vert_ends
            .iter()
            .take_while(|end| cx - label.width / 2.0 < **end + 8.0)
            .count();
        let reach = cx + label.width / 2.0;
        if line == vert_ends.len() {
            vert_ends.push(reach);
        } else {
            vert_ends[line] = reach;
        }
        extent = extent.max(reach);
        let id = svg::esc_text(&task.id);
        let _ = write!(
            vert_lines,
            "<path class=\"pele-vert\" data-id=\"{id}\" d=\"M{},{}V{}\"/>",
            n(vx),
            n(plot_top - grid_lead),
            n(plot_bottom)
        );
        vert_labels.push_str(&label_svg(
            &label,
            cx,
            vert_top + line as f64 * vert_line + vert_line / 2.0,
            &format!(" class=\"pele-vert-label\" data-id=\"{id}\""),
        ));
    }

    let width = (extent.max(pad + title.width) + pad).ceil();
    let height = (plot_bottom
        + tick_height
        + if vert_ends.is_empty() {
            0.0
        } else {
            4.0 + vert_ends.len() as f64 * vert_line
        }
        + pad)
        .ceil();

    let mut bands = String::new();
    let mut section_out = String::new();
    let mut row = 0;
    while row < row_count {
        let section = row_section[row];
        let end = row
            + row_section[row..]
                .iter()
                .take_while(|value| **value == section)
                .count();
        let index = categories.get(section).copied().unwrap_or(0) as f64 % section_styles;
        let y = plot_top + row as f64 * row_height;
        let h = (end - row) as f64 * row_height;
        if categories.len() > 1 || !section.is_empty() {
            let _ = write!(
                bands,
                "<rect class=\"pele-section pele-section-{}\" x=\"0\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"/>",
                js_string(index),
                n(y),
                n(width),
                n(h),
                svg::esc(if index % 2.0 == 0.0 {
                    &theme.surface
                } else {
                    "none"
                })
            );
        }
        if let Some(label) = section_labels.get(section) {
            section_out.push_str(&label_svg(
                label,
                pad + 8.0 + label.width / 2.0,
                y + h / 2.0,
                &format!(
                    " class=\"pele-section-label\" data-id=\"{}\"",
                    svg::esc_text(section)
                ),
            ));
        }
        row = end;
    }

    let mut excluded = String::new();
    if (!model.excludes.is_empty() || !model.includes.is_empty())
        && add(start, 6.0, Unit::Year) > stop
    {
        let mut flush = |from: &mut Option<f64>, to: &mut Option<f64>| {
            if let (Some(first), Some(last)) = (*from, *to) {
                let ex0 = left.max(x(start_of_day(first)));
                let ex1 = right.min(x(set_hours(last, 23.0, 59.0, 59.0, 999.0)));
                if ex1 > ex0 {
                    let _ = write!(
                        excluded,
                        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>",
                        n(ex0),
                        n(plot_top),
                        n(ex1 - ex0),
                        n(plot_height)
                    );
                }
            }
            *from = None;
            *to = None;
        };
        let mut from = None;
        let mut to = None;
        let mut day = start;
        while day <= stop {
            if calendar.invalid(day)? {
                from.get_or_insert(day);
                to = Some(day);
            } else {
                flush(&mut from, &mut to);
            }
            day = add_days(day, 1.0);
        }
        flush(&mut from, &mut to);
    }

    let critical = &theme.series[3 % theme.series.len().max(1)];
    let mut links = Vec::new();
    let mut tasks_out = String::new();
    for bar in &bars {
        let task = &tasks[bar.index];
        let cy = plot_top + bar.row as f64 * row_height + row_height / 2.0;
        let state = if task.active {
            "active"
        } else if task.done {
            "done"
        } else {
            ""
        };
        let mut classes = String::from("pele-task");
        if !state.is_empty() {
            classes.push_str(" pele-");
            classes.push_str(state);
        }
        if task.crit {
            classes.push_str(" pele-crit");
        }
        if task.milestone {
            classes.push_str(" pele-milestone");
        }
        if task.classes.iter().any(|class| class == "clickable") {
            classes.push_str(" pele-clickable");
        }
        let mut shape = String::new();
        let mut text_fill = String::new();
        if bar.x0.is_nan() {
            classes.push_str(" pele-unscheduled");
            text_fill = format!(" fill=\"{}\"", svg::esc(&theme.muted));
        } else {
            let color = if task.crit { critical } else { &theme.accent };
            let paint = match state {
                "done" => format!("fill=\"{}\"", svg::esc(&theme.surface_alt)),
                "active" => format!("fill=\"{}\" fill-opacity=\"0.25\"", svg::esc(color)),
                _ => format!("fill=\"{}\"", svg::esc(color)),
            };
            let stroke = format!(
                "stroke=\"{}\"",
                svg::esc(if state == "done" && !task.crit {
                    &theme.border
                } else {
                    color
                })
            );
            if task.milestone {
                let cx = f64::midpoint(bar.x0, bar.x1);
                let r = bar_height / 2.0;
                shape = format!(
                    "<path d=\"M{},{}L{},{}L{},{}L{},{}Z\" {paint} {stroke} stroke-linejoin=\"round\"/>",
                    n(cx),
                    n(cy - r),
                    n(cx + r),
                    n(cy),
                    n(cx),
                    n(cy + r),
                    n(cx - r),
                    n(cy)
                );
            } else {
                shape = format!(
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" {paint} {stroke} rx=\"{}\"/>",
                    n(bar.x0),
                    n(cy - bar_height / 2.0),
                    n(bar.x1 - bar.x0),
                    n(bar_height),
                    n(theme.radius)
                );
            }
            if bar.inside {
                if state.is_empty() {
                    text_fill = format!(" fill=\"{}\"", svg::esc(&theme.background));
                }
            } else if state == "done" {
                text_fill = format!(" fill=\"{}\"", svg::esc(&theme.muted));
            }
        }
        let mut body = shape
            + &label_svg(
                &bar.label,
                bar.label_x,
                cy,
                &format!(" class=\"pele-label\"{text_fill}"),
            );
        if let Some(href) = model.links.get(&task.id) {
            body = format!("<a href=\"{}\" rel=\"noopener\">{body}</a>", svg::esc(href));
            links.push(LinkInfo {
                id: text::decode_entities(&task.id),
                href: href.clone(),
                internal: false,
            });
        }
        let _ = write!(
            tasks_out,
            "<g class=\"{classes}\" data-id=\"{}\">{body}</g>",
            svg::esc_text(&task.id)
        );
    }

    let mut axis_line = format!("M{},{}H{}", n(left), n(plot_bottom), n(right));
    if top_axis {
        let _ = write!(
            axis_line,
            "M{},{}H{}",
            n(left),
            n(plot_top - grid_lead),
            n(right)
        );
    }
    let mut content = label_svg(&title, width / 2.0, pad + title.height / 2.0, title_attrs);
    if !bands.is_empty() {
        let _ = write!(content, "<g class=\"pele-sections\">{bands}</g>");
    }
    if !excluded.is_empty() {
        let _ = write!(
            content,
            "<g class=\"pele-excluded\" fill=\"{}\">{excluded}</g>",
            svg::esc(&theme.surface_alt)
        );
    }
    let border = svg::esc(&theme.border);
    let _ = write!(
        content,
        "<g class=\"pele-grid\" fill=\"none\"><path d=\"{grid}\" stroke=\"{border}\" stroke-dasharray=\"1 3\"/><path d=\"{axis_line}\" stroke=\"{border}\"/></g>"
    );
    if !vert_lines.is_empty() {
        let _ = write!(
            content,
            "<g class=\"pele-verts\" fill=\"none\" stroke=\"{}\" stroke-width=\"1.5\" stroke-dasharray=\"4 3\">{vert_lines}</g>",
            svg::esc(&theme.line)
        );
    }
    let _ = write!(
        content,
        "<g class=\"pele-tasks\" font-size=\"{}\">{tasks_out}</g><g class=\"pele-section-labels\" font-size=\"{}\">{section_out}</g><g class=\"pele-axis\" font-size=\"{}\" fill=\"{}\">{axis_bottom}{axis_top}</g>",
        n(label_size),
        n(section_size),
        n(tick_size),
        svg::esc(&theme.muted)
    );
    if !vert_labels.is_empty() {
        let _ = write!(
            content,
            "<g class=\"pele-vert-labels\" font-size=\"{}\">{vert_labels}</g>",
            n(label_size)
        );
    }
    let mut result = RenderResult::new(
        "gantt",
        svg::document("gantt", width, height, &document_options, &content),
        width,
        height,
    );
    result.links = links;
    Ok(result)
}
