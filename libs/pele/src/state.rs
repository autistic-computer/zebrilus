use crate::compound::{CEdge, CNode, Dir, cnode, compound_layout, direction};
use crate::fit_width::{runs_across, turn_to_fit};
use crate::flow_shapes::end_side;
use crate::layout::LayeredOptions;
use crate::svg::cluster::js_round;
use crate::svg::edges::{
    EdgePath, Fill, casings_svg, edge_casing, edge_label_svg, marker, marker_trim, over_fills,
    route_path, side_reach,
};
use crate::svg::ink::{ink_fill, paints};
use crate::svg::theme::{ResolvedStyle, class_names, resolve_style, sets_fill};
use crate::svg::title::with_title;
use crate::text::{self, Label};
use crate::{LinkInfo, PeleError, RenderOptions, RenderResult, svg};
use std::collections::HashMap;
use std::fmt::Write;

const MAX_DEPTH: usize = 128;
const MAX_STATES: usize = 10_000;
const NODE_SEP: f64 = 40.0;
const EDGE_SEP: f64 = 16.0;
const RANK_SEP: f64 = 44.0;
const GROUP_PAD: f64 = 16.0;
const REGION_PAD: f64 = 10.0;
const PAD_X: f64 = 16.0;
const PAD_Y: f64 = 10.0;
const LOOP: f64 = 26.0;
const FOLD: f64 = 8.0;
const NOTE_WRAP: f64 = 320.0;
const CORNER: f64 = 10.0;
const BAR: f64 = 8.0;
const NONE: u32 = u32::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum T {
    End,
    Error,
    Nl,
    Sd,
    Descr,
    Arrow,
    HideEmpty,
    Scale,
    Width,
    CompositState,
    StructStart,
    StructStop,
    StateDescr,
    As,
    Id,
    Fork,
    Join,
    Choice,
    Concurrent,
    Note,
    NoteText,
    AccTitle,
    AccTitleValue,
    AccDescr,
    AccDescrValue,
    AccDescrMultilineValue,
    Click,
    Str,
    Href,
    ClassDef,
    ClassDefId,
    DefaultClassDefId,
    ClassDefStyleOpts,
    Default,
    Style,
    StyleIds,
    StyleDefStyleOpts,
    Class,
    ClassEntityIds,
    StyleClass,
    DirectionTb,
    DirectionBt,
    DirectionRl,
    DirectionLr,
    EdgeState,
    StyleSeparator,
    LeftOf,
    RightOf,
    Invalid,
}

const TOKEN_NAMES: [&str; 49] = [
    "$end",
    "INVALID",
    "NL",
    "SD",
    "DESCR",
    "-->",
    "HIDE_EMPTY",
    "scale",
    "WIDTH",
    "COMPOSIT_STATE",
    "STRUCT_START",
    "STRUCT_STOP",
    "STATE_DESCR",
    "AS",
    "ID",
    "FORK",
    "JOIN",
    "CHOICE",
    "CONCURRENT",
    "note",
    "NOTE_TEXT",
    "acc_title",
    "acc_title_value",
    "acc_descr",
    "acc_descr_value",
    "acc_descr_multiline_value",
    "CLICK",
    "STRING",
    "HREF",
    "classDef",
    "CLASSDEF_ID",
    "DEFAULT_CLASSDEF_ID",
    "CLASSDEF_STYLEOPTS",
    "DEFAULT",
    "style",
    "STYLE_IDS",
    "STYLEDEF_STYLEOPTS",
    "class",
    "CLASSENTITY_IDS",
    "STYLECLASS",
    "direction_tb",
    "direction_bt",
    "direction_rl",
    "direction_lr",
    "EDGE_STATE",
    "STYLE_SEPARATOR",
    "left_of",
    "right_of",
    "INVALID",
];

const DIRECTION_TOKENS: [T; 4] = [
    T::DirectionTb,
    T::DirectionBt,
    T::DirectionRl,
    T::DirectionLr,
];
const DIRECTIONS: [&str; 4] = ["TB", "BT", "RL", "LR"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum S {
    Initial,
    Struct,
    State,
    StateString,
    StateId,
    Scale,
    AccTitle,
    AccDescr,
    AccDescrMultiline,
    ClassDef,
    ClassDefId,
    Class,
    ClassStyle,
    Style,
    StyleDefStyles,
    Note,
    NoteId,
    NoteText,
    FloatingNote,
    FloatingNoteId,
}

fn is_space(c: u32) -> bool {
    if c < 128 {
        return c == 32 || (9..=13).contains(&c);
    }
    matches!(
        c,
        0xa0 | 0x1680 | 0x2000..=0x200a | 0x2028 | 0x2029 | 0x202f | 0x205f | 0x3000 | 0xfeff
    )
}

fn is_word(c: u32) -> bool {
    matches!(c, 48..=57 | 65..=90 | 97..=122 | 95)
}

fn is_js_space(ch: char) -> bool {
    is_space(ch as u32)
}

fn js_trim(text: &str) -> &str {
    text.trim_matches(is_js_space)
}

fn js_sign(value: f64) -> f64 {
    if value > 0.0 {
        1.0
    } else if value < 0.0 {
        -1.0
    } else {
        0.0
    }
}

#[derive(Clone, Copy)]
enum Pattern {
    Direction,
    Stereotype,
    LineEnd,
    Percent,
    Brace,
}

fn lower_at(src: &[u16], at: usize, word: &str) -> bool {
    word.bytes().enumerate().all(|(i, a)| {
        src.get(at + i).is_some_and(|&c| {
            let c = u32::from(c);
            let a = u32::from(a);
            c == a || ((97..=122).contains(&a) && c == a - 32)
        })
    })
}

impl Pattern {
    fn at(self, src: &[u16], i: usize) -> bool {
        let c = u32::from(src[i]);
        match self {
            Self::Direction => lower_at(src, i, "direction"),
            Self::Stereotype => {
                let close = match c {
                    60 if src.get(i + 1) == Some(&60) => ">>",
                    91 if src.get(i + 1) == Some(&91) => "]]",
                    _ => return false,
                };
                ["fork", "join", "choice"].iter().any(|name| {
                    lower_at(src, i + 2, name)
                        && src.len() >= i + 2 + name.len() + 2
                        && src[i + 2 + name.len()..i + 4 + name.len()]
                            .iter()
                            .map(|&u| u32::from(u))
                            .eq(close.bytes().map(u32::from))
                })
            }
            Self::LineEnd => matches!(c, 10 | 13 | 0x2028 | 0x2029),
            Self::Percent => c == 37 && src.get(i + 1) == Some(&37),
            Self::Brace => c == 123,
        }
    }
}

struct Finder {
    pattern: Pattern,
    from: usize,
    next: Option<Option<usize>>,
}

impl Finder {
    fn new(pattern: Pattern) -> Self {
        Self {
            pattern,
            from: 0,
            next: None,
        }
    }

    fn find(&mut self, src: &[u16], q: usize) -> Option<usize> {
        let stale = match self.next {
            None => true,
            Some(next) => q < self.from || next.is_some_and(|next| q > next),
        };
        if stale {
            self.next = Some((q..src.len()).find(|&i| self.pattern.at(src, i)));
            self.from = q;
        }
        self.next.flatten()
    }
}

struct Tokens {
    types: Vec<T>,
    texts: Vec<String>,
    starts: Vec<usize>,
    error: Option<String>,
}

struct Lexer<'a> {
    src: &'a [u16],
    n: usize,
    p: usize,
    state: S,
    stack: Vec<S>,
    tokens: Tokens,
    directions: Finder,
    stereotypes: Finder,
    line_ends: Finder,
    percents: Finder,
    braces: Finder,
    dir_from: Option<usize>,
    dir_to: usize,
    dir_at: [Option<usize>; 4],
    dir_end: [usize; 4],
    stereo_from: Option<usize>,
    stereo_to: usize,
    stereo_at: [Option<usize>; 6],
}

fn utf16(src: &[u16]) -> String {
    String::from_utf16_lossy(src)
}

impl<'a> Lexer<'a> {
    fn new(src: &'a [u16]) -> Self {
        Self {
            src,
            n: src.len(),
            p: 0,
            state: S::Initial,
            stack: Vec::new(),
            tokens: Tokens {
                types: Vec::new(),
                texts: Vec::new(),
                starts: Vec::new(),
                error: None,
            },
            directions: Finder::new(Pattern::Direction),
            stereotypes: Finder::new(Pattern::Stereotype),
            line_ends: Finder::new(Pattern::LineEnd),
            percents: Finder::new(Pattern::Percent),
            braces: Finder::new(Pattern::Brace),
            dir_from: None,
            dir_to: 0,
            dir_at: [None; 4],
            dir_end: [0; 4],
            stereo_from: None,
            stereo_to: 0,
            stereo_at: [None; 6],
        }
    }

    fn c(&self, i: usize) -> u32 {
        self.src.get(i).map_or(NONE, |&u| u32::from(u))
    }

    fn slice(&self, s: usize, e: usize) -> String {
        if s >= e {
            String::new()
        } else {
            utf16(&self.src[s..e.min(self.n)])
        }
    }

    fn emit(&mut self, kind: T, s: usize, e: usize) {
        let text = self.slice(s, e);
        self.emit_text(kind, text, s, e);
    }

    fn emit_text(&mut self, kind: T, text: String, s: usize, e: usize) {
        self.tokens.types.push(kind);
        self.tokens.texts.push(text);
        self.tokens.starts.push(s);
        self.p = e;
    }

    fn push(&mut self, s: S) {
        self.stack.push(self.state);
        self.state = s;
    }

    fn pop(&mut self) {
        if let Some(s) = self.stack.pop() {
            self.state = s;
        }
    }

    fn word(&self, w: &str, q: usize) -> bool {
        lower_at(self.src, q, w)
    }

    fn ws_end(&self, q: usize) -> usize {
        let mut j = q;
        while j < self.n && is_space(self.c(j)) {
            j += 1;
        }
        j
    }

    fn word_end(&self, q: usize) -> usize {
        let mut j = q;
        while j < self.n && is_word(self.c(j)) {
            j += 1;
        }
        j
    }

    fn index_of(&self, unit: u16, q: usize) -> Option<usize> {
        self.src
            .get(q..)
            .and_then(|rest| rest.iter().position(|&c| c == unit))
            .map(|at| q + at)
    }

    fn eol(&self, q: usize) -> usize {
        self.index_of(10, q).unwrap_or(self.n)
    }

    fn line_end(&mut self, q: usize) -> usize {
        self.line_ends.find(self.src, q).unwrap_or(self.n)
    }

    fn direction(&mut self, q: usize) -> Option<usize> {
        if self.dir_from.is_none_or(|from| q < from) || q > self.dir_to {
            let first = self.directions.find(self.src, q);
            let end = if first.is_none() {
                self.n
            } else {
                self.line_end(q)
            };
            self.dir_from = Some(q);
            self.dir_to = end;
            self.dir_at = [None; 4];
            let mut d = first;
            while let Some(at) = d {
                if at >= end {
                    break;
                }
                let w = self.ws_end(at + 9);
                if w != at + 9 {
                    let kind = ["tb", "bt", "rl", "lr"]
                        .iter()
                        .position(|name| self.word(name, w));
                    if let Some(kind) = kind {
                        self.dir_at[kind] = Some(at);
                        self.dir_end[kind] = w + 2;
                    }
                }
                d = self.directions.find(self.src, at + 1);
            }
        }
        (0..4).find(|&kind| self.dir_at[kind].is_some_and(|at| at >= q))
    }

    fn stereotype(&mut self, q: usize) -> Option<usize> {
        if self.stereo_from.is_none_or(|from| q < from) || q > self.stereo_to {
            let first = self.stereotypes.find(self.src, q);
            let end = if first.is_none() {
                self.n
            } else {
                self.line_end(q)
            };
            self.stereo_from = Some(q);
            self.stereo_to = end;
            self.stereo_at = [None; 6];
            let mut d = first;
            while let Some(at) = d {
                if at >= end {
                    break;
                }
                let letter = self.c(at + 2) | 32;
                let base = if self.c(at) == 60 { 0 } else { 3 };
                let offset = match letter {
                    102 => 0,
                    106 => 1,
                    _ => 2,
                };
                self.stereo_at[base + offset] = Some(at);
                d = self.stereotypes.find(self.src, at + 1);
            }
        }
        (0..6).find(|&kind| self.stereo_at[kind].is_some_and(|at| at >= q))
    }

    fn emit_id(&mut self, s: usize, e: usize) -> bool {
        match self.percents.find(self.src, s) {
            Some(at) if at + 2 <= e => {
                if at == s {
                    self.p = e;
                    false
                } else {
                    self.emit(T::Id, s, at);
                    true
                }
            }
            _ => {
                self.emit(T::Id, s, e);
                true
            }
        }
    }

    fn finish(mut self, kind: T) -> Tokens {
        self.tokens.types.push(kind);
        self.tokens.texts.push(String::new());
        self.tokens.starts.push(self.p);
        self.tokens
    }

    fn comment(&self, c: u32) -> bool {
        c == 35 || (c == 37 && self.c(self.p + 1) == 37 && self.c(self.p + 2) != 123)
    }

    fn at_end(&mut self) {
        let n = self.n;
        let kind = match self.state {
            S::Initial => T::Nl,
            S::AccTitle => T::AccTitleValue,
            S::AccDescr => T::AccDescrValue,
            S::AccDescrMultiline => T::AccDescrMultilineValue,
            S::ClassDefId => T::ClassDefStyleOpts,
            S::ClassStyle => T::StyleClass,
            S::StyleDefStyles => T::StyleDefStyleOpts,
            S::StateString => T::StateDescr,
            S::FloatingNote => T::NoteText,
            S::StateId | S::FloatingNoteId => T::Id,
            _ => return,
        };
        self.emit(kind, n, n);
    }

    fn run(mut self) -> Tokens {
        loop {
            if self.p >= self.n {
                self.at_end();
                return self.finish(T::End);
            }
            let p = self.p;
            let c = self.c(p);
            let step = match self.state {
                S::Initial | S::Struct => self.document(c),
                S::State => self.state_line(c),
                S::StateString => {
                    if c == 34 {
                        self.p += 1;
                        self.pop();
                    } else {
                        let close = self.index_of(34, p).unwrap_or(self.n);
                        self.emit(T::StateDescr, p, close);
                    }
                    true
                }
                S::StateId => {
                    let mut j = p;
                    while j < self.n && !matches!(self.c(j), 10 | 123) {
                        j += 1;
                    }
                    if self.emit_id(p, j) {
                        self.pop();
                    }
                    true
                }
                S::Scale => {
                    if (48..=57).contains(&c) {
                        let mut j = p + 1;
                        while j < self.n && (48..=57).contains(&self.c(j)) {
                            j += 1;
                        }
                        self.emit(T::Width, p, j);
                        true
                    } else {
                        let q = self.ws_end(p);
                        if q > p && self.word("width", q) && !is_word(self.c(q + 5)) {
                            self.p = q + 5;
                            self.pop();
                            true
                        } else {
                            false
                        }
                    }
                }
                S::AccTitle => {
                    let e = self.eol(p);
                    self.emit(T::AccTitleValue, p, e);
                    self.pop();
                    true
                }
                S::AccDescr => {
                    let e = self.eol(p);
                    self.emit(T::AccDescrValue, p, e);
                    self.pop();
                    true
                }
                S::AccDescrMultiline => {
                    if c == 125 {
                        self.p += 1;
                        self.pop();
                    } else {
                        let close = self.index_of(125, p).unwrap_or(self.n);
                        self.emit(T::AccDescrMultilineValue, p, close);
                    }
                    true
                }
                S::ClassDef => {
                    let is_default = self.word("default", p) && self.ws_end(p + 7) > p + 7;
                    let j = if is_default { p + 7 } else { self.word_end(p) };
                    let e = self.ws_end(j);
                    if j == p || e == j {
                        false
                    } else {
                        self.emit(
                            if is_default {
                                T::DefaultClassDefId
                            } else {
                                T::ClassDefId
                            },
                            p,
                            e,
                        );
                        self.pop();
                        self.push(S::ClassDefId);
                        true
                    }
                }
                S::ClassDefId => {
                    let e = self.eol(p);
                    self.emit(T::ClassDefStyleOpts, p, e);
                    self.pop();
                    true
                }
                S::Class => {
                    let mut j = self.word_end(p);
                    if j == p {
                        false
                    } else {
                        while self.c(j) == 44 {
                            let k = self.ws_end(j + 1);
                            let e = self.word_end(k);
                            if e == k {
                                break;
                            }
                            j = e;
                        }
                        self.emit(T::ClassEntityIds, p, j);
                        self.pop();
                        self.push(S::ClassStyle);
                        true
                    }
                }
                S::ClassStyle => {
                    let e = self.eol(p);
                    self.emit(T::StyleClass, p, e);
                    self.pop();
                    true
                }
                S::Style => {
                    let mut j = p;
                    while j < self.n && (is_word(self.c(j)) || self.c(j) == 44) {
                        j += 1;
                    }
                    let e = self.ws_end(j);
                    if j == p || e == j {
                        false
                    } else {
                        self.emit(T::StyleIds, p, e);
                        self.pop();
                        self.push(S::StyleDefStyles);
                        true
                    }
                }
                S::StyleDefStyles => {
                    let e = self.eol(p);
                    self.emit(T::StyleDefStyleOpts, p, e);
                    self.pop();
                    true
                }
                S::Note => {
                    if self.word("left of", p) && !is_word(self.c(p + 7)) {
                        self.emit(T::LeftOf, p, p + 7);
                        self.pop();
                        self.push(S::NoteId);
                        true
                    } else if self.word("right of", p) && !is_word(self.c(p + 8)) {
                        self.emit(T::RightOf, p, p + 8);
                        self.pop();
                        self.push(S::NoteId);
                        true
                    } else if c == 34 {
                        self.p += 1;
                        self.pop();
                        self.push(S::FloatingNote);
                        true
                    } else {
                        false
                    }
                }
                S::NoteId => {
                    let q = self.ws_end(p);
                    let mut j = q;
                    while j < self.n {
                        let d = self.c(j);
                        if d == 58 || d == 45 || is_space(d) {
                            break;
                        }
                        j += 1;
                    }
                    if j == q {
                        false
                    } else {
                        if self.emit_id(p, j) {
                            self.pop();
                            self.push(S::NoteText);
                        }
                        true
                    }
                }
                S::NoteText => self.note_text(p),
                S::FloatingNote => {
                    let q = self.ws_end(p);
                    if self.word("as", q) {
                        let e = self.ws_end(q + 2);
                        self.emit(T::As, p, e);
                        self.pop();
                        self.push(S::FloatingNoteId);
                    } else if c == 34 {
                        self.p += 1;
                    } else {
                        let close = self.index_of(34, p).unwrap_or(self.n);
                        self.emit(T::NoteText, p, close);
                    }
                    true
                }
                S::FloatingNoteId => {
                    let e = self.eol(p);
                    if self.emit_id(p, e) {
                        self.pop();
                    }
                    true
                }
            };
            if !step {
                return self.finish(T::Error);
            }
        }
    }

    fn note_text(&mut self, p: usize) -> bool {
        let q = self.ws_end(p);
        if self.c(q) == 58 {
            let mut j = q + 1;
            while j < self.n && !matches!(self.c(j), 58 | 10 | 59) {
                j += 1;
            }
            if j > q + 1 {
                let text = js_trim(&self.slice(p + 2, j)).to_owned();
                self.emit_text(T::NoteText, text, p, j);
                self.pop();
                return true;
            }
        }
        let mut k = self.index_of(10, p);
        let mut e = 0;
        while let Some(at) = k {
            e = self.ws_end(at);
            if self.word("end note", e) && !is_word(self.c(e + 8)) {
                break;
            }
            k = self.index_of(10, e);
        }
        if k.is_none() {
            return false;
        }
        let text = js_trim(&self.slice(p, e)).to_owned();
        self.emit_text(T::NoteText, text, p, e + 8);
        self.pop();
        true
    }

    fn state_line(&mut self, c: u32) -> bool {
        let p = self.p;
        if c != 10 && is_space(c) {
            let mut j = p + 1;
            while j < self.n && self.c(j) != 10 && is_space(self.c(j)) {
                j += 1;
            }
            self.p = j;
            return true;
        }
        if self.comment(c) {
            self.p = self.eol(p);
            return true;
        }
        if let Some(kind) = self.stereotype(p) {
            let at = self.stereo_at[kind].unwrap_or(p);
            let choice = kind == 2 || kind == 5;
            let token = if choice {
                T::Choice
            } else if kind % 3 == 0 {
                T::Fork
            } else {
                T::Join
            };
            let text = js_trim(&self.slice(p, at)).to_owned();
            self.emit_text(token, text, p, at + if choice { 10 } else { 8 });
            self.pop();
            return true;
        }
        if c == 34 {
            self.p += 1;
            self.push(S::StateString);
            return true;
        }
        let q = if c == 10 { self.ws_end(p) } else { p };
        if self.word("as", q) {
            let e = self.ws_end(q + 2);
            if e > q + 2 {
                self.emit(T::As, p, e);
                self.push(S::StateId);
                return true;
            }
        }
        if c == 10 {
            self.p += 1;
            self.pop();
            return true;
        }
        if c == 123 {
            self.emit(T::StructStart, p, p + 1);
            self.pop();
            self.push(S::Struct);
            return true;
        }
        if is_word(c) {
            let first = self.word_end(p);
            let second = self.ws_end(first);
            if second > first && is_word(self.c(second)) {
                let brace = self.braces.find(self.src, second + 1);
                if let Some(brace) = brace
                    && brace < self.line_end(second)
                {
                    self.tokens.error = Some(format!(
                        "Error: State name must be a single word. Found: \"{}\"",
                        js_trim(&self.slice(p, brace + 1))
                    ));
                    return false;
                }
            }
        }
        let mut j = p + 1;
        while j < self.n {
            let d = self.c(j);
            if d == 123 || is_space(d) {
                break;
            }
            j += 1;
        }
        self.emit(T::CompositState, p, j);
        true
    }

    fn document(&mut self, c: u32) -> bool {
        let p = self.p;
        let root = self.state == S::Initial;
        let lc = c | 32;
        if root {
            if lc == 99 && self.word("click", p) && !is_word(self.c(p + 5)) {
                self.emit(T::Click, p, p + 5);
                return true;
            }
            if lc == 104 && self.word("href", p) && !is_word(self.c(p + 4)) {
                self.emit(T::Href, p, p + 4);
                return true;
            }
            if c == 34
                && let Some(close) = self.index_of(34, p + 1)
            {
                self.emit(T::Str, p, close + 1);
                return true;
            }
            if lc == 100 && self.word("default", p) && !is_word(self.c(p + 7)) {
                self.emit(T::Default, p, p + 7);
                return true;
            }
            if let Some(kind) = self.direction(p) {
                let e = self.eol(self.dir_end[kind]);
                self.emit(DIRECTION_TOKENS[kind], p, e);
                return true;
            }
            if c == 10 {
                let mut j = p + 1;
                while self.c(j) == 10 {
                    j += 1;
                }
                self.emit(T::Nl, p, j);
                return true;
            }
            if is_space(c) {
                self.p = self.ws_end(p);
                return true;
            }
        } else if c != 10 && is_space(c) {
            let mut j = p + 1;
            while j < self.n && self.c(j) != 10 && is_space(self.c(j)) {
                j += 1;
            }
            self.p = j;
            return true;
        }
        if self.comment(c) {
            self.p = self.eol(p);
            return true;
        }
        if lc == 115 {
            if root && self.word("scale", p) {
                let e = self.ws_end(p + 5);
                if e > p + 5 {
                    self.emit(T::Scale, p, e);
                    self.push(S::Scale);
                    return true;
                }
            }
            if self.word("style", p) {
                let e = self.ws_end(p + 5);
                if e > p + 5 {
                    self.emit(T::Style, p, e);
                    self.push(S::Style);
                    return true;
                }
            }
            if self.word("state", p) {
                let e = self.ws_end(p + 5);
                if e > p + 5 {
                    self.p = e;
                    self.push(S::State);
                    return true;
                }
                if root && self.word("diagram", p + 5) {
                    let from = if self.word("-v2", p + 12) {
                        p + 15
                    } else {
                        p + 12
                    };
                    let e = self.ws_end(from);
                    if e > from {
                        self.emit(T::Sd, p, e);
                        return true;
                    }
                }
            }
        } else if lc == 99 {
            if self.word("class", p) {
                let def = self.word("def", p + 5);
                let from = if def { p + 8 } else { p + 5 };
                let e = self.ws_end(from);
                if e > from {
                    self.emit(if def { T::ClassDef } else { T::Class }, p, e);
                    self.push(if def { S::ClassDef } else { S::Class });
                    return true;
                }
            }
        } else if lc == 97 && root && (self.word("acctitle", p) || self.word("accdescr", p)) {
            let title = (self.c(p + 3) | 32) == 116;
            let q = self.ws_end(p + 8);
            let d = self.c(q);
            if d == 58 {
                let e = self.ws_end(q + 1);
                self.emit(if title { T::AccTitle } else { T::AccDescr }, p, e);
                self.push(if title { S::AccTitle } else { S::AccDescr });
                return true;
            }
            if d == 123 && !title {
                self.p = self.ws_end(q + 1);
                self.push(S::AccDescrMultiline);
                return true;
            }
        }
        if root {
            if c == 123 {
                self.emit(T::StructStart, p, p + 1);
                self.push(S::Struct);
                return true;
            }
        } else {
            if let Some(kind) = self.direction(p) {
                let e = self.eol(self.dir_end[kind]);
                self.emit(DIRECTION_TOKENS[kind], p, e);
                return true;
            }
            if c == 125 {
                self.emit(T::StructStop, p, p + 1);
                self.pop();
                return true;
            }
            if c == 10 {
                self.p += 1;
                return true;
            }
        }
        if lc == 110 && self.word("note", p) {
            let e = self.ws_end(p + 4);
            if e > p + 4 {
                self.emit(T::Note, p, e);
                self.push(S::Note);
                return true;
            }
        }
        if root && lc == 104 && self.word("hide empty description", p) && !is_word(self.c(p + 22)) {
            self.emit(T::HideEmpty, p, p + 22);
            return true;
        }
        if c == 91 && self.c(p + 1) == 42 && self.c(p + 2) == 93 {
            self.emit(T::EdgeState, p, p + 3);
            return true;
        }
        let mut j = p;
        while j < self.n {
            let d = self.c(j);
            if d == 58 || d == 45 || d == 123 || is_space(d) {
                break;
            }
            j += 1;
        }
        if j > p {
            self.emit_id(p, j);
            return true;
        }
        if c == 58 {
            j = p + 1;
            while j < self.n {
                let d = self.c(j);
                if d == 10 || d == 59 {
                    break;
                }
                if d == 58 {
                    let after = self.c(j + 1);
                    if j + 1 >= self.n || after == 58 || after == 10 || after == 59 {
                        break;
                    }
                    j += 1;
                }
                j += 1;
            }
            if j > p + 1 {
                let text = js_trim(&self.slice(p, j)).to_owned();
                self.emit_text(T::Descr, text, p, j);
                return true;
            }
            if self.c(p + 1) == 58 && self.c(p + 2) == 58 {
                self.emit(T::StyleSeparator, p, p + 3);
                return true;
            }
        } else if c == 45 && self.c(p + 1) == 45 {
            if self.c(p + 2) == 62 {
                self.emit(T::Arrow, p, p + 3);
                return true;
            }
            if !root {
                self.emit(T::Concurrent, p, p + 2);
                return true;
            }
        }
        if !root {
            return false;
        }
        self.emit(T::Invalid, p, p + 1);
        true
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Note {
    position: String,
    text: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
enum Desc {
    #[default]
    Absent,
    Text(String),
    List(Vec<String>),
}

impl Desc {
    fn truthy(&self) -> bool {
        match self {
            Self::Absent => false,
            Self::Text(text) => !text.is_empty(),
            Self::List(_) => true,
        }
    }

    fn flat(&self) -> Vec<String> {
        match self {
            Self::Absent => Vec::new(),
            Self::Text(text) => vec![text.clone()],
            Self::List(list) => list.clone(),
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::Absent => 0,
            Self::Text(text) => text.encode_utf16().count(),
            Self::List(list) => list.len(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct StateStmt {
    id: String,
    kind: Option<&'static str>,
    description: Desc,
    doc: Option<usize>,
    note: Option<Note>,
    start: Option<bool>,
    classes: Vec<String>,
}

impl StateStmt {
    fn new(id: String, kind: Option<&'static str>) -> Self {
        Self {
            id,
            kind,
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Stmt {
    Text,
    State(StateStmt),
    Relation(StateStmt, StateStmt, Option<String>),
    ClassDef(String, String),
    Style(String, String),
    ApplyClass(String, String),
    Dir(&'static str),
    Click(StateStmt, String, String),
}

#[derive(Clone, Debug, Default)]
struct Known {
    classes: Vec<String>,
    styles: Vec<String>,
}

#[derive(Debug, Default)]
struct Model {
    docs: Vec<Vec<Stmt>>,
    dividers: usize,
    acc_title: Option<String>,
    acc_descr: Option<String>,
    states: HashMap<String, Known>,
    classes: HashMap<String, Vec<String>>,
    links: HashMap<String, (String, String)>,
}

type Edit = (usize, usize, usize);

fn step_input(edits: &[Edit], at: usize) -> usize {
    let mut shift: isize = 0;
    let at = at as isize;
    for &(from, to, length) in edits {
        let (from, to, length) = (from as isize, to as isize, length as isize);
        let start = from - shift;
        if at < start {
            break;
        }
        if at < start + length {
            return (from + (at - start).min(to - from - 1)) as usize;
        }
        shift += to - from - length;
    }
    (at + shift).max(0) as usize
}

fn drop_color_semicolons(text: Vec<u16>, keyword: &str, steps: &mut Vec<Vec<Edit>>) -> Vec<u16> {
    let key: Vec<u16> = keyword.encode_utf16().collect();
    let find = |from: usize| {
        text.get(from..)
            .and_then(|rest| rest.windows(key.len()).position(|w| w == key.as_slice()))
            .map(|at| from + at)
    };
    let mut edits = Vec::new();
    let mut out = Vec::new();
    let mut last = 0;
    let mut from = find(0);
    while let Some(start) = from {
        let line_end = text[start..]
            .iter()
            .position(|&c| matches!(c, 10 | 13 | 0x2028 | 0x2029))
            .map_or(text.len(), |at| start + at);
        let mut after_colon = false;
        let mut hash = None;
        for (j, &c) in text
            .iter()
            .enumerate()
            .take(line_end)
            .skip(start + key.len())
        {
            if c == 58 {
                after_colon = true;
            } else if c == 35 {
                if after_colon {
                    hash = Some(j);
                    break;
                }
            } else if is_space(u32::from(c)) {
                after_colon = false;
            }
        }
        if let Some(hash) = hash
            && line_end > 0
            && let Some(semi) = text[..line_end].iter().rposition(|&c| c == 59)
            && semi > hash
        {
            edits.push((semi, semi + 1, 0));
            out.extend_from_slice(&text[last..semi]);
            last = semi + 1;
        }
        from = if line_end >= text.len() {
            None
        } else {
            find(line_end + 1)
        };
    }
    steps.push(edits);
    if last == 0 {
        text
    } else {
        out.extend_from_slice(&text[last..]);
        out
    }
}

fn encode_entities(text: Vec<u16>, steps: &mut Vec<Vec<Edit>>) -> Vec<u16> {
    if !text.contains(&35) {
        return text;
    }
    let text = drop_color_semicolons(text, "style", steps);
    let text = drop_color_semicolons(text, "classDef", steps);
    let mut edits = Vec::new();
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        if text[i] == 35 {
            let mut j = i + 1;
            while j < text.len() && is_word(u32::from(text[j])) {
                j += 1;
            }
            if j > i + 1 && text.get(j) == Some(&59) {
                let inner = &text[i + 1..j];
                let digits = inner.iter().all(|&c| (48..=57).contains(&c));
                let mut code: Vec<u16> = if digits { "ﬂ°°" } else { "ﬂ°" }.encode_utf16().collect();
                code.extend_from_slice(inner);
                code.extend("¶ß".encode_utf16());
                edits.push((i, j + 1, code.len()));
                out.extend(code);
                i = j + 1;
                continue;
            }
        }
        out.push(text[i]);
        i += 1;
    }
    steps.push(edits);
    out
}

struct Source {
    text: Vec<u16>,
    parsed: Vec<u16>,
    steps: Vec<Vec<Edit>>,
}

impl Source {
    fn new(source: &str) -> Self {
        let text: Vec<u16> = source.encode_utf16().collect();
        let mut steps = Vec::new();
        let mut parsed = encode_entities(text.clone(), &mut steps);
        parsed.push(10);
        Self {
            text,
            parsed,
            steps,
        }
    }

    fn written(&self, at: usize) -> usize {
        let mut at = at;
        for edits in self.steps.iter().rev() {
            at = step_input(edits, at);
        }
        at.min(self.text.len())
    }

    fn locate(&self, offset: usize) -> (usize, usize) {
        let end = self
            .parsed
            .iter()
            .rposition(|&c| !is_space(u32::from(c)))
            .map_or(0, |at| at + 1);
        let offset = if end > 0 && offset >= end {
            self.written(end - 1) + 1
        } else {
            self.written(offset)
        };
        let offset = offset.min(self.text.len());
        let before = &self.text[..offset];
        let line = before.iter().filter(|&&c| c == 10).count() + 1;
        let line_start = before.iter().rposition(|&c| c == 10).map_or(0, |at| at + 1);
        (line, offset - line_start + 1)
    }

    fn error(&self, offset: usize, message: String) -> PeleError {
        let (line, column) = self.locate(offset);
        PeleError::syntax(message, line, column)
    }
}

fn trim_colon(text: &str) -> String {
    match text.strip_prefix(':') {
        Some(rest) => js_trim(rest).to_owned(),
        None => js_trim(text).to_owned(),
    }
}

const STATEMENT: &str = "'NL', 'ID', 'HIDE_EMPTY', 'scale', 'COMPOSIT_STATE', 'STATE_DESCR', 'FORK', 'JOIN', 'CHOICE', 'CONCURRENT', 'note', 'acc_title', 'acc_descr', 'acc_descr_multiline_value', 'CLICK', 'classDef', 'style', 'class', 'direction_tb', 'direction_bt', 'direction_rl', 'direction_lr', 'EDGE_STATE'";

struct Parser<'a> {
    source: &'a Source,
    tokens: Tokens,
    i: usize,
}

impl Parser<'_> {
    fn kind(&self, at: usize) -> T {
        self.tokens.types.get(at).copied().unwrap_or(T::End)
    }

    fn fail(&self, expected: &str) -> PeleError {
        let t = self.kind(self.i);
        let start = self.tokens.starts.get(self.i).copied().unwrap_or(0);
        if t == T::Error {
            if let Some(error) = &self.tokens.error {
                return self.source.error(start, error.clone());
            }
            return self
                .source
                .error(start, "Lexical error: Unrecognized text.".to_owned());
        }
        self.source.error(
            start,
            format!(
                "Parse error: Expecting {expected}, got '{}'",
                TOKEN_NAMES[t as usize]
            ),
        )
    }

    fn expect(&mut self, kind: T) -> Result<String, PeleError> {
        if self.kind(self.i) != kind {
            return Err(self.fail(&format!("'{}'", TOKEN_NAMES[kind as usize])));
        }
        self.i += 1;
        Ok(std::mem::take(&mut self.tokens.texts[self.i - 1]))
    }

    fn take(&mut self) -> String {
        self.i += 1;
        std::mem::take(&mut self.tokens.texts[self.i - 1])
    }

    fn id_statement(&mut self) -> Result<StateStmt, PeleError> {
        let t = self.kind(self.i);
        if t != T::Id && t != T::EdgeState {
            return Err(self.fail("'ID', 'EDGE_STATE'"));
        }
        let id = js_trim(&self.take()).to_owned();
        let mut state = StateStmt::new(id, Some("default"));
        state.description = Desc::Text(String::new());
        if self.kind(self.i) != T::StyleSeparator {
            return Ok(state);
        }
        self.i += 1;
        state.classes = vec![js_trim(&self.expect(T::Id)?).to_owned()];
        Ok(state)
    }
}

fn parse(source: &Source) -> Result<Model, PeleError> {
    let tokens = Lexer::new(&source.parsed).run();
    let mut parser = Parser {
        source,
        tokens,
        i: 0,
    };
    let mut model = Model {
        docs: vec![Vec::new()],
        ..Model::default()
    };
    while parser.kind(parser.i) == T::Nl {
        parser.i += 1;
    }
    if parser.kind(parser.i) != T::Sd {
        return Err(parser.fail("'NL', 'SD'"));
    }
    parser.i += 1;
    let mut doc = 0;
    let mut outer: Vec<usize> = Vec::new();
    let add = |model: &mut Model, doc: usize, value: Stmt| model.docs[doc].push(value);
    let add_text = |model: &mut Model, doc: usize, text: &str| {
        if text != "nl" {
            model.docs[doc].push(Stmt::Text);
        }
    };
    loop {
        let t = parser.kind(parser.i);
        match t {
            T::Nl => parser.i += 1,
            T::Id | T::EdgeState => {
                let mut first = parser.id_statement()?;
                match parser.kind(parser.i) {
                    T::Descr => {
                        first.description = Desc::Text(trim_colon(&parser.take()));
                        add(&mut model, doc, Stmt::State(first));
                    }
                    T::Arrow => {
                        parser.i += 1;
                        let second = parser.id_statement()?;
                        let description = if parser.kind(parser.i) == T::Descr {
                            Some(trim_colon(&parser.take()))
                        } else {
                            None
                        };
                        add(&mut model, doc, Stmt::Relation(first, second, description));
                    }
                    _ => add(&mut model, doc, Stmt::State(first)),
                }
            }
            T::HideEmpty => {
                let text = parser.take();
                add_text(&mut model, doc, &text);
            }
            T::Scale => {
                let text = parser.take();
                parser.expect(T::Width)?;
                add_text(&mut model, doc, &text);
            }
            T::CompositState => {
                if parser.kind(parser.i + 1) == T::StructStart {
                    let id = parser.take();
                    open(
                        &mut parser,
                        &mut model,
                        &mut doc,
                        &mut outer,
                        id,
                        String::new(),
                    )?;
                } else {
                    let text = parser.take();
                    add_text(&mut model, doc, &text);
                }
            }
            T::StateDescr => {
                let description = parser.take();
                parser.expect(T::As)?;
                let id = parser.expect(T::Id)?;
                if parser.kind(parser.i) == T::StructStart {
                    open(
                        &mut parser,
                        &mut model,
                        &mut doc,
                        &mut outer,
                        id,
                        description,
                    )?;
                } else if id.contains(':') {
                    let mut parts = id.split(':');
                    let head = parts.next().unwrap_or("").to_owned();
                    let second = parts.next().unwrap_or("").to_owned();
                    let mut state = StateStmt::new(head, Some("default"));
                    state.description = Desc::List(vec![js_trim(&description).to_owned(), second]);
                    add(&mut model, doc, Stmt::State(state));
                } else {
                    let mut state = StateStmt::new(id, Some("default"));
                    state.description = Desc::Text(js_trim(&description).to_owned());
                    add(&mut model, doc, Stmt::State(state));
                }
            }
            T::Fork | T::Join | T::Choice => {
                let kind = match t {
                    T::Fork => "fork",
                    T::Join => "join",
                    _ => "choice",
                };
                let id = parser.take();
                add(&mut model, doc, Stmt::State(StateStmt::new(id, Some(kind))));
            }
            T::Concurrent => {
                parser.i += 1;
                model.dividers += 1;
                let id = format!("divider-id-{}", model.dividers);
                add(
                    &mut model,
                    doc,
                    Stmt::State(StateStmt::new(id, Some("divider"))),
                );
            }
            T::Note => {
                let keyword = parser.take();
                if matches!(parser.kind(parser.i), T::LeftOf | T::RightOf) {
                    let position = js_trim(&parser.take()).to_owned();
                    let id = js_trim(&parser.expect(T::Id)?).to_owned();
                    let text = js_trim(&parser.expect(T::NoteText)?).to_owned();
                    let mut state = StateStmt::new(id, None);
                    state.note = Some(Note { position, text });
                    add(&mut model, doc, Stmt::State(state));
                } else {
                    if parser.kind(parser.i) != T::NoteText {
                        return Err(parser.fail("'NOTE_TEXT', 'left_of', 'right_of'"));
                    }
                    parser.i += 1;
                    parser.expect(T::As)?;
                    parser.expect(T::Id)?;
                    add_text(&mut model, doc, &keyword);
                }
            }
            T::DirectionTb | T::DirectionBt | T::DirectionRl | T::DirectionLr => {
                let at = DIRECTION_TOKENS.iter().position(|d| *d == t).unwrap_or(0);
                parser.i += 1;
                add(&mut model, doc, Stmt::Dir(DIRECTIONS[at]));
            }
            T::AccTitle => {
                parser.i += 1;
                let value = js_trim(&parser.expect(T::AccTitleValue)?).to_owned();
                model.acc_title = Some(value.trim_start_matches(is_js_space).to_owned());
                add_text(&mut model, doc, &value);
            }
            T::AccDescr => {
                parser.i += 1;
                let value = js_trim(&parser.expect(T::AccDescrValue)?).to_owned();
                model.acc_descr = Some(collapse_indent(&value));
                add_text(&mut model, doc, &value);
            }
            T::AccDescrMultilineValue => {
                let value = js_trim(&parser.take()).to_owned();
                model.acc_descr = Some(collapse_indent(&value));
                add_text(&mut model, doc, &value);
            }
            T::Click => {
                parser.i += 1;
                let id = parser.id_statement()?;
                let mut tooltip = String::new();
                let url = if parser.kind(parser.i) == T::Href {
                    parser.i += 1;
                    parser.expect(T::Str)?
                } else {
                    if parser.kind(parser.i) != T::Str {
                        return Err(parser.fail("'STRING', 'HREF'"));
                    }
                    let url = parser.take();
                    tooltip = parser.expect(T::Str)?;
                    url
                };
                parser.expect(T::Nl)?;
                add(&mut model, doc, Stmt::Click(id, url, tooltip));
            }
            T::ClassDef => {
                parser.i += 1;
                let id = js_trim(&parser.expect(T::ClassDefId)?).to_owned();
                let classes = js_trim(&parser.expect(T::ClassDefStyleOpts)?).to_owned();
                add(&mut model, doc, Stmt::ClassDef(id, classes));
            }
            T::Style => {
                parser.i += 1;
                let id = js_trim(&parser.expect(T::StyleIds)?).to_owned();
                let style = js_trim(&parser.expect(T::StyleDefStyleOpts)?).to_owned();
                add(&mut model, doc, Stmt::Style(id, style));
            }
            T::Class => {
                parser.i += 1;
                let id = js_trim(&parser.expect(T::ClassEntityIds)?).to_owned();
                let style = js_trim(&parser.expect(T::StyleClass)?).to_owned();
                add(&mut model, doc, Stmt::ApplyClass(id, style));
            }
            T::StructStop => {
                let Some(parent) = outer.pop() else {
                    return Err(parser.fail(&format!("'$end', {STATEMENT}")));
                };
                parser.i += 1;
                doc = parent;
            }
            T::End => {
                if !outer.is_empty() {
                    return Err(parser.fail(&format!("'STRUCT_STOP', {STATEMENT}")));
                }
                return Ok(model);
            }
            _ => {
                return Err(parser.fail(&if outer.is_empty() {
                    format!("'$end', {STATEMENT}")
                } else {
                    format!("'STRUCT_STOP', {STATEMENT}")
                }));
            }
        }
    }
}

fn open(
    parser: &mut Parser<'_>,
    model: &mut Model,
    doc: &mut usize,
    outer: &mut Vec<usize>,
    id: String,
    description: String,
) -> Result<(), PeleError> {
    if outer.len() >= MAX_DEPTH {
        return Err(PeleError::limit(
            "State diagram composites are nested too deeply",
        ));
    }
    parser.i += 1;
    let inner = model.docs.len();
    model.docs.push(Vec::new());
    let mut state = StateStmt::new(id, Some("default"));
    state.description = Desc::Text(description);
    state.doc = Some(inner);
    model.docs[*doc].push(Stmt::State(state));
    outer.push(*doc);
    *doc = inner;
    Ok(())
}

fn collapse_indent(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        out.push(ch);
        while ch == '\n' && chars.peek().is_some_and(|c| is_js_space(*c)) {
            chars.next();
        }
    }
    out
}

#[derive(Clone, Copy)]
enum Slot {
    Root,
    Item(usize, usize),
    Side(usize, usize, bool),
}

fn rename(state: &mut StateStmt, parent: &str, first: bool) {
    if state.id == "[*]" {
        state.id = format!("{parent}{}", if first { "_start" } else { "_end" });
        state.start = Some(first);
    } else {
        state.id = js_trim(&state.id).to_owned();
    }
}

impl Model {
    fn direction(&self) -> &'static str {
        doc_direction(&self.docs[0])
    }

    fn translate(&mut self) {
        let mut pending = vec![("root".to_owned(), Slot::Root, true)];
        while let Some((parent, slot, first)) = pending.pop() {
            let (id, doc) = match slot {
                Slot::Root => ("root".to_owned(), Some(0)),
                Slot::Item(d, i) => match &mut self.docs[d][i] {
                    Stmt::Relation(..) => {
                        pending.push((parent.clone(), Slot::Side(d, i, true), false));
                        pending.push((parent, Slot::Side(d, i, false), true));
                        continue;
                    }
                    Stmt::State(state) => {
                        rename(state, &parent, first);
                        (state.id.clone(), state.doc)
                    }
                    _ => continue,
                },
                Slot::Side(d, i, second) => {
                    let Stmt::Relation(one, two, _) = &mut self.docs[d][i] else {
                        continue;
                    };
                    let state = if second { two } else { one };
                    rename(state, &parent, first);
                    (state.id.clone(), state.doc)
                }
            };
            let Some(doc) = doc else {
                continue;
            };
            let items = std::mem::take(&mut self.docs[doc]);
            let mut regions = Vec::new();
            let mut current = Vec::new();
            for item in items {
                match item {
                    Stmt::State(state) if state.kind == Some("divider") && state.doc.is_none() => {
                        let region = self.docs.len();
                        self.docs.push(std::mem::take(&mut current));
                        regions.push(Stmt::State(StateStmt {
                            doc: Some(region),
                            ..state
                        }));
                    }
                    other => current.push(other),
                }
            }
            if regions.is_empty() {
                self.docs[doc] = current;
            } else {
                if !current.is_empty() {
                    self.dividers += 1;
                    let region = self.docs.len();
                    self.docs.push(current);
                    let mut state =
                        StateStmt::new(format!("divider-id-{}", self.dividers), Some("divider"));
                    state.doc = Some(region);
                    regions.push(Stmt::State(state));
                }
                self.docs[doc] = regions;
            }
            for k in (0..self.docs[doc].len()).rev() {
                pending.push((id.clone(), Slot::Item(doc, k), true));
            }
        }
    }

    fn add_state(&mut self, id: &str, classes: &[String]) {
        let trimmed = js_trim(id);
        if !self.states.contains_key(trimmed) {
            self.states.insert(trimmed.to_owned(), Known::default());
        }
        let trimmed = trimmed.to_owned();
        for name in classes {
            self.set_css_class(&trimmed, js_trim(name));
        }
    }

    fn named(&mut self, ids: &str) -> Vec<String> {
        let mut out = Vec::new();
        for id in ids.split(',') {
            if self.states.contains_key(id) {
                out.push(id.to_owned());
                continue;
            }
            self.add_state(id, &[]);
            let trimmed = js_trim(id);
            if self.states.contains_key(trimmed) {
                out.push(trimmed.to_owned());
            }
        }
        out
    }

    fn set_css_class(&mut self, ids: &str, name: &str) {
        for id in self.named(ids) {
            if let Some(state) = self.states.get_mut(&id) {
                state.classes.push(name.to_owned());
            }
        }
    }

    fn add_style_class(&mut self, id: &str, attributes: &str) {
        let found = self.classes.entry(id.to_owned()).or_default();
        if attributes.is_empty() {
            return;
        }
        for attribute in attributes.split(',') {
            found.push(js_trim(&attribute.replacen(';', "", 1)).to_owned());
        }
    }

    fn extract(&mut self) {
        let root = std::mem::take(&mut self.docs[0]);
        for item in &root {
            match item {
                Stmt::State(state) => self.add_state(&state.id, &[]),
                Stmt::Relation(one, two, _) => {
                    self.add_state(&one.id, &one.classes);
                    self.add_state(&two.id, &two.classes);
                }
                Stmt::ClassDef(id, classes) => self.add_style_class(js_trim(id), classes),
                Stmt::Style(id, style) => {
                    let styles: Vec<String> = style
                        .split(',')
                        .map(|s| js_trim(&s.replace(';', "")).to_owned())
                        .collect();
                    for named in self.named(js_trim(id)) {
                        if let Some(state) = self.states.get_mut(&named) {
                            state.styles.clone_from(&styles);
                        }
                    }
                }
                Stmt::ApplyClass(id, style) => self.set_css_class(js_trim(id), style),
                Stmt::Click(id, url, tooltip) => {
                    self.links
                        .insert(id.id.clone(), (url.clone(), tooltip.clone()));
                }
                Stmt::Text | Stmt::Dir(_) => {}
            }
        }
        self.docs[0] = root;
    }
}

fn doc_direction(doc: &[Stmt]) -> &'static str {
    doc.iter()
        .rev()
        .find_map(|item| match item {
            Stmt::Dir(value) => Some(*value),
            _ => None,
        })
        .unwrap_or("TB")
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Shape {
    #[default]
    Rect,
    RectWithTitle,
    StateStart,
    StateEnd,
    Choice,
    Fork,
    Join,
    Divider,
    RoundedWithTitle,
    Note,
    NoteGroup,
    Unknown,
}

#[derive(Clone, Debug, Default)]
struct GNode {
    id: String,
    shape: Shape,
    label: String,
    description: Vec<String>,
    css_classes: String,
    css_compiled: Vec<String>,
    css_styles: Vec<String>,
    is_group: bool,
    parent: Option<String>,
    dir: Option<&'static str>,
    note_for: Option<String>,
    drafted: bool,
}

#[derive(Clone, Debug)]
struct GEdge {
    id: String,
    start: String,
    end: String,
    label: String,
    dashed: bool,
}

#[derive(Clone, Debug)]
struct Draft {
    shape: Shape,
    description: Desc,
    css_classes: String,
    css_styles: Vec<String>,
    group: bool,
    dir: Option<&'static str>,
}

struct GraphBuilder<'a> {
    model: &'a Model,
    nodes: Vec<GNode>,
    edges: Vec<GEdge>,
    by_id: HashMap<String, usize>,
    drafts: HashMap<String, Draft>,
    counter: usize,
}

impl GraphBuilder<'_> {
    fn insert(&mut self, mut node: GNode) {
        if node.id.is_empty() || node.id == "</join></fork>" || node.id == "</choice>" {
            return;
        }
        if let Some(&at) = self.by_id.get(&node.id) {
            let existing = &mut self.nodes[at];
            let parent = node.parent.take().or_else(|| existing.parent.take());
            let note_for = node.note_for.take().or_else(|| existing.note_for.take());
            *existing = node;
            existing.parent = parent;
            existing.note_for = note_for;
        } else {
            self.by_id.insert(node.id.clone(), self.nodes.len());
            self.nodes.push(node);
        }
    }

    fn draft(&self, item: &StateStmt) -> Draft {
        let known = self.model.states.get(&item.id);
        let class_str = known.map_or(String::new(), |known| known.classes.join(" "));
        let mut shape = match item.start {
            Some(true) => Shape::StateStart,
            Some(false) => Shape::StateEnd,
            None => Shape::Rect,
        };
        match item.kind {
            Some("default") => {}
            Some("fork") => shape = Shape::Fork,
            Some("join") => shape = Shape::Join,
            Some("choice") => shape = Shape::Choice,
            Some("divider") => shape = Shape::Divider,
            _ => shape = Shape::Unknown,
        }
        Draft {
            shape,
            description: Desc::Text(item.id.clone()),
            css_classes: format!("{class_str} statediagram-state"),
            css_styles: known.map_or(Vec::new(), |known| known.styles.clone()),
            group: false,
            dir: None,
        }
    }

    fn visit(&mut self, parent: Option<&str>, item: &StateStmt, alt: bool) {
        let id = item.id.as_str();
        if id == "root" {
            return;
        }
        if !self.drafts.contains_key(id) {
            let draft = self.draft(item);
            self.drafts.insert(id.to_owned(), draft);
        }
        let Some(draft) = self.drafts.get_mut(id) else {
            return;
        };
        if item.description.truthy() {
            match &mut draft.description {
                Desc::List(list) => {
                    draft.shape = Shape::RectWithTitle;
                    list.extend(item.description.flat());
                }
                Desc::Text(text) if !text.is_empty() => {
                    draft.shape = Shape::RectWithTitle;
                    draft.description = if text == id {
                        Desc::List(item.description.flat())
                    } else {
                        let mut joined = vec![std::mem::take(text)];
                        joined.extend(item.description.flat());
                        Desc::List(joined)
                    };
                }
                _ => {
                    draft.shape = Shape::Rect;
                    draft.description = item.description.clone();
                }
            }
        }
        if draft.description.len() == 1 && draft.shape == Shape::RectWithTitle {
            draft.shape = if draft.group {
                Shape::RoundedWithTitle
            } else {
                Shape::Rect
            };
        }
        if !draft.group
            && let Some(doc) = item.doc
        {
            draft.group = true;
            draft.dir = Some(doc_direction(&self.model.docs[doc]));
            draft.shape = if item.kind == Some("divider") {
                Shape::Divider
            } else {
                Shape::RoundedWithTitle
            };
            draft.css_classes = format!(
                "{} statediagram-cluster {}",
                draft.css_classes,
                if alt { "statediagram-cluster-alt" } else { "" }
            );
        }
        let node = GNode {
            id: id.to_owned(),
            parent: parent.filter(|parent| *parent != "root").map(str::to_owned),
            drafted: true,
            ..GNode::default()
        };
        let Some(note) = &item.note else {
            self.insert(node);
            return;
        };
        let aside = |aside_id: String, shape: Shape, css_classes: String, is_group: bool| GNode {
            id: aside_id,
            shape,
            label: note.text.clone(),
            css_classes,
            is_group,
            ..GNode::default()
        };
        let note_id = format!("{id}----note-{}", self.counter);
        self.counter += 1;
        let group_id = format!("{id}----parent");
        let mut note_node = aside(
            note_id.clone(),
            Shape::Note,
            "statediagram-note".to_owned(),
            false,
        );
        note_node.parent = Some(group_id.clone());
        note_node.note_for = Some(id.to_owned());
        let group = aside(group_id, Shape::NoteGroup, draft.css_classes.clone(), true);
        self.insert(group);
        self.insert(note_node);
        self.insert(node);
        let left = note.position == "left of";
        let (from, to) = if left {
            (note_id, id.to_owned())
        } else {
            (id.to_owned(), note_id)
        };
        self.edges.push(GEdge {
            id: format!("{from}-{to}"),
            start: from,
            end: to,
            label: String::new(),
            dashed: true,
        });
    }

    fn finish(mut self) -> Result<Graph, PeleError> {
        for node in &mut self.nodes {
            if node.drafted
                && let Some(draft) = self.drafts.get(&node.id)
            {
                node.shape = draft.shape;
                (node.label, node.description) = match &draft.description {
                    Desc::List(list) => (
                        list.first().cloned().unwrap_or_default(),
                        list.iter().skip(1).cloned().collect(),
                    ),
                    Desc::Text(text) => (text.clone(), Vec::new()),
                    Desc::Absent => (String::new(), Vec::new()),
                };
                node.css_classes.clone_from(&draft.css_classes);
                node.css_styles.clone_from(&draft.css_styles);
                node.is_group = draft.group;
                node.dir = draft.dir;
                if node.shape == Shape::Divider {
                    node.label = String::new();
                }
            }
            if node.is_group && !node.description.is_empty() {
                return Err(PeleError::semantic(format!(
                    "Group nodes can only have label. Remove the additional description for node [{}]",
                    node.id
                )));
            }
            if !node.css_classes.is_empty() {
                for name in node.css_classes.split(' ') {
                    if let Some(styles) = self.model.classes.get(name) {
                        node.css_compiled.extend(styles.iter().cloned());
                    }
                }
            }
        }
        Ok(Graph {
            nodes: self.nodes,
            edges: self.edges,
        })
    }
}

struct Graph {
    nodes: Vec<GNode>,
    edges: Vec<GEdge>,
}

fn build_graph(model: &Model) -> Result<Graph, PeleError> {
    let mut builder = GraphBuilder {
        model,
        nodes: Vec::new(),
        edges: Vec::new(),
        by_id: HashMap::new(),
        drafts: HashMap::new(),
        counter: 0,
    };
    let mut stack: Vec<(Option<&StateStmt>, usize, usize, bool)> = vec![(None, 0, 0, true)];
    while let Some(frame) = stack.last_mut() {
        let (item, doc, at, alt) = *frame;
        if at >= model.docs[doc].len() {
            stack.pop();
            continue;
        }
        frame.2 += 1;
        let parent = item.map(|item| item.id.as_str());
        match &model.docs[doc][at] {
            Stmt::State(state) => {
                builder.visit(parent, state, alt);
                if let Some(inner) = state.doc {
                    stack.push((Some(state), inner, 0, !alt));
                }
            }
            Stmt::Relation(one, two, description) => {
                builder.visit(parent, one, alt);
                builder.visit(parent, two, alt);
                let id = format!("edge{}", builder.counter);
                builder.counter += 1;
                builder.edges.push(GEdge {
                    id,
                    start: one.id.clone(),
                    end: two.id.clone(),
                    label: description.clone().unwrap_or_default(),
                    dashed: false,
                });
            }
            _ => {}
        }
        if builder.nodes.len() > MAX_STATES {
            return Err(PeleError::limit(format!(
                "State diagram has more than {MAX_STATES} states"
            )));
        }
    }
    builder.finish()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum K {
    State,
    Described,
    Start,
    End,
    Choice,
    Fork,
    Join,
    Note,
    Composite,
    Region,
    Hidden,
}

const CLASSES: [&str; 9] = [
    "pele-node pele-state",
    "pele-node pele-state",
    "pele-node pele-state-start",
    "pele-node pele-state-end",
    "pele-node pele-state-choice",
    "pele-node pele-state-fork",
    "pele-node pele-state-join",
    "pele-note",
    "pele-cluster pele-state-composite",
];

fn kind_of(node: &GNode) -> K {
    if node.is_group {
        return if node.shape == Shape::Divider {
            K::Region
        } else {
            K::Composite
        };
    }
    match node.shape {
        Shape::RectWithTitle => K::Described,
        Shape::StateStart => K::Start,
        Shape::StateEnd => K::End,
        Shape::Choice => K::Choice,
        Shape::Fork => K::Fork,
        Shape::Join => K::Join,
        Shape::Note => K::Note,
        Shape::Divider => K::Hidden,
        _ => K::State,
    }
}

struct View {
    node: Option<usize>,
    kind: K,
    style: Option<ResolvedStyle>,
    label: Label,
    lines: Vec<Label>,
    w: f64,
    h: f64,
    title_h: f64,
}

impl View {
    fn new(node: Option<usize>, kind: K) -> Self {
        Self {
            node,
            kind,
            style: None,
            label: Label::default(),
            lines: Vec::new(),
            w: 0.0,
            h: 0.0,
            title_h: 0.0,
        }
    }
}

fn group(dir: Dir, pad_x: f64, pad_top: f64, pad_bottom: f64) -> CNode {
    let mut c = cnode(0.0, 0.0, None);
    c.is_group = true;
    c.dir = Some(dir);
    c.pad_x = pad_x;
    c.pad_top = pad_top;
    c.pad_bottom = pad_bottom;
    c
}

fn rect(x: f64, y: f64, w: f64, h: f64, rest: &str) -> String {
    format!(
        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{rest}/>",
        svg::num(x),
        svg::num(y),
        svg::num(w),
        svg::num(h)
    )
}

fn unquote(text: &str) -> &str {
    text.trim_matches('"')
}

fn without_mermaid_classes(classes: &str) -> String {
    classes
        .split(' ')
        .filter(|name| {
            !matches!(
                *name,
                "statediagram-state"
                    | "statediagram-cluster-alt"
                    | "statediagram-cluster"
                    | "statediagram-note"
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn row(y: f64) -> i64 {
    js_round(y) as i64
}

fn title_x(c: &CNode, width: f64, crossings: &HashMap<i64, Vec<f64>>) -> f64 {
    let Some(xs) = crossings.get(&row(c.y - c.h / 2.0)) else {
        return c.x;
    };
    if width == 0.0 {
        return c.x;
    }
    let left = c.x - c.w / 2.0;
    let half = width / 2.0;
    let mut best = c.x;
    let mut best_gap = -1.0;
    for x in [c.x, left + GROUP_PAD + half, left + c.w - GROUP_PAD - half] {
        let mut gap = f64::INFINITY;
        for &cx in xs {
            if cx > left && cx < left + c.w {
                gap = gap.min((cx - x).abs() - half);
            }
        }
        if gap >= 6.0 {
            return x;
        }
        if gap > best_gap + 0.5 {
            best_gap = gap;
            best = x;
        }
    }
    best
}

pub fn render(source: &str, options: &RenderOptions) -> Result<RenderResult, PeleError> {
    let source = Source::new(source);
    let mut model = parse(&source)?;
    model.translate();
    model.extract();
    let graph = build_graph(&model)?;
    if graph.edges.len() > options.max_edges {
        return Err(PeleError::limit(format!(
            "State diagram has more than {} transitions",
            options.max_edges
        )));
    }
    let direction = model.direction();
    if runs_across(direction) {
        turn_to_fit(options, |down| draw(&model, &graph, options, down))
    } else {
        draw(&model, &graph, options, false)
    }
}

fn number_option(options: &RenderOptions, key: &str, fallback: f64) -> f64 {
    options.config["state"][key]
        .as_f64()
        .filter(|value| *value > 0.0)
        .map_or(fallback, |value| value.min(2000.0))
}

struct Layout<'a> {
    views: Vec<View>,
    cnodes: Vec<CNode>,
    logical: Vec<Option<usize>>,
    graph: &'a Graph,
    root_dir: Dir,
}

impl Layout<'_> {
    fn inner_dir(&self, composite: usize) -> Dir {
        let node = self.views[composite].node.map(|n| &self.graph.nodes[n]);
        direction(node.and_then(|node| node.dir).unwrap_or("TB"))
    }

    fn across(&self, i: usize) -> bool {
        match self.logical[i] {
            None => self.root_dir.vertical(),
            Some(p) => self.inner_dir(p).vertical(),
        }
    }

    fn encloses(&self, composite: usize, item: usize) -> bool {
        if !self.cnodes[composite].is_group {
            return false;
        }
        let mut p = self.logical[item];
        while let Some(at) = p {
            if at == composite {
                return true;
            }
            p = self.logical[at];
        }
        false
    }

    fn add(&mut self, node: Option<usize>, kind: K, c: Option<CNode>) -> usize {
        self.views.push(View::new(node, kind));
        if let Some(c) = c {
            self.cnodes.push(c);
        }
        self.views.len() - 1
    }
}

fn break_rings(logical: &mut [Option<usize>]) {
    let states = logical.len();
    let mut kids = vec![Vec::new(); states];
    let mut queue = Vec::new();
    let mut placed = vec![false; states];
    for i in 0..states {
        match logical[i] {
            None => {
                placed[i] = true;
                queue.push(i);
            }
            Some(p) => kids[p].push(i),
        }
    }
    let mut done = 0;
    let settle = |queue: &mut Vec<usize>, placed: &mut [bool], done: &mut usize| {
        while *done < queue.len() {
            for &kid in &kids[queue[*done]] {
                if !placed[kid] {
                    placed[kid] = true;
                    queue.push(kid);
                }
            }
            *done += 1;
        }
    };
    settle(&mut queue, &mut placed, &mut done);
    let mut seen = vec![usize::MAX; states];
    for i in 0..states {
        if placed[i] {
            continue;
        }
        let mut ring = i;
        while seen[ring] != i {
            seen[ring] = i;
            match logical[ring] {
                Some(p) => ring = p,
                None => break,
            }
        }
        logical[ring] = None;
        placed[ring] = true;
        queue.push(ring);
        settle(&mut queue, &mut placed, &mut done);
    }
}

fn too_deep(parents: impl Fn(usize) -> Option<usize>, count: usize, limit: usize) -> bool {
    let mut depth = vec![usize::MAX; count];
    for i in 0..count {
        let mut chain = Vec::new();
        let mut at = Some(i);
        let mut base = 0;
        while let Some(node) = at {
            if depth[node] != usize::MAX {
                base = depth[node];
                break;
            }
            chain.push(node);
            if chain.len() > count {
                return true;
            }
            at = parents(node);
        }
        for (k, &node) in chain.iter().rev().enumerate() {
            depth[node] = base + k + usize::from(at.is_some());
            if depth[node] > limit {
                return true;
            }
        }
    }
    false
}

fn draw(
    model: &Model,
    graph: &Graph,
    options: &RenderOptions,
    turned: bool,
) -> Result<RenderResult, PeleError> {
    let n = svg::num;
    let theme = &options.theme;
    let curve = options.config["flowchart"]["curve"].as_str();
    let size = options.font_size;
    let small = js_round(size * 0.875);
    let measurer = options.text_measurer();
    let wrap_width = number_option(options, "wrappingWidth", 200.0);
    let pad = options.padding;
    let root_dir = if turned {
        Dir::Tb
    } else {
        direction(model.direction())
    };
    let text = |raw: &str, font_size: f64, wrap: f64, style: u8| {
        text::layout_label(raw, true, measurer, font_size, wrap, style)
    };

    let mut index = HashMap::<&str, usize>::new();
    let mut layout = Layout {
        views: Vec::new(),
        cnodes: Vec::new(),
        logical: Vec::new(),
        graph,
        root_dir,
    };
    for (at, node) in graph.nodes.iter().enumerate() {
        if node.shape == Shape::NoteGroup {
            continue;
        }
        let view = layout.add(Some(at), kind_of(node), None);
        index.insert(node.id.as_str(), view);
    }
    let states = layout.views.len();
    let node_of = |layout: &Layout<'_>, i: usize| &graph.nodes[layout.views[i].node.unwrap_or(0)];

    for i in 0..states {
        let parent = node_of(&layout, i)
            .parent
            .as_deref()
            .and_then(|id| index.get(id).copied());
        layout.logical.push(match parent {
            Some(p) if p != i && node_of(&layout, p).is_group => Some(p),
            _ => None,
        });
    }
    break_rings(&mut layout.logical);
    let owners: Vec<Option<usize>> = (0..states)
        .map(|i| {
            node_of(&layout, i)
                .note_for
                .as_deref()
                .and_then(|id| index.get(id).copied())
        })
        .collect();
    for (i, owner) in owners.iter().enumerate() {
        if let Some(owner) = *owner {
            layout.logical[i] = layout.logical[owner];
        }
    }
    if too_deep(|i| layout.logical[i], states, 2 * MAX_DEPTH) {
        return Err(PeleError::limit(
            "State diagram composites are nested too deeply",
        ));
    }

    let mut degree = vec![0usize; states];
    let mut noted = vec![false; states];
    let mut appear = vec![f64::INFINITY; states];
    let mut appeared = 0.0;
    let mut counts = vec![0usize; 2 * states];
    for edge in &graph.edges {
        let (Some(&src), Some(&dst)) =
            (index.get(edge.start.as_str()), index.get(edge.end.as_str()))
        else {
            continue;
        };
        if edge.dashed {
            noted[if layout.views[src].kind == K::Note {
                dst
            } else {
                src
            }] = true;
            continue;
        }
        counts[2 * src] += 1;
        degree[src] = degree[src].max(counts[2 * src]);
        counts[2 * dst + 1] += 1;
        degree[dst] = degree[dst].max(counts[2 * dst + 1]);
        if appear[src] == f64::INFINITY {
            appear[src] = appeared;
            appeared += 1.0;
        }
        if appear[dst] == f64::INFINITY {
            appear[dst] = appeared;
            appeared += 1.0;
        }
    }

    for (i, &degree) in degree.iter().enumerate() {
        let node = node_of(&layout, i);
        let style = if node.css_styles.is_empty() {
            resolve_style(&node.css_compiled)
        } else {
            let mut all = node.css_compiled.clone();
            all.extend(node.css_styles.iter().cloned());
            resolve_style(&all)
        };
        let text_style =
            if style.bold { text::BOLD } else { 0 } | if style.italic { text::ITALIC } else { 0 };
        let font_size = style.font_size.unwrap_or(size);
        let kind = layout.views[i].kind;
        let across = layout.across(i);
        let mut c = None;
        let view = &mut layout.views[i];
        match kind {
            K::Composite => {
                view.label = text(&node.label, font_size, 4000.0, text_style);
                view.title_h = if view.label.height > 0.0 {
                    view.label.height + 12.0
                } else {
                    0.0
                };
                let dir = direction(node.dir.unwrap_or("TB"));
                let mut g = group(dir, GROUP_PAD, view.title_h + GROUP_PAD, GROUP_PAD);
                g.min_w = view.label.width + 2.0 * GROUP_PAD;
                c = Some(g);
            }
            K::Region => {
                let dir = direction(node.dir.unwrap_or("TB"));
                c = Some(group(dir, REGION_PAD, REGION_PAD, REGION_PAD));
            }
            K::Start => {
                view.w = 14.0;
                view.h = 14.0;
            }
            K::End => {
                view.w = 18.0;
                view.h = 18.0;
            }
            K::Choice => {
                view.w = 30.0;
                view.h = 30.0;
            }
            K::Fork | K::Join => {
                let length = (40.0 * degree as f64 + 16.0).clamp(72.0, 320.0);
                view.w = if across { length } else { BAR };
                view.h = if across { BAR } else { length };
            }
            K::Hidden => {}
            K::Note => {
                view.label = text(&node.label, small, wrap_width.max(NOTE_WRAP), 0);
                view.w = view.label.width + 24.0;
                view.h = view.label.height + 16.0;
            }
            K::State | K::Described => {
                view.label = text(&node.label, font_size, wrap_width, text_style);
                let mut width = view.label.width;
                let mut height = view.label.height + 2.0 * PAD_Y;
                if kind == K::Described {
                    height += 14.0;
                    for line in &node.description {
                        let label = text(
                            if line.is_empty() { " " } else { line },
                            small,
                            wrap_width,
                            text_style,
                        );
                        width = width.max(label.width);
                        height += label.height;
                        view.lines.push(label);
                    }
                }
                view.w = (width + 2.0 * PAD_X).max(56.0);
                view.h = height;
            }
        }
        view.style = Some(style);
        let mut c = c.unwrap_or_else(|| cnode(view.w, view.h, None));
        if matches!(kind, K::State | K::Described) {
            c.span = (if across { view.w } else { view.h }) - 24.0;
        }
        layout.cnodes.push(c);
    }

    let mut cedges = Vec::new();
    let mut edge_labels = Vec::new();
    let mut drawn = Vec::new();
    let mut loop_count = vec![0usize; states];
    let mut loop_width = vec![0.0f64; states];
    let mut loop_height = vec![0.0f64; states];
    let mut anchors = HashMap::<usize, usize>::new();
    for edge in &graph.edges {
        let (Some(&src0), Some(&dst0)) =
            (index.get(edge.start.as_str()), index.get(edge.end.as_str()))
        else {
            continue;
        };
        let (mut src, mut dst) = (src0, dst0);
        let label = text(&edge.label, small, wrap_width, 0);
        let label_w = if label.width > 0.0 {
            label.width + 12.0
        } else {
            0.0
        };
        if src == dst {
            loop_count[src] += 1;
            loop_width[src] = loop_width[src].max(label_w);
            loop_height[src] += label.height;
        } else if layout.encloses(src, dst) {
            anchors.insert(cedges.len(), src);
            appear.push(-1.0);
            src = layout.add(None, K::Hidden, Some(cnode(0.0, 0.0, Some(src0))));
        } else if layout.encloses(dst, src) {
            anchors.insert(cedges.len(), dst);
            appear.push(f64::INFINITY);
            dst = layout.add(None, K::Hidden, Some(cnode(0.0, 0.0, Some(dst0))));
        }
        drawn.push(edge);
        cedges.push(CEdge {
            src,
            dst,
            minlen: 1,
            label_w,
            label_h: if label.height > 0.0 {
                label.height + 4.0
            } else {
                0.0
            },
            route: Vec::new(),
            label_x: 0.0,
            label_y: 0.0,
        });
        edge_labels.push(label);
    }

    for i in 0..states {
        if owners[i].is_some() {
            continue;
        }
        let across = layout.across(i);
        let mut room = 0.0;
        if loop_count[i] > 0 {
            let reach = LOOP + (loop_count[i] - 1) as f64 * 8.0;
            room = if across {
                reach + loop_width[i]
            } else {
                reach * 0.75 + 4.0 + loop_height[i]
            };
            let item = &mut layout.cnodes[i];
            if !item.is_group {
                if across {
                    item.w += 2.0 * room;
                    item.h = item.h.max(loop_height[i]);
                } else {
                    item.h += 2.0 * room;
                    item.w = item.w.max(loop_width[i]);
                }
                room = 0.0;
            }
        }
        if !noted[i] && room == 0.0 {
            layout.cnodes[i].parent = layout.logical[i];
            continue;
        }
        let mut c = if across {
            group(Dir::Lr, room, 0.0, 0.0)
        } else {
            group(Dir::Tb, 0.0, room, room)
        };
        c.parent = layout.logical[i];
        appear.push(appear[i]);
        let wrapper = layout.add(None, K::Hidden, Some(c));
        layout.cnodes[i].parent = Some(wrapper);
    }
    for (i, owner) in owners.iter().enumerate() {
        if let Some(owner) = *owner {
            layout.cnodes[i].parent = layout.cnodes[owner].parent;
        }
    }

    {
        let count = layout.cnodes.len();
        let mut members = vec![Vec::new(); count];
        let mut top = Vec::new();
        for i in 0..count {
            match layout.cnodes[i].parent {
                Some(p) => members[p].push(i),
                None => top.push(i),
            }
        }
        let initial = |i: usize| usize::from(layout.views[i].kind != K::Start);
        let by_flow = |a: &usize, b: &usize| {
            initial(*a)
                .cmp(&initial(*b))
                .then_with(|| {
                    appear[*a]
                        .partial_cmp(&appear[*b])
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| a.cmp(b))
        };
        top.sort_by(by_flow);
        top.reverse();
        let mut pending = top;
        let mut seq = 0;
        while let Some(i) = pending.pop() {
            layout.cnodes[i].seq = seq;
            seq += 1;
            let mut inside = std::mem::take(&mut members[i]);
            inside.sort_by(by_flow);
            pending.extend(inside.into_iter().rev());
        }
    }

    let layout_options = LayeredOptions {
        node_sep: number_option(options, "nodeSpacing", NODE_SEP),
        edge_sep: EDGE_SEP,
        rank_sep: number_option(options, "rankSpacing", RANK_SEP),
        port_sep: 20.0,
        tracks: matches!(curve, Some("step" | "stepBefore" | "stepAfter")),
        straight: curve == Some("linear"),
        head_room: marker_trim("arrow_point"),
        ..LayeredOptions::default()
    };
    let result = compound_layout(&mut layout.cnodes, &mut cedges, root_dir, &layout_options);
    let Layout {
        views,
        cnodes,
        logical,
        ..
    } = layout;
    let across = |i: usize| match logical[i] {
        None => root_dir.vertical(),
        Some(p) => direction(
            views[p]
                .node
                .and_then(|node| graph.nodes[node].dir)
                .unwrap_or("TB"),
        )
        .vertical(),
    };

    let width = (result.width + 2.0 * pad).ceil();
    let height = (result.height + 2.0 * pad).ceil();
    let mut links = Vec::new();

    let mut crossings = HashMap::<i64, Vec<f64>>::new();
    for e in &cedges {
        let route = &e.route;
        let mut k = 3;
        while k + 3 < route.len() {
            crossings
                .entry(row(route[k + 1]))
                .or_default()
                .push(route[k]);
            k += 3;
        }
    }

    let mut children = vec![Vec::new(); cnodes.len()];
    let mut order = Vec::new();
    for (i, c) in cnodes.iter().enumerate() {
        match c.parent {
            Some(p) => children[p].push(i),
            None => order.push(i),
        }
    }
    let roots = order.clone();
    let mut q = 0;
    while q < order.len() {
        let more = children[order[q]].clone();
        order.extend(more);
        q += 1;
    }

    let surface = svg::esc(&theme.surface);
    let border = svg::esc(&theme.border);
    let line = svg::esc(&theme.line);
    let background = svg::esc(&theme.background);
    let surface_attrs = format!(" fill=\"{surface}\" stroke=\"{border}\"");
    let rule = format!(" fill=\"none\" stroke=\"{border}\"");

    let mut clusters = String::new();
    let mut fills = Vec::new();
    let mut dividers = String::new();
    let mut nodes_out = String::new();
    let mut notes_out = String::new();
    let divide =
        |dividers: &mut String, items: &[usize], left: f64, top: f64, right: f64, bottom: f64| {
            let regions: Vec<usize> = items
                .iter()
                .copied()
                .filter(|&i| views[i].kind == K::Region)
                .collect();
            if regions.len() < 2 {
                return;
            }
            let lo = |i: usize, x: bool| {
                if x {
                    cnodes[i].x - cnodes[i].w / 2.0
                } else {
                    cnodes[i].y - cnodes[i].h / 2.0
                }
            };
            let hi = |i: usize, x: bool| {
                if x {
                    cnodes[i].x + cnodes[i].w / 2.0
                } else {
                    cnodes[i].y + cnodes[i].h / 2.0
                }
            };
            for x in [true, false] {
                let mut sorted = regions.clone();
                sorted.sort_by(|&a, &b| {
                    (lo(a, x) - lo(b, x))
                        .partial_cmp(&0.0)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| a.cmp(&b))
                });
                if sorted
                    .windows(2)
                    .any(|pair| hi(pair[0], x) > lo(pair[1], x) + 0.5)
                {
                    continue;
                }
                for pair in sorted.windows(2) {
                    let at = n((hi(pair[0], x) + lo(pair[1], x)) / 2.0 + pad);
                    if x {
                        let _ = write!(
                            dividers,
                            "<path class=\"pele-divider\" d=\"M{at},{}V{}\"/>",
                            n(top),
                            n(bottom)
                        );
                    } else {
                        let _ = write!(
                            dividers,
                            "<path class=\"pele-divider\" d=\"M{},{at}H{}\"/>",
                            n(left),
                            n(right)
                        );
                    }
                }
                return;
            }
            for &i in &regions {
                let c = &cnodes[i];
                dividers.push_str(&rect(
                    c.x - c.w / 2.0 + pad,
                    c.y - c.h / 2.0 + pad,
                    c.w,
                    c.h,
                    &format!(" class=\"pele-divider\" rx=\"{}\"", n(CORNER)),
                ));
            }
        };
    divide(
        &mut dividers,
        &roots,
        pad / 2.0,
        pad / 2.0,
        width - pad / 2.0,
        height - pad / 2.0,
    );

    for &i in &order {
        let view = &views[i];
        let (Some(at), Some(style)) = (view.node, view.style.as_ref()) else {
            continue;
        };
        if view.kind >= K::Region {
            continue;
        }
        let node = &graph.nodes[at];
        let c = &cnodes[i];
        let x = c.x + pad;
        let y = c.y + pad;
        let (w, h) = (view.w, view.h);
        let ink = ink_fill(style, None);
        let label_attrs = format!(" class=\"pele-label\"{ink}{}", style.text);
        let mut body = match view.kind {
            K::Composite => {
                let left = x - c.w / 2.0;
                let top = y - c.h / 2.0;
                if paints(&style.shape) {
                    fills.push(Fill {
                        bounds: [left, top, left + c.w, top + c.h],
                        shape: style.shape.clone(),
                    });
                }
                let mut body = rect(
                    left,
                    top,
                    c.w,
                    c.h,
                    &format!(
                        " rx=\"{}\" fill=\"{}\"{} stroke=\"{border}\"{}",
                        n(CORNER),
                        svg::esc(&theme.surface_alt),
                        if sets_fill(&style.shape) {
                            ""
                        } else {
                            " fill-opacity=\"0.5\""
                        },
                        style.shape
                    ),
                );
                if view.title_h > 0.0 {
                    let _ = write!(
                        body,
                        "<path d=\"M{},{}H{}\"{rule}{}/>",
                        n(left),
                        n(top + view.title_h),
                        n(left + c.w),
                        style.line
                    );
                    body.push_str(&svg::label_with_options(
                        &view.label,
                        title_x(c, view.label.width, &crossings) + pad,
                        top + view.title_h / 2.0,
                        &format!(" class=\"pele-cluster-label\"{ink}{}", style.text),
                        options,
                    ));
                }
                divide(
                    &mut dividers,
                    &children[i],
                    left,
                    top + view.title_h,
                    left + c.w,
                    top + c.h,
                );
                body
            }
            K::Start => format!("<circle r=\"7\" fill=\"{line}\"{}/>", style.shape),
            K::End => format!(
                "<circle r=\"8.5\" fill=\"{background}\" stroke=\"{line}\"{}/><circle r=\"5\" fill=\"{line}\"{}/>",
                style.line, style.shape
            ),
            K::Choice => format!(
                "<polygon points=\"0,{} {},0 0,{} {},0\"{surface_attrs}{}/>",
                n(-h / 2.0),
                n(w / 2.0),
                n(h / 2.0),
                n(-w / 2.0),
                style.shape
            ),
            K::Fork | K::Join => rect(
                -w / 2.0,
                -h / 2.0,
                w,
                h,
                &format!(" rx=\"2\" fill=\"{line}\"{}", style.shape),
            ),
            K::Note => {
                let r = n(w / 2.0);
                let b = n(h / 2.0);
                let fold = n(w / 2.0 - FOLD);
                let corner = n(FOLD - h / 2.0);
                format!(
                    "<path d=\"M-{r},-{b}H{fold}L{r},{corner}V{b}H-{r}Z\" fill=\"{background}\" stroke=\"{border}\"/><path d=\"M{fold},-{b}V{corner}H{r}\"{rule}/>{}",
                    svg::label_with_options(
                        &view.label,
                        0.0,
                        0.0,
                        &format!(
                            " class=\"pele-label\" font-size=\"{}\" fill=\"{}\"",
                            n(small),
                            svg::esc(&theme.muted)
                        ),
                        options
                    )
                )
            }
            _ => {
                let mut body = rect(
                    -w / 2.0,
                    -h / 2.0,
                    w,
                    h,
                    &format!(
                        " rx=\"{}\"{surface_attrs}{}",
                        n(CORNER.min(h / 2.0)),
                        style.shape
                    ),
                );
                if view.kind == K::State {
                    body.push_str(&svg::label_with_options(
                        &view.label,
                        0.0,
                        0.0,
                        &label_attrs,
                        options,
                    ));
                } else {
                    let mut at = PAD_Y - h / 2.0 + view.label.height;
                    let _ = write!(
                        body,
                        "<path d=\"M{},{}H{}\"{rule}{}/>",
                        n(-w / 2.0),
                        n(at + 6.0),
                        n(w / 2.0),
                        style.line
                    );
                    body.push_str(&svg::label_with_options(
                        &view.label,
                        0.0,
                        at - view.label.height / 2.0,
                        &label_attrs,
                        options,
                    ));
                    at += 14.0;
                    for line in &view.lines {
                        body.push_str(&svg::label_with_options(
                            line,
                            0.0,
                            at + line.height / 2.0,
                            &format!(
                                " class=\"pele-state-description\" font-size=\"{}\"{ink}{}",
                                n(small),
                                style.text
                            ),
                            options,
                        ));
                        at += line.height;
                    }
                }
                body
            }
        };
        if let Some((url, tooltip)) = model.links.get(&node.id) {
            let tooltip = text::decode_entities(unquote(tooltip));
            if !tooltip.is_empty() {
                body = format!("<title>{}</title>{body}", svg::esc(&tooltip));
            }
            if let Some(href) = link_url(&safe_url(unquote(url))) {
                body = format!(
                    "<a href=\"{}\" target=\"_blank\" rel=\"noopener\">{body}</a>",
                    svg::esc(&href)
                );
                links.push(LinkInfo {
                    id: text::decode_entities(&node.id),
                    href,
                    internal: false,
                });
            }
        }
        let open = format!(
            "<g class=\"{}{}\" data-id=\"{}\"",
            CLASSES[view.kind as usize],
            class_names(&without_mermaid_classes(&node.css_classes)),
            svg::esc_text(&node.id)
        );
        match view.kind {
            K::Composite => {
                let _ = write!(clusters, "{open}>{body}</g>");
            }
            K::Note => {
                let _ = write!(
                    notes_out,
                    "{open} transform=\"translate({},{})\">{body}</g>",
                    n(x),
                    n(y)
                );
            }
            _ => {
                let _ = write!(
                    nodes_out,
                    "{open} transform=\"translate({},{})\">{body}</g>",
                    n(x),
                    n(y)
                );
            }
        }
    }

    let attach = |route: &mut [f64], at: usize, i: usize| {
        let c = &cnodes[i];
        if c.is_group || route.len() < at + 3 {
            return;
        }
        let view = &views[i];
        let bar = matches!(view.kind, K::Fork | K::Join);
        let other = if at == 0 { 3 } else { at - 3 };
        for axis in [1, 0] {
            let (extent, drawn_extent, breadth) = if axis == 1 {
                (c.h, view.h, view.w)
            } else {
                (c.w, view.w, view.h)
            };
            let center = (if axis == 1 { c.y } else { c.x }) + pad;
            let cross = (if axis == 1 { c.x } else { c.y }) + pad;
            let delta = route[at + axis] - center;
            if (delta.abs() - extent / 2.0).abs() >= 0.5 {
                continue;
            }
            route[at + axis] -= js_sign(delta) * (extent - drawn_extent) / 2.0;
            if bar && breadth > drawn_extent && other + 3 <= route.len() {
                let value = route[other + 1 - axis];
                let lo = cross - breadth / 2.0 + 6.0;
                let hi = cross + breadth / 2.0 - 6.0;
                route[at + 1 - axis] = if value < lo {
                    lo
                } else if value > hi {
                    hi
                } else {
                    value
                };
            }
            return;
        }
    };

    let mut reach = side_reach();
    for (ce, edge) in cedges.iter().zip(&drawn) {
        if ce.src == ce.dst || ce.route.len() < 6 || edge.dashed {
            continue;
        }
        reach.add(
            ce.dst,
            end_side(&ce.route, ce.route.len() - 3, &cnodes[ce.dst]),
            "arrow_point",
        );
    }

    let mut casings = String::new();
    let mut edges_out = String::new();
    let mut labels_out = String::new();
    let mut loop_seen = vec![0usize; states];
    let mut loop_text = vec![0.0f64; states];
    for i in 0..cedges.len() {
        let edge = drawn[i];
        let label = &edge_labels[i];
        let end_type = if edge.dashed { "none" } else { "arrow_point" };
        let path = if cedges[i].src == cedges[i].dst {
            let src = cedges[i].src;
            let a = &cnodes[src];
            let view = &views[src];
            let k = loop_seen[src] as f64;
            loop_seen[src] += 1;
            let side = across(src);
            let w = if a.is_group { a.w } else { view.w };
            let h = if a.is_group { a.h } else { view.h };
            let out = (if side { w } else { h }) / 2.0;
            let spread = ((if side { h } else { w }) / 2.0 - 4.0)
                .min(8.0 + k * 6.0)
                .max(2.0);
            let reach = LOOP + k * 8.0;
            let hyp = reach.hypot(spread);
            let trim = marker_trim(end_type) / if hyp == 0.0 { 1.0 } else { hyp };
            let px = |dx: f64, dy: f64| a.x + pad + if side { out + dx } else { -dy };
            let py = |dx: f64, dy: f64| a.y + pad + if side { dy } else { out + dx };
            let at = |dx: f64, dy: f64| format!("{},{}", n(px(dx, dy)), n(py(dx, dy)));
            let mut edx = px(-reach, -spread) - px(0.0, 0.0);
            let mut edy = py(-reach, -spread) - py(0.0, 0.0);
            let len = edx.hypot(edy);
            let len = if len == 0.0 { 1.0 } else { len };
            edx /= len;
            edy /= len;
            let path = EdgePath {
                d: format!(
                    "M{}C{} {} {}",
                    at(0.0, -spread),
                    at(reach, -2.0 * spread),
                    at(reach, 2.0 * spread),
                    at(reach * trim, spread + spread * trim)
                ),
                tokens: None,
                sx: 0.0,
                sy: 0.0,
                sdx: 0.0,
                sdy: 0.0,
                ex: px(0.0, spread),
                ey: py(0.0, spread),
                edx,
                edy,
            };
            let furthest = LOOP + (loop_count[src] as f64 - 1.0) * 8.0;
            let below = loop_text[src] + label.height / 2.0;
            loop_text[src] += label.height;
            let (ax, ay) = (a.x, a.y);
            let ce = &mut cedges[i];
            ce.label_x = if side {
                ax + out + furthest + 4.0 + ce.label_w / 2.0
            } else {
                ax
            };
            ce.label_y = if side {
                ay - loop_height[src] / 2.0 + below
            } else {
                ay + out + furthest * 0.75 + 4.0 + below
            };
            path
        } else {
            let ce = &cedges[i];
            let mut route = ce.route.clone();
            let ends = route.len() >= 3;
            let last = route.len().saturating_sub(3);
            let leaves = if ends {
                reach.of(ce.src, end_side(&route, 0, &cnodes[ce.src]))
            } else {
                0.0
            };
            let arrives = if ends {
                reach.of(ce.dst, end_side(&route, last, &cnodes[ce.dst]))
            } else {
                0.0
            };
            for point in route.chunks_mut(3) {
                point[0] += pad;
                if point.len() > 1 {
                    point[1] += pad;
                }
            }
            if ends {
                attach(&mut route, 0, ce.src);
                attach(&mut route, last, ce.dst);
            }
            if let Some(&composite) = anchors.get(&i)
                && route.len() >= 6
            {
                let g = &cnodes[composite];
                let inward = views[ce.src].kind == K::Hidden;
                let at = if inward { 0 } else { route.len() - 3 };
                let gdir = g.dir.unwrap_or_default();
                let near = matches!(gdir, Dir::Tb | Dir::Lr) == inward;
                let point = if gdir.vertical() {
                    [
                        route[at],
                        g.y + pad
                            + if near {
                                views[composite].title_h - g.h / 2.0
                            } else {
                                g.h / 2.0
                            },
                        0.0,
                    ]
                } else {
                    [
                        g.x + pad + if near { -g.w / 2.0 } else { g.w / 2.0 },
                        route[at + 1],
                        1.0,
                    ]
                };
                route = if inward {
                    point.iter().copied().chain(route).collect()
                } else {
                    route.into_iter().chain(point).collect()
                };
            }
            route_path(
                &route,
                curve,
                0.0,
                marker_trim(end_type),
                Some(leaves),
                Some(arrives),
            )
        };
        let over = over_fills(
            &path.d,
            &[path.ex, path.ey],
            &fills,
            "",
            path.tokens.as_deref(),
        );
        if over.as_deref() == Some("") {
            casings.push_str(&edge_casing(
                &path,
                "none",
                end_type,
                "",
                false,
                &theme.background,
            ));
        }
        let ink = over.as_deref().filter(|ink| !ink.is_empty());
        let kind = if edge.dashed {
            format!(
                "pele-note-edge\" stroke=\"{}\" stroke-dasharray=\"3 4",
                ink.map_or(border.as_str(), |ink| ink)
            )
        } else if let Some(ink) = ink {
            format!("pele-transition\" stroke=\"{ink}")
        } else {
            "pele-transition".to_owned()
        };
        let _ = write!(
            edges_out,
            "<g class=\"pele-edge {kind}\" data-id=\"{}\"><path d=\"{}\"/>{}</g>",
            svg::esc_text(&edge.id),
            path.d,
            marker(
                end_type,
                path.ex,
                path.ey,
                path.edx,
                path.edy,
                ink.map_or(line.as_str(), |ink| ink),
                "none"
            )
        );
        if label.width > 0.0 {
            labels_out.push_str(&edge_label_svg(
                &edge.id,
                label,
                cedges[i].label_x + pad,
                cedges[i].label_y + pad,
                "",
                options,
            ));
        }
    }

    let mut content = String::new();
    if !clusters.is_empty() {
        let _ = write!(content, "<g class=\"pele-clusters\">{clusters}</g>");
    }
    if !dividers.is_empty() {
        let _ = write!(
            content,
            "<g class=\"pele-dividers\" stroke-dasharray=\"4 4\"{rule}>{dividers}</g>"
        );
    }
    content.push_str(&casings_svg(&casings, &theme.background));
    if !edges_out.is_empty() {
        let _ = write!(
            content,
            "<g class=\"pele-edges\" fill=\"none\" stroke=\"{line}\" stroke-linecap=\"round\">{edges_out}</g>"
        );
    }
    if !labels_out.is_empty() {
        let _ = write!(
            content,
            "<g class=\"pele-edge-labels\" font-size=\"{}\">{labels_out}</g>",
            n(small)
        );
    }
    let _ = write!(
        content,
        "<g class=\"pele-nodes\">{nodes_out}{notes_out}</g>"
    );
    let titled = with_title(
        options.title.as_deref(),
        options,
        size,
        pad,
        width,
        height,
        content,
    );
    let mut effective = options.clone();
    effective.acc_title = model.acc_title.clone().or(effective.acc_title);
    effective.acc_descr = model.acc_descr.clone().or(effective.acc_descr);
    let svg = svg::document(
        "state",
        titled.width,
        titled.height,
        &effective,
        &titled.content,
    );
    let mut result = RenderResult::new("state", svg, titled.width, titled.height);
    result.links = links;
    Ok(result)
}

const BLANK: &str = "about:blank";
const LINK_SCHEMES: [&str; 4] = ["http", "https", "mailto", "tel"];

fn control(ch: char) -> bool {
    matches!(ch, '\u{0}'..='\u{1f}' | '\u{7f}'..='\u{9f}' | '\u{2000}'..='\u{200d}' | '\u{feff}' | '\u{fffe}' | '\u{ffff}')
}

fn decode_uri(text: &str) -> String {
    if !text.contains('%') {
        return text.to_owned();
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let digits = bytes
                .get(i + 1..i + 3)
                .and_then(|pair| std::str::from_utf8(pair).ok())
                .and_then(|pair| u8::from_str_radix(pair, 16).ok());
            let Some(value) = digits else {
                return text.to_owned();
            };
            out.push(value);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_owned())
}

fn char_code(digits: &str) -> Option<char> {
    let lower = digits.to_ascii_lowercase();
    let value = if let Some(hex) = lower.strip_prefix("0x") {
        u32::from_str_radix(hex, 16).ok()
    } else if let Some(octal) = lower.strip_prefix("0o") {
        u32::from_str_radix(octal, 8).ok()
    } else if let Some(binary) = lower.strip_prefix("0b") {
        u32::from_str_radix(binary, 2).ok()
    } else if digits.bytes().all(|b| b.is_ascii_digit()) {
        Some(digits.bytes().fold(0u32, |acc, b| {
            acc.wrapping_mul(10).wrapping_add(u32::from(b - b'0')) & 0xffff
        }))
    } else {
        None
    };
    char::from_u32(value.unwrap_or(0) & 0xffff)
}

fn numeric_entities(text: &str) -> (String, bool) {
    let mut out = String::with_capacity(text.len());
    let mut found = false;
    let mut rest = text;
    while let Some(at) = rest.find("&#") {
        let after = &rest[at + 2..];
        let len = after
            .bytes()
            .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_')
            .count();
        if len == 0 {
            out.push_str(&rest[..at + 2]);
            rest = after;
            continue;
        }
        found = true;
        out.push_str(&rest[..at]);
        if let Some(ch) = char_code(&after[..len]) {
            out.push(ch);
        }
        rest = after[len..].strip_prefix(';').unwrap_or(&after[len..]);
    }
    out.push_str(rest);
    (out, found)
}

fn control_entities(text: &str) -> (String, bool) {
    let mut out = String::with_capacity(text.len());
    let mut found = false;
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        let lower = after.get(..8).map(str::to_ascii_lowercase);
        let short = after.get(..4).map(str::to_ascii_lowercase);
        if lower.as_deref() == Some("newline;") {
            found = true;
            rest = &after[8..];
        } else if short.as_deref() == Some("tab;") {
            found = true;
            rest = &after[4..];
        } else {
            out.push('&');
            rest = after;
        }
    }
    out.push_str(rest);
    (out, found)
}

fn whitespace_escapes(text: &str) -> (String, bool) {
    let mut out = String::with_capacity(text.len());
    let mut found = false;
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let slash = if bytes[i] == b'\\' {
            Some(1)
        } else if bytes[i] == b'%'
            && bytes.get(i + 1) == Some(&b'5')
            && matches!(bytes.get(i + 2), Some(b'c' | b'C'))
        {
            Some(3)
        } else {
            None
        };
        if let Some(slash) = slash {
            let j = i + slash;
            let escape = match bytes.get(j) {
                Some(b'n' | b'r' | b't') => Some(1),
                Some(b'%') => match (bytes.get(j + 1), bytes.get(j + 2)) {
                    (Some(b'6'), Some(b'e' | b'E')) | (Some(b'7'), Some(b'2' | b'4')) => Some(3),
                    _ => None,
                },
                _ => None,
            };
            if let Some(escape) = escape {
                found = true;
                i = j + escape;
                continue;
            }
        }
        let ch = text[i..].chars().next().unwrap_or('\0');
        out.push(ch);
        i += ch.len_utf8().max(1);
    }
    (out, found)
}

fn unwrap_url(url: &str, percent: bool) -> Option<String> {
    let trimmed = js_trim(url);
    let mut out = if percent {
        decode_uri(trimmed)
    } else {
        trimmed.to_owned()
    };
    for _ in 0..32 {
        let cleaned: String = out.chars().filter(|ch| !control(*ch)).collect();
        let (cleaned, _) = numeric_entities(&cleaned);
        let (cleaned, _) = control_entities(&cleaned);
        let cleaned: String = cleaned.chars().filter(|ch| !control(*ch)).collect();
        let (cleaned, _) = whitespace_escapes(&cleaned);
        out = js_trim(&cleaned).to_owned();
        if percent {
            out = decode_uri(&out);
        }
        let more = out.chars().any(control)
            || numeric_entities(&out).1
            || control_entities(&out).1
            || whitespace_escapes(&out).1;
        if !more {
            return Some(out);
        }
    }
    None
}

fn encode_part(text: &str, set: &[char]) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_ascii() && !ch.is_ascii_control() && !set.contains(&ch) {
            out.push(ch);
        } else {
            let mut buffer = [0; 4];
            for byte in ch.encode_utf8(&mut buffer).bytes() {
                let _ = write!(out, "%{byte:02X}");
            }
        }
    }
    out
}

fn web_url(url: &str) -> Option<String> {
    let colon = url.find(':')?;
    let scheme = url[..colon].to_ascii_lowercase();
    let rest = url[colon + 1..].trim_start_matches('/');
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    let host = authority.rsplit('@').next().unwrap_or(authority);
    if host.is_empty()
        || host.chars().any(|ch| {
            ch.is_whitespace()
                || control(ch)
                || matches!(ch, '<' | '>' | '^' | '|' | '%' | '[' | ']' | '\\')
        })
    {
        return None;
    }
    let authority = format!(
        "{}{}",
        &authority[..authority.len() - host.len()],
        host.to_ascii_lowercase()
    );
    let tail = &rest[end..];
    let (before_fragment, fragment) = match tail.find('#') {
        Some(at) => (&tail[..at], Some(&tail[at + 1..])),
        None => (tail, None),
    };
    let (path, query) = match before_fragment.find('?') {
        Some(at) => (&before_fragment[..at], Some(&before_fragment[at + 1..])),
        None => (before_fragment, None),
    };
    let mut out = format!("{scheme}://{authority}");
    if path.is_empty() {
        out.push('/');
    } else {
        out.push_str(&encode_part(path, &[' ', '"', '<', '>', '`', '{', '}']));
    }
    if let Some(query) = query {
        out.push('?');
        out.push_str(&encode_part(query, &[' ', '"', '<', '>', '\'']));
    }
    if let Some(fragment) = fragment {
        out.push('#');
        out.push_str(&encode_part(fragment, &[' ', '"', '<', '>', '`']));
    }
    Some(out)
}

fn sanitize_url(url: &str) -> String {
    if url.is_empty() {
        return BLANK.to_owned();
    }
    let (Some(decoded), Some(kept)) = (unwrap_url(url, true), unwrap_url(url, false)) else {
        return BLANK.to_owned();
    };
    if decoded.is_empty() || kept.is_empty() {
        return BLANK.to_owned();
    }
    if decoded.starts_with('.') || decoded.starts_with('/') {
        return kept;
    }
    let start = decoded.trim_start_matches(is_js_space);
    let line = start
        .split(['\n', '\r', '\u{2028}', '\u{2029}'])
        .next()
        .unwrap_or("");
    let colon = line.rfind(':').map(|at| at + 1);
    let entity = line.to_ascii_lowercase().rfind("&colon;").map(|at| at + 7);
    let Some(end) = colon.max(entity).filter(|end| *end > 1) else {
        return kept;
    };
    let protocol = js_trim(&line[..end].to_lowercase()).to_owned();
    let bare = protocol.trim_start_matches(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'));
    if ["javascript", "data", "vbscript"]
        .iter()
        .any(|bad| bare.starts_with(bad))
    {
        return BLANK.to_owned();
    }
    let normalized = kept.trim_start_matches(is_js_space).replace('\\', "/");
    if protocol == "mailto:" || protocol.contains("://") {
        return normalized;
    }
    if protocol == "http:" || protocol == "https:" {
        return web_url(&normalized).unwrap_or_else(|| BLANK.to_owned());
    }
    normalized
}

fn safe_url(url: &str) -> String {
    sanitize_url(&text::decode_entities(url))
}

fn allowed(url: &str) -> bool {
    let bytes = url.as_bytes();
    if bytes.len() >= 2 && matches!(bytes[0], b'/' | b'\\') && matches!(bytes[1], b'/' | b'\\') {
        return false;
    }
    let Some(colon) = url.find(':') else {
        return true;
    };
    let scheme = &url[..colon];
    let named = scheme
        .bytes()
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic())
        && scheme
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'.' | b'-'));
    !named || LINK_SCHEMES.contains(&scheme.to_ascii_lowercase().as_str())
}

fn link_url(url: &str) -> Option<String> {
    let decoded = unwrap_url(url, true)?;
    (allowed(url) && allowed(&decoded)).then(|| url.to_owned())
}
