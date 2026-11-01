use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write;
use std::sync::LazyLock;

use indexmap::IndexMap;
use regex::Regex;

use crate::compound::{CEdge, CNode, Dir, cnode, compound_layout, direction, shift_layout};
use crate::fit_width::{runs_across, turn_to_fit};
use crate::layout::LayeredOptions;
use crate::svg::cluster::{Crossings, cluster_title_x, js_round, mark_crossings};
use crate::svg::edges::{
    Fill, FillUnder, casing, casings_svg, edge_ink, edge_label_svg, fill_under, hollow_on,
    route_path,
};
use crate::svg::ink::{ON_DARK, ON_LIGHT, ink, ink_fill, paints};
use crate::svg::theme::{
    ResolvedStyle, class_names, resolve_style, rounded, rounded_attr, sets_fill,
};
use crate::svg::title::title_label;
use crate::svg::{esc, esc_text, label_with_options, num, shared_style};
use crate::text::{self, BOLD, ITALIC, Label, MONO, SUB, SUP, UNDERLINE};
use crate::{LinkInfo, PeleError, RenderOptions, RenderResult};

const NODE_SEP: f64 = 48.0;
const EDGE_SEP: f64 = 20.0;
const RANK_SEP: f64 = 64.0;
const GROUP_PAD: f64 = 20.0;
const LOOP: f64 = 28.0;
const LOOP_BAND: f64 = 38.0;
const MIN_WIDTH: f64 = 96.0;
const BOX_PAD_X: f64 = 18.0;
const BOX_PAD_Y: f64 = 12.0;
const HEAD_PAD_Y: f64 = 9.0;
const CELL_PAD_X: f64 = 12.0;
const CELL_GAP: f64 = 16.0;
const ROW_PAD: f64 = 9.0;
const COMMENT_WRAP: f64 = 260.0;
const NO_WRAP: f64 = 1e6;
const PORT_GAP: f64 = 18.0;
const PORT_MARGIN: f64 = 14.0;
const MARKER_LENGTH: f64 = 18.0;
const BAR: f64 = 5.0;
const FOOT: f64 = 6.0;
const FOOT_LENGTH: f64 = 10.0;
const RING: f64 = 3.5;
const MAX_DEPTH: usize = 128;
const MAX_ENTITIES: usize = 5000;
const MAX_ATTRIBUTES: usize = 50_000;

const NAME: &str = "'UNICODE_TEXT', 'NUM', 'ENTITY_NAME', 'DECIMAL_NUM', 'ENTITY_ONE'";
const CARDS: &str = "'ZERO_OR_ONE', 'ZERO_OR_MORE', 'ONE_OR_MORE', 'ONLY_ONE', 'MD_PARENT'";
const LINE: &str = "'EOF', 'NEWLINE', 'title', 'acc_title', 'acc_descr', 'acc_descr_multiline_value', 'SUBGRAPH', 'direction_tb', 'direction_bt', 'direction_rl', 'direction_lr', 'CLASSDEF', 'CLASS', 'STYLE', 'UNICODE_TEXT', 'NUM', 'ENTITY_NAME', 'DECIMAL_NUM', 'ENTITY_ONE'";
const DIRECTIONS: [&str; 4] = ["TB", "BT", "RL", "LR"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum T {
    End,
    Error,
    ErDiagram,
    Eof,
    Newline,
    Colon,
    StyleSeparator,
    BlockStart,
    BlockStop,
    Sqs,
    Sqe,
    AccTitle,
    AccTitleValue,
    AccDescr,
    AccDescrValue,
    AccDescrMultilineValue,
    EndKw,
    Subgraph,
    DirectionTb,
    DirectionBt,
    DirectionRl,
    DirectionLr,
    ClassDef,
    UnicodeText,
    StyleText,
    Comma,
    Class,
    Style,
    Semi,
    Num,
    Brkt,
    EntityName,
    DecimalNum,
    EntityOne,
    AttributeWord,
    AttributeKey,
    Comment,
    ZeroOrOne,
    ZeroOrMore,
    OneOrMore,
    OnlyOne,
    MdParent,
    NonIdentifying,
    Identifying,
    Word,
    Char,
}

impl T {
    fn name(self) -> &'static str {
        match self {
            Self::End => "$end",
            Self::Error => "INVALID",
            Self::ErDiagram => "ER_DIAGRAM",
            Self::Eof => "EOF",
            Self::Newline => "NEWLINE",
            Self::Colon => "COLON",
            Self::StyleSeparator => "STYLE_SEPARATOR",
            Self::BlockStart => "BLOCK_START",
            Self::BlockStop => "BLOCK_STOP",
            Self::Sqs => "SQS",
            Self::Sqe => "SQE",
            Self::AccTitle => "acc_title",
            Self::AccTitleValue => "acc_title_value",
            Self::AccDescr => "acc_descr",
            Self::AccDescrValue => "acc_descr_value",
            Self::AccDescrMultilineValue => "acc_descr_multiline_value",
            Self::EndKw => "END",
            Self::Subgraph => "SUBGRAPH",
            Self::DirectionTb => "direction_tb",
            Self::DirectionBt => "direction_bt",
            Self::DirectionRl => "direction_rl",
            Self::DirectionLr => "direction_lr",
            Self::ClassDef => "CLASSDEF",
            Self::UnicodeText => "UNICODE_TEXT",
            Self::StyleText => "STYLE_TEXT",
            Self::Comma => "COMMA",
            Self::Class => "CLASS",
            Self::Style => "STYLE",
            Self::Semi => "SEMI",
            Self::Num => "NUM",
            Self::Brkt => "BRKT",
            Self::EntityName => "ENTITY_NAME",
            Self::DecimalNum => "DECIMAL_NUM",
            Self::EntityOne => "ENTITY_ONE",
            Self::AttributeWord => "ATTRIBUTE_WORD",
            Self::AttributeKey => "ATTRIBUTE_KEY",
            Self::Comment => "COMMENT",
            Self::ZeroOrOne => "ZERO_OR_ONE",
            Self::ZeroOrMore => "ZERO_OR_MORE",
            Self::OneOrMore => "ONE_OR_MORE",
            Self::OnlyOne => "ONLY_ONE",
            Self::MdParent => "MD_PARENT",
            Self::NonIdentifying => "NON_IDENTIFYING",
            Self::Identifying => "IDENTIFYING",
            Self::Word => "WORD",
            Self::Char => "CHAR",
        }
    }

    fn cardinality(self) -> Option<Cardinality> {
        Some(match self {
            Self::ZeroOrOne => Cardinality::ZeroOrOne,
            Self::ZeroOrMore => Cardinality::ZeroOrMore,
            Self::OneOrMore => Cardinality::OneOrMore,
            Self::OnlyOne => Cardinality::OnlyOne,
            Self::MdParent => Cardinality::MdParent,
            _ => return None,
        })
    }

    fn is_name(self) -> bool {
        matches!(
            self,
            Self::UnicodeText | Self::EntityName | Self::Num | Self::DecimalNum | Self::EntityOne
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cardinality {
    ZeroOrOne,
    ZeroOrMore,
    OneOrMore,
    OnlyOne,
    MdParent,
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

fn is_digit(c: u32) -> bool {
    (48..=57).contains(&c)
}

fn is_word(c: u32) -> bool {
    is_digit(c) || (65..=90).contains(&c) || (97..=122).contains(&c) || c == 95
}

fn is_line_end(c: u32) -> bool {
    matches!(c, 10 | 13 | 0x2028 | 0x2029)
}

fn js_trim(text: &str) -> &str {
    text.trim_matches(|ch: char| is_space(ch as u32))
}

const TEXT: u8 = 1;
const STYLE_TEXT: u8 = 2;
const ATTR_START: u8 = 4;
const ATTR_REST: u8 = 8;

fn class(c: u32) -> u8 {
    let mut kind = 0;
    if is_word(c) {
        kind |= TEXT | STYLE_TEXT | ATTR_REST;
        if !is_digit(c) {
            kind |= ATTR_START;
        }
    }
    match c {
        45 => kind |= TEXT | STYLE_TEXT | ATTR_REST,
        42 => kind |= TEXT | STYLE_TEXT | ATTR_REST | ATTR_START,
        46 => kind |= TEXT | ATTR_REST,
        91 | 93 | 40 | 41 | 44 => kind |= ATTR_REST,
        _ => {}
    }
    kind
}

fn is_attr(c: u32, kind: u8) -> bool {
    if c < 128 {
        class(c) & kind != 0
    } else {
        c >= 0xc0 || c == 0xb5
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum S {
    Initial,
    Block,
    BlockBq,
    AccTitle,
    AccDescr,
    AccDescrMultiline,
    Style,
}

#[derive(Clone, Copy)]
struct Token {
    kind: T,
    start: usize,
    end: usize,
}

struct Lexer<'a> {
    src: &'a [u16],
    n: usize,
    p: usize,
    state: S,
    tokens: Vec<Token>,
    dir_next: isize,
    dir_failed: usize,
    line_end: isize,
    tilde_next: isize,
    tilde_line: isize,
    tilde_last: isize,
    ws_next: isize,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a [u16]) -> Self {
        Self {
            src,
            n: src.len(),
            p: 0,
            state: S::Initial,
            tokens: Vec::new(),
            dir_next: -2,
            dir_failed: 0,
            line_end: -1,
            tilde_next: -2,
            tilde_line: -1,
            tilde_last: -1,
            ws_next: -1,
        }
    }

    fn at(&self, q: usize) -> u32 {
        self.src.get(q).map_or(0, |&c| u32::from(c))
    }

    fn emit(&mut self, kind: T, start: usize, end: usize) {
        self.tokens.push(Token { kind, start, end });
        self.p = end;
    }

    fn index_of(&self, unit: u16, from: usize) -> Option<usize> {
        self.src
            .get(from..)?
            .iter()
            .position(|&c| c == unit)
            .map(|at| from + at)
    }

    fn kw(&self, word: &str) -> bool {
        word.bytes().enumerate().all(|(i, k)| {
            let k = u32::from(k);
            let c = self.at(self.p + i);
            c == k || ((97..=122).contains(&k) && c | 32 == k)
        })
    }

    fn kwb(&self, word: &str) -> bool {
        self.kw(word) && !is_word(self.at(self.p + word.len()))
    }

    fn lower(&self, q: usize) -> u32 {
        let c = self.at(q);
        if (65..=90).contains(&c) { c + 32 } else { c }
    }

    fn next_line_end(&self, q: usize) -> usize {
        (q..self.n)
            .find(|&at| is_line_end(self.at(at)))
            .unwrap_or(self.n)
    }

    fn digits(&self, mut q: usize) -> usize {
        while is_digit(self.at(q)) {
            q += 1;
        }
        q
    }

    fn skip_spaces(&self, mut q: usize) -> usize {
        while is_space(self.at(q)) {
            q += 1;
        }
        q
    }

    fn acc(&self, word: &str, close: u32) -> Option<usize> {
        if !self.kw(word) {
            return None;
        }
        let q = self.skip_spaces(self.p + word.len());
        (self.at(q) == close).then(|| self.skip_spaces(q + 1))
    }

    fn word_at(&self, q: usize, word: &str) -> bool {
        word.bytes()
            .enumerate()
            .all(|(i, k)| self.at(q + i) | 32 == u32::from(k) && is_word(self.at(q + i)))
    }

    fn direction_end(&self, which: &str, line_end: usize) -> Option<usize> {
        if line_end < self.p + 9 {
            return None;
        }
        for q in (self.p..=line_end - 9).rev() {
            if !self.word_at(q, "direction") {
                continue;
            }
            let r = q + 9;
            if !is_space(self.at(r)) {
                continue;
            }
            let r = self.skip_spaces(r);
            if self.word_at(r, which) {
                return Some(self.index_of(10, r + 2).unwrap_or(self.n));
            }
        }
        None
    }

    fn direction(&mut self) -> bool {
        if self.dir_next == -1 {
            return false;
        }
        let p = self.p as isize;
        if self.dir_next < p {
            self.dir_next = (self.p..self.n)
                .find(|&q| self.word_at(q, "direction"))
                .map_or(-1, |q| q as isize);
            if self.dir_next == -1 {
                return false;
            }
        }
        if self.p < self.dir_failed {
            return false;
        }
        if self.line_end < p {
            self.line_end = self.next_line_end(self.p) as isize;
        }
        if self.dir_next >= self.line_end {
            return false;
        }
        let line_end = self.line_end as usize;
        for (i, which) in ["tb", "bt", "rl", "lr"].into_iter().enumerate() {
            if let Some(e) = self.direction_end(which, line_end) {
                let kind = [
                    T::DirectionTb,
                    T::DirectionBt,
                    T::DirectionRl,
                    T::DirectionLr,
                ][i];
                self.emit(kind, self.p, e);
                return true;
            }
        }
        self.dir_failed = line_end;
        false
    }

    fn quoted(&mut self) {
        let p = self.p;
        let Some(e) = self.index_of(34, p + 1) else {
            self.emit(T::Char, p, p + 1);
            return;
        };
        let name = e > p + 1
            && self.src[p + 1..e]
                .iter()
                .all(|&c| !matches!(c, 37 | 92 | 13 | 10 | 11 | 8));
        self.emit(if name { T::EntityName } else { T::Word }, p, e + 1);
    }

    fn number(&mut self, c: u32) {
        let p = self.p;
        let d = self.at(p + 1);
        if d == 43 && (c == 49 || c == 48) {
            self.emit(if c == 49 { T::OneOrMore } else { T::ZeroOrMore }, p, p + 2);
            return;
        }
        let q = self.digits(p + 1);
        if self.at(q) == 46 && is_digit(self.at(q + 1)) {
            let e = self.digits(q + 2);
            self.emit(T::DecimalNum, p, e);
            return;
        }
        if c == 49 {
            if is_space(d) {
                let f = self.at(self.skip_spaces(p + 2));
                if is_word(f) || f == 34 || f == 39 {
                    self.emit(T::OnlyOne, p, p + 1);
                    return;
                }
            } else if d == 45 || d == 46 {
                let f = self.at(p + 2);
                if f == 45 || f == 46 {
                    self.emit(T::OnlyOne, p, p + 1);
                    return;
                }
            }
            if !is_word(d) {
                self.emit(T::EntityOne, p, p + 1);
                return;
            }
        }
        self.emit(T::Num, p, q);
    }

    fn keyword(&mut self, lc: u32) -> bool {
        let p = self.p;
        let d = self.at(p + 1);
        let found = match lc {
            101 => {
                if self.kwb("erdiagram") {
                    Some((T::ErDiagram, p + 9))
                } else if self.kwb("end") {
                    Some((T::EndKw, self.skip_spaces(p + 3)))
                } else {
                    None
                }
            }
            115 => {
                if self.kwb("style") {
                    self.state = S::Style;
                    Some((T::Style, p + 5))
                } else if self.kwb("subgraph") {
                    Some((T::Subgraph, p + 8))
                } else {
                    None
                }
            }
            99 => {
                if self.kwb("classdef") {
                    self.state = S::Style;
                    Some((T::ClassDef, p + 8))
                } else if self.kwb("class") {
                    Some((T::Class, p + 5))
                } else {
                    None
                }
            }
            111 => {
                if self.kwb("one or zero") {
                    Some((T::ZeroOrOne, p + 11))
                } else if self.kwb("one or more") || self.kwb("one or many") {
                    Some((T::OneOrMore, p + 11))
                } else if self.kwb("one") {
                    Some((T::OnlyOne, p + 3))
                } else if self.kwb("only one") {
                    Some((T::OnlyOne, p + 8))
                } else if d == 124 {
                    Some((T::ZeroOrOne, p + 2))
                } else if d == 123 {
                    Some((T::ZeroOrMore, p + 2))
                } else if self.kwb("optionally to") {
                    Some((T::NonIdentifying, p + 13))
                } else {
                    None
                }
            }
            122 => {
                if self.kwb("zero or one") {
                    Some((T::ZeroOrOne, p + 11))
                } else if self.kwb("zero or more") || self.kwb("zero or many") {
                    Some((T::ZeroOrMore, p + 12))
                } else {
                    None
                }
            }
            109 => {
                if self.kw("many(0)") {
                    Some((T::ZeroOrMore, p + 7))
                } else if self.kw("many(1)") {
                    Some((T::OneOrMore, p + 7))
                } else if self.kwb("many") {
                    Some((T::ZeroOrMore, p + 4))
                } else {
                    None
                }
            }
            116 => self.kwb("to").then_some((T::Identifying, p + 2)),
            117 => matches!(d, 46 | 45 | 124).then_some((T::MdParent, p + 1)),
            124 => {
                if self.lower(p + 1) == 111 && !is_word(self.at(p + 2)) {
                    Some((T::ZeroOrOne, p + 2))
                } else if d == 124 {
                    Some((T::OnlyOne, p + 2))
                } else if d == 123 {
                    Some((T::OneOrMore, p + 2))
                } else {
                    None
                }
            }
            125 => {
                if self.lower(p + 1) == 111 && !is_word(self.at(p + 2)) {
                    Some((T::ZeroOrMore, p + 2))
                } else if d == 124 {
                    Some((T::OneOrMore, p + 2))
                } else {
                    None
                }
            }
            46 => matches!(d, 46 | 45).then_some((T::NonIdentifying, p + 2)),
            45 => match d {
                45 => Some((T::Identifying, p + 2)),
                46 => Some((T::NonIdentifying, p + 2)),
                _ => None,
            },
            _ => None,
        };
        let Some((kind, end)) = found else {
            return false;
        };
        self.emit(kind, p, end);
        true
    }

    fn initial(&mut self, c: u32) {
        let p = self.p;
        if (c == 97 || c == 65) && self.kw("acc") {
            if let Some(e) = self.acc("acctitle", 58) {
                self.state = S::AccTitle;
                self.emit(T::AccTitle, p, e);
                return;
            }
            if let Some(e) = self.acc("accdescr", 58) {
                self.state = S::AccDescr;
                self.emit(T::AccDescr, p, e);
                return;
            }
            if let Some(e) = self.acc("accdescr", 123) {
                self.state = S::AccDescrMultiline;
                self.p = e;
                return;
            }
        }
        if self.direction() {
            return;
        }
        match c {
            32 | 9 | 13 => {
                let mut e = p + 1;
                while matches!(self.at(e), 32 | 9 | 13) {
                    e += 1;
                }
                self.p = e;
                return;
            }
            10 => {
                let mut e = p + 1;
                while self.at(e) == 10 {
                    e += 1;
                }
                self.emit(T::Newline, p, e);
                return;
            }
            34 => {
                self.quoted();
                return;
            }
            123 => {
                self.state = S::Block;
                self.emit(T::BlockStart, p, p + 1);
                return;
            }
            35 => {
                self.emit(T::Brkt, p, p + 1);
                return;
            }
            44 => {
                self.emit(T::Comma, p, p + 1);
                return;
            }
            58 => {
                if self.at(p + 1) == 58 && self.at(p + 2) == 58 {
                    self.emit(T::StyleSeparator, p, p + 3);
                } else {
                    self.emit(T::Colon, p, p + 1);
                }
                return;
            }
            91 => {
                self.emit(T::Sqs, p, p + 1);
                return;
            }
            93 => {
                self.emit(T::Sqe, p, p + 1);
                return;
            }
            _ => {}
        }
        if is_digit(c) {
            self.number(c);
            return;
        }
        if self.keyword(if (65..=90).contains(&c) { c + 32 } else { c }) {
            return;
        }
        let text = |c: u32| c >= 128 || class(c) & TEXT != 0;
        if text(c) {
            let mut e = p + 1;
            while e < self.n && text(self.at(e)) {
                e += 1;
            }
            self.emit(T::UnicodeText, p, e);
            return;
        }
        self.emit(T::Char, p, p + 1);
    }

    fn generic(&mut self) -> bool {
        let p = self.p as isize;
        if self.tilde_next == -1 {
            return false;
        }
        if self.tilde_next < p {
            self.tilde_next = self.index_of(126, self.p).map_or(-1, |at| at as isize);
            if self.tilde_next == -1 {
                return false;
            }
        }
        if self.ws_next < p {
            let mut q = self.p;
            while q < self.n && !is_space(self.at(q)) {
                q += 1;
            }
            self.ws_next = q as isize;
        }
        if self.tilde_next >= self.ws_next {
            return false;
        }
        if self.tilde_line < p {
            let line = self.next_line_end(self.p);
            self.tilde_line = line as isize;
            self.tilde_last = self.src[..line.min(self.n)]
                .iter()
                .rposition(|&c| c == 126)
                .map_or(-1, |at| at as isize);
        }
        if self.tilde_last <= self.tilde_next {
            return false;
        }
        let mut e = self.tilde_last as usize + 1;
        while e < self.n && !is_space(self.at(e)) {
            e += 1;
        }
        self.emit(T::AttributeWord, self.p, e);
        true
    }

    fn block(&mut self, c: u32) {
        let p = self.p;
        if is_space(c) {
            self.p = self.skip_spaces(p + 1);
            return;
        }
        let lc = c | 32;
        if matches!(lc, 112 | 102 | 117) && self.at(p + 1) | 32 == 107 && !is_word(self.at(p + 2)) {
            self.emit(T::AttributeKey, p, p + 2);
            return;
        }
        if self.generic() {
            return;
        }
        if is_attr(c, ATTR_START) {
            let mut e = p + 1;
            while e < self.n && is_attr(self.at(e), ATTR_REST) {
                e += 1;
            }
            self.emit(T::AttributeWord, p, e);
            return;
        }
        if c == 96 {
            self.state = S::BlockBq;
            self.p += 1;
            return;
        }
        if c == 34 {
            if let Some(e) = self.index_of(34, p + 1) {
                self.emit(T::Comment, p, e + 1);
                return;
            }
        } else if c == 125 {
            self.state = S::Initial;
            self.emit(T::BlockStop, p, p + 1);
            return;
        }
        self.emit(T::Char, p, p + 1);
    }

    fn style(&mut self, c: u32) -> bool {
        let p = self.p;
        let style_text = |c: u32| c >= 128 || class(c) & STYLE_TEXT != 0;
        match c {
            10 => {
                let mut e = p + 1;
                while self.at(e) == 10 {
                    e += 1;
                }
                self.state = S::Initial;
                self.emit(T::Newline, p, e);
            }
            58 => self.emit(T::Colon, p, p + 1),
            44 => self.emit(T::Comma, p, p + 1),
            35 => self.emit(T::Brkt, p, p + 1),
            59 => self.emit(T::Semi, p, p + 1),
            _ if is_space(c) => self.p = self.skip_spaces(p + 1),
            _ if style_text(c) => {
                let mut e = p + 1;
                while e < self.n && style_text(self.at(e)) {
                    e += 1;
                }
                self.emit(T::StyleText, p, e);
            }
            _ => {
                self.tokens.push(Token {
                    kind: T::Error,
                    start: p,
                    end: p,
                });
                return false;
            }
        }
        true
    }

    fn run(mut self) -> Vec<Token> {
        while self.p < self.n {
            let c = self.at(self.p);
            match self.state {
                S::Initial => self.initial(c),
                S::Block => self.block(c),
                S::BlockBq => {
                    if c == 96 {
                        self.state = S::Block;
                        self.p += 1;
                    } else {
                        let e = self.index_of(96, self.p).unwrap_or(self.n);
                        self.emit(T::AttributeWord, self.p, e);
                    }
                }
                S::AccTitle | S::AccDescr => {
                    let e = self.index_of(10, self.p).unwrap_or(self.n);
                    let kind = if self.state == S::AccTitle {
                        T::AccTitleValue
                    } else {
                        T::AccDescrValue
                    };
                    self.emit(kind, self.p, e);
                    self.state = S::Initial;
                }
                S::AccDescrMultiline => {
                    if c == 125 {
                        self.state = S::Initial;
                        self.p += 1;
                    } else {
                        let e = self.index_of(125, self.p).unwrap_or(self.n);
                        self.emit(T::AccDescrMultilineValue, self.p, e);
                    }
                }
                S::Style => {
                    if !self.style(c) {
                        return self.tokens;
                    }
                }
            }
        }
        let last = match self.state {
            S::Initial => Some(T::Eof),
            S::AccTitle => Some(T::AccTitleValue),
            S::AccDescr => Some(T::AccDescrValue),
            S::AccDescrMultiline => Some(T::AccDescrMultilineValue),
            _ => None,
        };
        let n = self.n;
        if let Some(kind) = last {
            self.tokens.push(Token {
                kind,
                start: n,
                end: n,
            });
        }
        self.tokens.push(Token {
            kind: T::End,
            start: n,
            end: n,
        });
        self.tokens
    }
}

type Edit = (usize, usize, usize);

fn find_units(text: &[u16], word: &[u16], from: usize) -> Option<usize> {
    if word.is_empty() || from > text.len() {
        return None;
    }
    text[from..]
        .windows(word.len())
        .position(|window| window == word)
        .map(|at| from + at)
}

fn drop_color_semicolons(text: Vec<u16>, keyword: &str) -> (Vec<u16>, Vec<Edit>) {
    let word: Vec<u16> = keyword.encode_utf16().collect();
    let mut drops = Vec::new();
    let mut from = find_units(&text, &word, 0);
    while let Some(start) = from {
        let line_end = (start..text.len())
            .find(|&at| is_line_end(u32::from(text[at])))
            .unwrap_or(text.len());
        let mut after_colon = false;
        let mut hash = None;
        for (j, &c) in text
            .iter()
            .enumerate()
            .take(line_end)
            .skip(start + word.len())
        {
            let c = u32::from(c);
            if c == 58 {
                after_colon = true;
            } else if c == 35 {
                if after_colon {
                    hash = Some(j);
                    break;
                }
            } else if is_space(c) {
                after_colon = false;
            }
        }
        if let Some(hash) = hash
            && let Some(semi) = (hash + 1..line_end).rev().find(|&at| text[at] == 59)
        {
            drops.push(semi);
        }
        from = if line_end >= text.len() {
            None
        } else {
            find_units(&text, &word, line_end + 1)
        };
    }
    if drops.is_empty() {
        return (text, Vec::new());
    }
    let mut out = Vec::with_capacity(text.len());
    let mut last = 0;
    for &semi in &drops {
        out.extend_from_slice(&text[last..semi]);
        last = semi + 1;
    }
    out.extend_from_slice(&text[last..]);
    (out, drops.into_iter().map(|at| (at, at + 1, 0)).collect())
}

fn encode_entities(text: Vec<u16>) -> (Vec<u16>, Vec<Vec<Edit>>) {
    if !text.contains(&35) {
        return (text, Vec::new());
    }
    let (text, styles) = drop_color_semicolons(text, "style");
    let (text, defs) = drop_color_semicolons(text, "classDef");
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
                let numeric = inner.iter().all(|&c| is_digit(u32::from(c)));
                let mut code: Vec<u16> =
                    if numeric { "ﬂ°°" } else { "ﬂ°" }.encode_utf16().collect();
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
    (out, vec![styles, defs, edits])
}

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

fn input_offset(steps: &[Vec<Edit>], at: usize) -> usize {
    steps
        .iter()
        .rev()
        .fold(at, |at, edits| step_input(edits, at))
}

#[derive(Clone, Debug, Default)]
struct Attribute {
    kind: String,
    name: String,
    keys: Vec<String>,
    comment: String,
}

#[derive(Clone, Debug)]
struct Entity {
    id: String,
    label: String,
    alias: String,
    attributes: Vec<Attribute>,
    css_classes: String,
    css_styles: Vec<String>,
}

#[derive(Clone, Debug)]
struct Relationship {
    entity_a: String,
    role_a: String,
    entity_b: String,
    card_a: Cardinality,
    identifying: bool,
    card_b: Cardinality,
}

#[derive(Clone, Debug, Default)]
struct ClassDef {
    styles: Vec<String>,
    text_styles: Vec<String>,
}

#[derive(Clone, Debug)]
struct Subgraph {
    id: String,
    nodes: Vec<String>,
    title: String,
    classes: Vec<String>,
    css_styles: Vec<String>,
    dir: Option<&'static str>,
}

enum Item {
    Name(String),
    Dir(&'static str),
}

#[derive(Default)]
struct Frame {
    id: String,
    title: String,
    list: Vec<Item>,
}

struct Model {
    direction: &'static str,
    entities: IndexMap<String, Entity>,
    relationships: Vec<Relationship>,
    classes: HashMap<String, ClassDef>,
    subgraphs: Vec<Subgraph>,
    lookup: HashMap<String, usize>,
    claimed: HashSet<String>,
    acc_title: Option<String>,
    acc_descr: Option<String>,
    attributes: usize,
}

impl Model {
    fn new() -> Self {
        Self {
            direction: "TB",
            entities: IndexMap::new(),
            relationships: Vec::new(),
            classes: HashMap::new(),
            subgraphs: Vec::new(),
            lookup: HashMap::new(),
            claimed: HashSet::new(),
            acc_title: None,
            acc_descr: None,
            attributes: 0,
        }
    }

    fn add_entity(&mut self, name: &str, alias: &str) -> &mut Entity {
        let size = self.entities.len();
        let entity = self
            .entities
            .entry(name.to_owned())
            .or_insert_with(|| Entity {
                id: format!("entity-{name}-{size}"),
                label: name.to_owned(),
                alias: alias.to_owned(),
                attributes: Vec::new(),
                css_classes: "default".to_owned(),
                css_styles: Vec::new(),
            });
        if entity.alias.is_empty() && !alias.is_empty() {
            alias.clone_into(&mut entity.alias);
        }
        entity
    }

    fn add_attributes(&mut self, name: &str, attributes: Vec<Attribute>) {
        self.attributes += attributes.len();
        self.add_entity(name, "").attributes.extend(attributes);
    }

    fn add_relationship(
        &mut self,
        a: &str,
        role: String,
        b: &str,
        spec: (Cardinality, bool, Cardinality),
    ) {
        let entity_a = if self.lookup.contains_key(a) {
            a.to_owned()
        } else {
            self.add_entity(a, "").id.clone()
        };
        let entity_b = if self.lookup.contains_key(b) {
            b.to_owned()
        } else {
            self.add_entity(b, "").id.clone()
        };
        self.relationships.push(Relationship {
            entity_a,
            role_a: role,
            entity_b,
            card_b: spec.0,
            identifying: spec.1,
            card_a: spec.2,
        });
    }

    fn add_css_styles(&mut self, ids: &[String], styles: &[String]) {
        for id in ids {
            if let Some(entity) = self.entities.get_mut(id) {
                entity.css_styles.extend(styles.iter().cloned());
            }
            if let Some(&at) = self.lookup.get(id) {
                self.subgraphs[at].css_styles.extend(styles.iter().cloned());
            }
        }
    }

    fn add_class(&mut self, ids: &[String], styles: &[String]) {
        for id in ids {
            let def = self.classes.entry(id.clone()).or_default();
            for style in styles {
                if style.contains("color") {
                    def.text_styles.push(style.replacen("fill", "bgFill", 1));
                }
                def.styles.push(style.clone());
            }
        }
    }

    fn set_class(&mut self, ids: &[String], names: &[String]) {
        for id in ids {
            if let Some(entity) = self.entities.get_mut(id) {
                for name in names {
                    entity.css_classes.push(' ');
                    entity.css_classes.push_str(name);
                }
            }
            if let Some(&at) = self.lookup.get(id) {
                self.subgraphs[at].classes.extend(names.iter().cloned());
            }
        }
    }

    fn add_subgraph(&mut self, id: &str, list: Vec<Item>, title: &str) -> String {
        let id = js_trim(id).to_owned();
        let mut seen = HashSet::new();
        let mut nodes = Vec::new();
        let mut dir = None;
        for item in list {
            match item {
                Item::Dir(value) => dir = Some(value),
                Item::Name(name) => {
                    let trimmed = js_trim(&name);
                    if trimmed.is_empty() || !seen.insert(trimmed.to_owned()) {
                        continue;
                    }
                    if !self.claimed.contains(&name) {
                        nodes.push(name);
                    }
                }
            }
        }
        self.claimed.extend(nodes.iter().cloned());
        self.subgraphs.push(Subgraph {
            id: id.clone(),
            nodes,
            title: js_trim(title).to_owned(),
            classes: Vec::new(),
            css_styles: Vec::new(),
            dir,
        });
        self.lookup.insert(id.clone(), self.subgraphs.len() - 1);
        id
    }

    fn set_acc_description(&mut self, text: &str) {
        let mut out = String::with_capacity(text.len());
        let mut chars = text.chars().peekable();
        while let Some(ch) = chars.next() {
            out.push(ch);
            if ch == '\n' {
                while chars.next_if(|next| is_space(*next as u32)).is_some() {}
            }
        }
        self.acc_descr = Some(out);
    }
}

enum Failure {
    Parse(usize, String),
    Lexical(usize),
    Limit(String),
}

struct Parser<'a> {
    src: &'a [u16],
    tokens: Vec<Token>,
    i: usize,
    db: Model,
    max_edges: usize,
}

impl Parser<'_> {
    fn ty(&self) -> T {
        self.tokens.get(self.i).map_or(T::End, |token| token.kind)
    }

    fn text(&self) -> String {
        self.tokens.get(self.i).map_or_else(String::new, |token| {
            String::from_utf16_lossy(&self.src[token.start..token.end])
        })
    }

    fn take(&mut self) -> String {
        let text = self.text();
        self.i += 1;
        text
    }

    fn start(&self) -> usize {
        self.tokens
            .get(self.i)
            .or(self.tokens.last())
            .map_or(0, |token| token.start)
    }

    fn fail(&self, expected: &str) -> Failure {
        let t = self.ty();
        if t == T::Error {
            return Failure::Lexical(self.start());
        }
        let got = if t == T::Char {
            self.text()
        } else {
            t.name().to_owned()
        };
        Failure::Parse(self.start(), format!("Expecting {expected}, got '{got}'"))
    }

    fn expect(&mut self, kind: T) -> Result<String, Failure> {
        if self.ty() != kind {
            return Err(self.fail(&format!("'{}'", kind.name())));
        }
        Ok(self.take())
    }

    fn separator(&mut self) -> Result<(), Failure> {
        if !matches!(self.ty(), T::Newline | T::Semi | T::Eof) {
            return Err(self.fail("'EOF', 'NEWLINE', 'SEMI'"));
        }
        self.i += 1;
        Ok(())
    }

    fn entity_name(&mut self) -> Result<String, Failure> {
        let t = self.ty();
        if !t.is_name() {
            return Err(self.fail(NAME));
        }
        let text = self.take();
        Ok(if t == T::EntityName {
            unquote(&text)
        } else {
            text
        })
    }

    fn id_list(&mut self) -> Result<Vec<String>, Failure> {
        let mut ids = Vec::new();
        loop {
            if !matches!(self.ty(), T::UnicodeText | T::StyleText) {
                return Err(self.fail("'UNICODE_TEXT', 'STYLE_TEXT'"));
            }
            ids.push(self.take());
            if self.ty() != T::Comma {
                return Ok(ids);
            }
            self.i += 1;
        }
    }

    fn styles(&mut self) -> Result<Vec<String>, Failure> {
        let component = |t: T| matches!(t, T::StyleText | T::Num | T::Colon | T::Brkt);
        let mut styles = Vec::new();
        loop {
            if !component(self.ty()) {
                return Err(self.fail("'STYLE_TEXT', 'COLON', 'NUM', 'BRKT'"));
            }
            let mut style = self.take();
            while component(self.ty()) {
                style.push_str(&self.take());
            }
            styles.push(style);
            if self.ty() != T::Comma {
                return Ok(styles);
            }
            self.i += 1;
        }
    }

    fn rel_spec(&mut self) -> Result<(Cardinality, bool, Cardinality), Failure> {
        let card_b = self.ty().cardinality().unwrap_or(Cardinality::OnlyOne);
        self.i += 1;
        let identifying = match self.ty() {
            T::Identifying => true,
            T::NonIdentifying => false,
            _ => return Err(self.fail("'NON_IDENTIFYING', 'IDENTIFYING'")),
        };
        self.i += 1;
        let Some(card_a) = self.ty().cardinality() else {
            return Err(self.fail(CARDS));
        };
        self.i += 1;
        Ok((card_b, identifying, card_a))
    }

    fn role(&mut self) -> Result<String, Failure> {
        let t = self.ty();
        if !matches!(t, T::Word | T::EntityName | T::UnicodeText) {
            return Err(self.fail("'UNICODE_TEXT', 'ENTITY_NAME', 'WORD'"));
        }
        let text = self.take();
        Ok(if t == T::UnicodeText {
            text
        } else {
            unquote(&text)
        })
    }

    fn is_char(&self, ch: &str) -> bool {
        self.ty() == T::Char && self.text() == ch
    }

    fn attributes(&mut self) -> Result<Vec<Attribute>, Failure> {
        let mut list = Vec::new();
        loop {
            let mut kind = self.expect(T::AttributeWord)?;
            if self.is_char("?") {
                kind.push_str(&self.take());
            }
            if self.ty() != T::AttributeWord {
                return Err(self.fail(if kind.ends_with('?') {
                    "'ATTRIBUTE_WORD'"
                } else {
                    "'ATTRIBUTE_WORD', '?'"
                }));
            }
            let mut attribute = Attribute {
                kind,
                name: self.take(),
                ..Attribute::default()
            };
            if self.ty() == T::AttributeKey {
                attribute.keys.push(self.take());
                while self.is_char(",") {
                    self.i += 1;
                    let key = self.expect(T::AttributeKey)?;
                    attribute.keys.push(key);
                }
            }
            if self.ty() == T::Comment {
                attribute.comment = unquote(&self.take());
            }
            list.push(attribute);
            if self.ty() == T::BlockStop {
                return Ok(list);
            }
        }
    }

    fn block(&mut self) -> Result<Option<Vec<Attribute>>, Failure> {
        if self.ty() != T::BlockStart {
            return Ok(None);
        }
        self.i += 1;
        if self.ty() == T::BlockStop {
            self.i += 1;
            return Ok(None);
        }
        let list = self.attributes()?;
        self.i += 1;
        Ok(Some(list))
    }

    fn entity_statement(&mut self) -> Result<Vec<String>, Failure> {
        let a = self.entity_name()?;
        let mut t = self.ty();
        if t == T::Sqs {
            self.i += 1;
            let alias = self.entity_name()?;
            self.expect(T::Sqe)?;
            let mut classes = None;
            if self.ty() == T::StyleSeparator {
                self.i += 1;
                classes = Some(self.id_list()?);
            }
            let attrs = self.block()?;
            self.db.add_entity(&a, &alias);
            if let Some(attrs) = attrs {
                self.db.add_attributes(&a, attrs);
            }
            if let Some(classes) = classes {
                self.db.set_class(std::slice::from_ref(&a), &classes);
            }
            return Ok(vec![a]);
        }
        let mut classes = None;
        if t == T::StyleSeparator {
            self.i += 1;
            classes = Some(self.id_list()?);
            t = self.ty();
        }
        if t.cardinality().is_some() {
            let spec = self.rel_spec()?;
            let b = self.entity_name()?;
            let mut classes_b = None;
            if self.ty() == T::StyleSeparator {
                self.i += 1;
                classes_b = Some(self.id_list()?);
            }
            self.expect(T::Colon)?;
            let label = self.role()?;
            self.db.add_entity(&a, "");
            self.db.add_entity(&b, "");
            self.db.add_relationship(&a, label, &b, spec);
            if let Some(classes) = classes {
                self.db.set_class(std::slice::from_ref(&a), &classes);
            }
            if let Some(classes) = classes_b {
                self.db.set_class(std::slice::from_ref(&b), &classes);
            }
            return Ok(vec![a, b]);
        }
        let attrs = self.block()?;
        self.db.add_entity(&a, "");
        if let Some(attrs) = attrs {
            self.db.add_attributes(&a, attrs);
        }
        if let Some(classes) = classes {
            self.db.set_class(std::slice::from_ref(&a), &classes);
        }
        Ok(vec![a])
    }

    fn check_limits(&self) -> Result<(), Failure> {
        let db = &self.db;
        if db.relationships.len() > self.max_edges {
            return Err(Failure::Limit(format!(
                "Edge limit exceeded. The limit is {}.",
                self.max_edges
            )));
        }
        if db.entities.len() + db.subgraphs.len() > MAX_ENTITIES {
            return Err(Failure::Limit(format!(
                "Entity limit exceeded. The limit is {MAX_ENTITIES}."
            )));
        }
        if db.attributes > MAX_ATTRIBUTES {
            return Err(Failure::Limit(format!(
                "Attribute limit exceeded. The limit is {MAX_ATTRIBUTES}."
            )));
        }
        Ok(())
    }

    fn statements(&mut self) -> Result<(), Failure> {
        self.expect(T::ErDiagram)?;
        let mut frames = vec![Frame::default()];
        loop {
            let nested = frames.len() > 1;
            let t = self.ty();
            let push = |frames: &mut Vec<Frame>, item: Item| {
                if nested && let Some(frame) = frames.last_mut() {
                    frame.list.push(item);
                }
            };
            match t {
                T::Newline => self.i += 1,
                T::Eof => {
                    self.i += 1;
                    if nested {
                        return Err(self.fail(&format!("'END', {LINE}")));
                    }
                    return Ok(());
                }
                T::EndKw => {
                    if !nested {
                        return Err(self.fail(LINE));
                    }
                    self.i += 1;
                    let frame = frames.pop().unwrap_or_default();
                    let id = self.db.add_subgraph(&frame.id, frame.list, &frame.title);
                    if frames.len() > 1
                        && let Some(outer) = frames.last_mut()
                    {
                        outer.list.push(Item::Name(id));
                    }
                }
                T::Subgraph => {
                    self.i += 1;
                    let id = self.entity_name()?;
                    let mut title = id.clone();
                    if self.ty() == T::Sqs {
                        self.i += 1;
                        title = self.entity_name()?;
                        while self.ty().is_name() {
                            title.push(' ');
                            title.push_str(&self.entity_name()?);
                        }
                        self.expect(T::Sqe)?;
                    }
                    self.separator()?;
                    if frames.len() > MAX_DEPTH {
                        return Err(Failure::Limit(
                            "Entity relationship subgraphs are nested too deeply".to_owned(),
                        ));
                    }
                    frames.push(Frame {
                        id,
                        title,
                        list: Vec::new(),
                    });
                }
                T::AccTitle => {
                    self.i += 1;
                    let value = js_trim(&self.expect(T::AccTitleValue)?).to_owned();
                    self.db.acc_title = Some(value.clone());
                    push(&mut frames, Item::Name(value));
                }
                T::AccDescr => {
                    self.i += 1;
                    let value = js_trim(&self.expect(T::AccDescrValue)?).to_owned();
                    self.db.set_acc_description(&value);
                    push(&mut frames, Item::Name(value));
                }
                T::AccDescrMultilineValue => {
                    let value = js_trim(&self.take()).to_owned();
                    self.db.set_acc_description(&value);
                    push(&mut frames, Item::Name(value));
                }
                T::DirectionTb | T::DirectionBt | T::DirectionRl | T::DirectionLr => {
                    self.i += 1;
                    let value = DIRECTIONS[match t {
                        T::DirectionTb => 0,
                        T::DirectionBt => 1,
                        T::DirectionRl => 2,
                        _ => 3,
                    }];
                    if nested {
                        push(&mut frames, Item::Dir(value));
                    } else {
                        self.db.direction = value;
                    }
                }
                T::ClassDef => {
                    let keyword = self.take();
                    let ids = self.id_list()?;
                    let styles = self.styles()?;
                    self.separator()?;
                    self.db.add_class(&ids, &styles);
                    push(&mut frames, Item::Name(keyword));
                }
                T::Class => {
                    let keyword = self.take();
                    let ids = self.id_list()?;
                    let names = self.id_list()?;
                    self.db.set_class(&ids, &names);
                    push(&mut frames, Item::Name(keyword));
                }
                T::Style => {
                    let keyword = self.take();
                    let ids = self.id_list()?;
                    let styles = self.styles()?;
                    self.separator()?;
                    self.db.add_css_styles(&ids, &styles);
                    push(&mut frames, Item::Name(keyword));
                }
                _ => {
                    if !t.is_name() {
                        return Err(self.fail(&if nested {
                            format!("'END', {LINE}")
                        } else {
                            LINE.to_owned()
                        }));
                    }
                    for name in self.entity_statement()? {
                        push(&mut frames, Item::Name(name));
                    }
                }
            }
            self.check_limits()?;
        }
    }
}

fn unquote(text: &str) -> String {
    let mut chars = text.chars();
    chars.next();
    chars.next_back();
    chars.as_str().to_owned()
}

fn source_offset(text: &[u16], steps: &[Vec<Edit>], offset: usize) -> usize {
    let end = text
        .iter()
        .rposition(|&c| !is_space(u32::from(c)))
        .map_or(0, |at| at + 1);
    if end > 0 && offset >= end {
        input_offset(steps, end - 1) + 1
    } else {
        input_offset(steps, offset)
    }
}

fn parse(source: &str, max_edges: usize) -> Result<Model, PeleError> {
    let original: Vec<u16> = source.encode_utf16().collect();
    let (mut encoded, steps) = encode_entities(original.clone());
    encoded.push(10);
    let tokens = Lexer::new(&encoded).run();
    let mut parser = Parser {
        src: &encoded,
        tokens,
        i: 0,
        db: Model::new(),
        max_edges,
    };
    let (at, message) = match parser.statements() {
        Ok(()) => return Ok(parser.db),
        Err(Failure::Limit(message)) => return Err(PeleError::limit(message)),
        Err(Failure::Parse(at, detail)) => (at, detail),
        Err(Failure::Lexical(at)) => (at, "Lexical error. Unrecognized text.".to_owned()),
    };
    let (line, column) = position(&original, source_offset(&encoded, &steps, at));
    Err(PeleError::syntax(message, line, column))
}

fn position(text: &[u16], offset: usize) -> (usize, usize) {
    let before = &text[..offset.min(text.len())];
    let line = before.iter().filter(|&&c| c == 10).count() + 1;
    let start = before.iter().rposition(|&c| c == 10).map_or(0, |at| at + 1);
    (line, offset - start + 1)
}

struct GraphNode<'a> {
    id: &'a str,
    label: &'a str,
    alias: &'a str,
    attributes: &'a [Attribute],
    is_group: bool,
    parent: Option<&'a str>,
    css_styles: &'a [String],
    compiled: Vec<String>,
    css_classes: String,
    dir: Option<&'static str>,
}

struct GraphEdge<'a> {
    id: String,
    start: &'a str,
    end: &'a str,
    label: &'a str,
    start_kind: Cardinality,
    end_kind: Cardinality,
    dashed: bool,
}

struct Graph<'a> {
    nodes: Vec<GraphNode<'a>>,
    edges: Vec<GraphEdge<'a>>,
}

fn compiled<'b>(model: &Model, names: impl Iterator<Item = &'b str>) -> Vec<String> {
    let mut out = Vec::new();
    for name in names {
        let Some(def) = model.classes.get(name) else {
            continue;
        };
        out.extend(def.styles.iter().map(|style| js_trim(style).to_owned()));
        out.extend(
            def.text_styles
                .iter()
                .map(|style| js_trim(style).to_owned()),
        );
    }
    out
}

fn build_graph(model: &Model) -> Graph<'_> {
    let mut parent_of = HashMap::<&str, &str>::new();
    let mut subgraph_ids = HashSet::<&str>::new();
    for subgraph in model.subgraphs.iter().rev() {
        subgraph_ids.insert(&subgraph.id);
        for id in &subgraph.nodes {
            parent_of.insert(id, &subgraph.id);
        }
    }
    let mut nodes = Vec::new();
    for subgraph in model.subgraphs.iter().rev() {
        nodes.push(GraphNode {
            id: &subgraph.id,
            label: &subgraph.title,
            alias: "",
            attributes: &[],
            is_group: true,
            parent: parent_of.get(subgraph.id.as_str()).copied(),
            css_styles: &subgraph.css_styles,
            compiled: compiled(model, subgraph.classes.iter().map(String::as_str)),
            css_classes: subgraph.classes.join(" "),
            dir: subgraph.dir,
        });
    }
    let mut moved = HashMap::<&str, &str>::new();
    for (name, entity) in &model.entities {
        if subgraph_ids.contains(name.as_str()) {
            moved.insert(&entity.id, name);
            continue;
        }
        nodes.push(GraphNode {
            id: &entity.id,
            label: &entity.label,
            alias: &entity.alias,
            attributes: &entity.attributes,
            is_group: false,
            parent: parent_of.get(name.as_str()).copied(),
            css_styles: &entity.css_styles,
            compiled: compiled(model, entity.css_classes.split(' ')),
            css_classes: entity.css_classes.clone(),
            dir: None,
        });
    }
    let edges = model
        .relationships
        .iter()
        .enumerate()
        .map(|(index, relationship)| GraphEdge {
            id: format!(
                "id_{}_{}_{index}",
                relationship.entity_a, relationship.entity_b
            ),
            start: moved
                .get(relationship.entity_a.as_str())
                .copied()
                .unwrap_or(&relationship.entity_a),
            end: moved
                .get(relationship.entity_b.as_str())
                .copied()
                .unwrap_or(&relationship.entity_b),
            label: &relationship.role_a,
            start_kind: relationship.card_b,
            end_kind: relationship.card_a,
            dashed: !relationship.identifying,
        })
        .collect();
    Graph { nodes, edges }
}

fn er_marker(kind: Cardinality, x: f64, y: f64, dx: f64, dy: f64, background: &str) -> String {
    let at = |back: f64, side: f64| {
        format!(
            "{},{}",
            num(x - dx * back - dy * side),
            num(y - dy * back + dx * side)
        )
    };
    let bar = |back: f64| format!("M{}L{}", at(back, -BAR), at(back, BAR));
    let foot = format!(
        "M{}L{}L{}",
        at(0.0, -FOOT),
        at(FOOT_LENGTH, 0.0),
        at(0.0, FOOT)
    );
    let ring = |back: f64| {
        format!(
            "<circle class=\"pele-marker\" cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{background}\"/>",
            num(x - dx * back),
            num(y - dy * back),
            num(RING)
        )
    };
    let path = |d: String| format!("<path class=\"pele-marker\" d=\"{d}\"/>");
    match kind {
        Cardinality::OnlyOne => path(bar(6.0) + &bar(10.0)),
        Cardinality::ZeroOrOne => path(bar(6.0)) + &ring(13.5),
        Cardinality::OneOrMore => path(foot + &bar(FOOT_LENGTH + 3.5)),
        Cardinality::ZeroOrMore => path(foot) + &ring(FOOT_LENGTH + RING + 0.5),
        Cardinality::MdParent => format!(
            "<path class=\"pele-marker\" d=\"M{}L{}L{}L{}Z\" fill=\"{background}\"/>",
            at(0.0, 0.0),
            at(6.0, 4.0),
            at(12.0, 0.0),
            at(6.0, -4.0)
        ),
    }
}

struct Row {
    cells: [Label; 4],
    h: f64,
}

struct View {
    style: ResolvedStyle,
    label: Label,
    w: f64,
    h: f64,
    head_h: f64,
    rows: Vec<Row>,
    cols: Vec<f64>,
    row_size: f64,
    key_size: f64,
    below: bool,
}

struct Loop {
    width: f64,
    height: f64,
    band: f64,
    at: f64,
}

fn span_style(style: u8, size: f64, options: &RenderOptions) -> String {
    let mut out = String::new();
    if style & BOLD != 0 {
        out.push_str(" font-weight=\"bold\"");
    }
    if style & ITALIC != 0 {
        out.push_str(" font-style=\"italic\"");
    }
    if style & MONO != 0 {
        let _ = write!(out, " font-family=\"{}\"", esc(&options.font_family_mono));
    }
    if style & UNDERLINE != 0 {
        out.push_str(" text-decoration=\"underline\"");
    }
    if style & (SUB | SUP) != 0 {
        let _ = write!(
            out,
            " baseline-shift=\"{}\" font-size=\"{}\"",
            if style & SUB != 0 { "sub" } else { "super" },
            num(size / 1.2)
        );
    }
    out
}

fn text_style(attrs: &str, shared: u8, options: &RenderOptions) -> String {
    let mut out = attrs.to_owned();
    if shared & BOLD != 0 && !attrs.contains(" font-weight=\"") {
        out.push_str(" font-weight=\"bold\"");
    }
    if shared & ITALIC != 0 && !attrs.contains(" font-style=\"") {
        out.push_str(" font-style=\"italic\"");
    }
    if shared & MONO != 0 && !attrs.contains(" font-family=\"") {
        let _ = write!(out, " font-family=\"{}\"", esc(&options.font_family_mono));
    }
    out
}

fn text_left(label: &Label, x: f64, y: f64, attrs: &str, options: &RenderOptions) -> String {
    if label.lines.is_empty() {
        return String::new();
    }
    if let [line] = label.lines.as_slice()
        && let [only] = line.as_slice()
        && only.style == 0
    {
        return format!(
            "<text{attrs} x=\"{}\" y=\"{}\">{}</text>",
            num(x),
            num(y),
            esc(&only.text)
        );
    }
    let shared = shared_style(label);
    let shift = label.size * 0.35;
    let top = y - shift - label.line_height / 2.0;
    let mut body = String::new();
    for (i, line) in label.lines.iter().enumerate() {
        let y = top + (i as f64 + 0.5) * label.line_height + shift;
        let mut sx = x;
        let mut placed = false;
        for span in line {
            if !span.text.is_empty() {
                let pos = if placed {
                    String::new()
                } else {
                    format!(" x=\"{}\" y=\"{}\"", num(sx), num(y))
                };
                let _ = write!(
                    body,
                    "<tspan{pos}{}>{}</tspan>",
                    span_style(span.style & !shared, label.size, options),
                    esc(&span.text)
                );
                placed = true;
            }
            sx += span.width;
        }
    }
    format!(
        "<text{} xml:space=\"preserve\">{body}</text>",
        text_style(attrs, shared, options)
    )
}

fn pair_tildes(text: &str) -> String {
    let count = text.matches('~').count();
    if count <= 1 {
        return text.to_owned();
    }
    let lead = !count.is_multiple_of(2) && text.starts_with('~');
    let mut chars: Vec<char> = text.chars().collect();
    let at: Vec<usize> = chars
        .iter()
        .enumerate()
        .skip(usize::from(lead))
        .filter(|(_, ch)| **ch == '~')
        .map(|(i, _)| i)
        .collect();
    let (mut lo, mut hi) = (0, at.len().saturating_sub(1));
    while lo < hi {
        chars[at[lo]] = '<';
        chars[at[hi]] = '>';
        lo += 1;
        hi -= 1;
    }
    chars.into_iter().collect()
}

fn parse_generic_types(input: &str) -> String {
    if !input.contains('~') {
        return input.to_owned();
    }
    let mut sets = Vec::new();
    let mut last = 0;
    for (at, _) in input.match_indices(',') {
        sets.push(&input[last..at]);
        sets.push(",");
        last = at + 1;
    }
    sets.push(&input[last..]);
    let tildes = |text: &str| text.matches('~').count();
    let mut output = Vec::new();
    let mut i = 0;
    while i < sets.len() {
        let mut set = sets[i].to_owned();
        if set == ","
            && i > 0
            && i + 1 < sets.len()
            && tildes(sets[i - 1]) == 1
            && tildes(sets[i + 1]) == 1
        {
            set = format!("{},{}", sets[i - 1], sets[i + 1]);
            i += 1;
            output.pop();
        }
        output.push(pair_tildes(&set));
        i += 1;
    }
    output.concat()
}

fn cell_text(raw: &str) -> String {
    let converted = parse_generic_types(raw);
    if converted == raw {
        converted
    } else {
        converted.replace('<', "&lt;").replace('>', "&gt;")
    }
}

static RE_HTML_ENTITIES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"&#(\w+);?").expect("valid entity pattern"));
static RE_HTML_CTRL_ENTITY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)&(?:newline|tab);").expect("valid entity pattern"));
static RE_WS_ESCAPE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:\\|%5[cC])(?:%(?:6[eE]|72|74)|[nrt])").expect("valid escape pattern")
});
static RE_SCHEME_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^([a-z][a-z0-9+.-]*):").expect("valid scheme pattern"));

fn is_ctrl(ch: char) -> bool {
    matches!(ch, '\0'..='\u{1f}' | '\u{7f}'..='\u{9f}' | '\u{2000}'..='\u{200d}' | '\u{feff}' | '\u{fffe}' | '\u{ffff}')
}

fn decode_uri(uri: &str) -> String {
    let bytes = uri.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = |at: usize| bytes.get(at).and_then(|b| (*b as char).to_digit(16));
            let (Some(high), Some(low)) = (hex(i + 1), hex(i + 2)) else {
                return uri.to_owned();
            };
            out.push((high * 16 + low) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| uri.to_owned())
}

fn char_code(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    let radix = |digits: &str, base: u32| {
        (!digits.is_empty())
            .then(|| {
                digits.chars().try_fold(0.0_f64, |value, ch| {
                    ch.to_digit(base)
                        .map(|d| value * f64::from(base) + f64::from(d))
                })
            })
            .flatten()
    };
    let value = if let Some(digits) = lower.strip_prefix("0x") {
        radix(digits, 16)
    } else if let Some(digits) = lower.strip_prefix("0b") {
        radix(digits, 2)
    } else if let Some(digits) = lower.strip_prefix("0o") {
        radix(digits, 8)
    } else if raw == "Infinity" {
        Some(f64::INFINITY)
    } else {
        let (mantissa, exponent) = lower.split_once('e').unwrap_or((&lower, "0"));
        let digits = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
        (digits(mantissa) && digits(exponent))
            .then(|| raw.parse::<f64>().ok())
            .flatten()
    };
    let unit = value
        .filter(|value| value.is_finite())
        .map_or(0, |value| value.trunc().rem_euclid(65536.0) as u32);
    char::from_u32(unit).map_or_else(String::new, String::from)
}

fn unwrap_url(url: &str, percent: bool) -> Option<String> {
    let mut out = if percent {
        decode_uri(js_trim(url))
    } else {
        js_trim(url).to_owned()
    };
    for _ in 0..32 {
        let step: String = out.chars().filter(|ch| !is_ctrl(*ch)).collect();
        let step =
            RE_HTML_ENTITIES.replace_all(&step, |found: &regex::Captures<'_>| char_code(&found[1]));
        let step = RE_HTML_CTRL_ENTITY.replace_all(&step, "");
        let step: String = step.chars().filter(|ch| !is_ctrl(*ch)).collect();
        let step = RE_WS_ESCAPE.replace_all(&step, "");
        out = js_trim(&step).to_owned();
        if percent {
            out = decode_uri(&out);
        }
        if !out.chars().any(is_ctrl)
            && !RE_HTML_ENTITIES.is_match(&out)
            && !RE_HTML_CTRL_ENTITY.is_match(&out)
            && !RE_WS_ESCAPE.is_match(&out)
        {
            return Some(out);
        }
    }
    None
}

fn scheme_prefix(text: &str) -> Option<&str> {
    let mut start = 0;
    for line in text.split(['\n', '\r', '\u{2028}', '\u{2029}']) {
        let lower = line.to_ascii_lowercase();
        let end = line
            .char_indices()
            .skip(1)
            .filter(|(at, ch)| *ch == ':' || (*ch == '&' && lower[*at..].starts_with("&colon;")))
            .last()
            .map(|(at, ch)| at + if ch == ':' { 1 } else { 7 });
        if let Some(end) = end {
            return Some(&text[start..start + end]);
        }
        start += line.len() + 1;
    }
    None
}

fn http_host(url: &str) -> bool {
    let rest = url.split_once(':').map_or("", |(_, rest)| rest);
    let rest = rest.trim_start_matches(['/', '\\']);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or("");
    let host = match host.rsplit_once(':') {
        Some((host, port)) if port.bytes().all(|b| b.is_ascii_digit()) => {
            if port.parse::<u32>().is_ok_and(|port| port > 65535) {
                return false;
            }
            host
        }
        Some(_) => return false,
        None => host,
    };
    !host.is_empty()
        && !host
            .chars()
            .any(|ch| ch <= ' ' || "#%<>?@[\\]^|".contains(ch))
}

fn sanitize_url(url: &str) -> Option<String> {
    if url.is_empty() {
        return None;
    }
    let decoded = unwrap_url(url, true).filter(|text| !text.is_empty())?;
    let kept = unwrap_url(url, false).filter(|text| !text.is_empty())?;
    if decoded.starts_with(['.', '/']) {
        return Some(kept);
    }
    let Some(scheme) = scheme_prefix(decoded.trim_start_matches(|ch: char| is_space(ch as u32)))
    else {
        return Some(kept);
    };
    let protocol = js_trim(&scheme.to_lowercase()).to_owned();
    let bare = protocol.trim_start_matches(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'));
    if ["javascript", "data", "vbscript"]
        .iter()
        .any(|bad| bare.starts_with(bad))
    {
        return None;
    }
    let normalized = kept
        .trim_start_matches(|ch: char| is_space(ch as u32))
        .replace('\\', "/");
    if protocol == "mailto:" || protocol.contains("://") {
        return Some(normalized);
    }
    if (protocol == "http:" || protocol == "https:") && !http_host(&normalized) {
        return None;
    }
    Some(normalized)
}

fn link_allowed(url: &str) -> bool {
    let allowed = |url: &str| {
        !(url.starts_with("//")
            || url.starts_with("\\\\")
            || url.starts_with("/\\")
            || url.starts_with("\\/"))
            && RE_SCHEME_NAME.captures(url).is_none_or(|scheme| {
                matches!(
                    scheme[1].to_ascii_lowercase().as_str(),
                    "http" | "https" | "mailto" | "tel"
                )
            })
    };
    unwrap_url(url, true).is_some_and(|decoded| allowed(url) && allowed(&decoded))
}

fn internal_link(name: &str) -> bool {
    let Some(url) = sanitize_url(name) else {
        return false;
    };
    if !link_allowed(&url) {
        return false;
    }
    let seen: String = name
        .chars()
        .filter(|ch| {
            !matches!(ch, '\0'..='\u{8}' | '\u{b}' | '\u{c}' | '\u{e}'..='\u{1f}' | '\u{7f}' | '\u{fffe}' | '\u{ffff}' | '\t' | '\n' | '\r')
        })
        .collect();
    link_allowed(seen.trim_start_matches(|ch: char| ch <= ' '))
}

fn js_sign(value: f64) -> f64 {
    if value > 0.0 {
        1.0
    } else if value < 0.0 {
        -1.0
    } else {
        value
    }
}

fn straighten(route: &[f64]) -> Vec<f64> {
    let mut out: Vec<f64> = route.iter().take(3).copied().collect();
    let mut k = 3;
    while k + 2 < route.len() {
        let prev = out.len() - 3;
        let flat = |axis: usize| {
            (out[prev + axis] - route[k + axis]).abs() < 0.01
                && (route[k + axis] - route[k + 3 + axis]).abs() < 0.01
        };
        let skip = k + 3 < route.len() && (flat(0) || flat(1));
        if !skip {
            out.extend_from_slice(&route[k..k + 3]);
        }
        k += 3;
    }
    out
}

fn spread_ends(cedges: &mut [CEdge], cnodes: &[CNode], views: &[View]) {
    let mut sides = BTreeMap::<usize, Vec<(usize, usize, f64)>>::new();
    let mut add = |node: usize, edge: usize, route: &[f64], at: usize, next: usize| {
        let c = &cnodes[node];
        let dx = route[at] - c.x;
        let dy = route[at + 1] - c.y;
        let side = if (dy.abs() - c.h / 2.0).abs() < 0.5 {
            if dy < 0.0 { 0 } else { 2 }
        } else if (dx.abs() - c.w / 2.0).abs() < 0.5 {
            if dx < 0.0 { 3 } else { 1 }
        } else {
            return;
        };
        let toward = route[next + usize::from(side & 1 == 1)];
        sides
            .entry(node * 4 + side)
            .or_default()
            .push((edge, at, toward));
    };
    for (k, e) in cedges.iter().enumerate() {
        if e.src == e.dst || e.route.len() < 6 {
            continue;
        }
        let last = e.route.len() - 3;
        add(e.src, k, &e.route, 0, 3);
        add(e.dst, k, &e.route, last, last - 3);
    }
    for (key, mut ends) in sides {
        if ends.len() < 2 {
            continue;
        }
        let node = key >> 2;
        let vertical = key & 1 == 1;
        let c = &cnodes[node];
        let (w, h) = if c.is_group {
            (c.w, c.h)
        } else {
            (views[node].w, views[node].h)
        };
        let length = if vertical { h } else { w };
        let count = ends.len() as f64;
        let step = PORT_GAP
            .min((length - 2.0 * PORT_MARGIN) / (count - 1.0))
            .max(0.0);
        let center = if vertical { c.y } else { c.x };
        ends.sort_by(|a, b| a.2.total_cmp(&b.2));
        for (i, (edge, at, _)) in ends.into_iter().enumerate() {
            cedges[edge].route[at + usize::from(vertical)] =
                center + (i as f64 - (count - 1.0) / 2.0) * step;
        }
    }
}

fn inset(route: &mut [f64], at: usize, view: &View, c: &CNode) {
    if c.is_group {
        return;
    }
    let dx = route[at] - c.x;
    let dy = route[at + 1] - c.y;
    if c.w != view.w && (dx.abs() - c.w / 2.0).abs() < 0.5 {
        route[at] -= js_sign(dx) * (c.w - view.w) / 2.0;
    }
    if c.h != view.h && (dy.abs() - c.h / 2.0).abs() < 0.5 {
        route[at + 1] -= js_sign(dy) * (c.h - view.h) / 2.0;
    }
}

pub fn render(source: &str, options: &RenderOptions) -> Result<RenderResult, PeleError> {
    let model = parse(source, options.max_edges)?;
    if runs_across(model.direction) {
        turn_to_fit(options, |down| draw(&model, options, down))
    } else {
        draw(&model, options, false)
    }
}

fn draw(model: &Model, options: &RenderOptions, turned: bool) -> Result<RenderResult, PeleError> {
    let graph = build_graph(model);
    let theme = &options.theme;
    let size = options.font_size;
    let small_size = js_round(size * 0.875);
    let measurer = options.text_measurer();
    let pad = options.padding;
    let er = &options.config["er"];
    let number_option = |key: &str, fallback: f64| {
        er[key]
            .as_f64()
            .filter(|value| *value > 0.0)
            .map_or(fallback, |value| value.min(2000.0))
    };

    let mut index = HashMap::<&str, usize>::new();
    for (i, node) in graph.nodes.iter().enumerate() {
        index.insert(node.id, i);
    }

    let mut cedges = Vec::new();
    let mut edge_labels = Vec::new();
    let mut drawn = Vec::new();
    let mut loops = Vec::<Loop>::new();
    let mut loops_of = IndexMap::<usize, Vec<usize>>::new();
    let mut loop_of = Vec::new();
    for edge in &graph.edges {
        let (Some(&src), Some(&dst)) = (index.get(edge.start), index.get(edge.end)) else {
            continue;
        };
        let label = text::layout_label(edge.label, true, measurer, small_size, 200.0, 0);
        let looped = (src == dst).then(|| {
            loops.push(Loop {
                width: label.width,
                height: label.height,
                band: 0.0,
                at: 0.0,
            });
            loops_of.entry(src).or_default().push(loops.len() - 1);
            loops.len() - 1
        });
        loop_of.push(looped);
        drawn.push(edge);
        cedges.push(CEdge {
            src,
            dst,
            minlen: 1,
            label_w: if label.width > 0.0 {
                label.width + 12.0
            } else {
                0.0
            },
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

    let horizontal = |i: usize| {
        let mut at = i;
        for _ in 0..graph.nodes.len() {
            let Some(up) = graph.nodes[at]
                .parent
                .and_then(|parent| index.get(parent).copied())
            else {
                break;
            };
            if let Some(dir) = graph.nodes[up].dir {
                return dir == "LR" || dir == "RL";
            }
            at = up;
        }
        model.direction == "LR" || model.direction == "RL"
    };

    let mut views = Vec::with_capacity(graph.nodes.len());
    let mut cnodes = Vec::with_capacity(graph.nodes.len());
    for (i, node) in graph.nodes.iter().enumerate() {
        let style = if node.css_styles.is_empty() {
            resolve_style(&node.compiled)
        } else {
            let mut all = node.compiled.clone();
            all.extend(node.css_styles.iter().cloned());
            resolve_style(&all)
        };
        let base = if style.bold { BOLD } else { 0 } | if style.italic { ITALIC } else { 0 };
        let name_size = style.font_size.unwrap_or(size);
        let row_size = style.font_size.unwrap_or(small_size);
        let key_size = style.font_size.unwrap_or_else(|| js_round(size * 0.75));
        let label = if node.is_group {
            text::layout_label(node.label, true, measurer, name_size, 4000.0, base)
        } else {
            let shown = if node.alias.is_empty() {
                node.label
            } else {
                node.alias
            };
            text::layout_label(shown, true, measurer, name_size, 240.0, base)
        };
        let own: &[usize] = loops_of.get(&i).map_or(&[], Vec::as_slice);
        let below = !own.is_empty() && !node.is_group && horizontal(i);
        let mut view = View {
            style,
            label,
            w: 0.0,
            h: 0.0,
            head_h: 0.0,
            rows: Vec::new(),
            cols: Vec::new(),
            row_size,
            key_size,
            below,
        };
        let mut reach: f64 = 0.0;
        let mut bands = 0.0;
        for &k in own {
            let lp = &mut loops[k];
            lp.band = LOOP_BAND.max(if below {
                lp.width + 12.0
            } else {
                lp.height + 6.0
            });
            bands += lp.band;
            reach = reach.max(if below { lp.height } else { lp.width });
        }
        let min_w = MIN_WIDTH.max(if below { bands + 8.0 } else { 0.0 });
        let mut c = if node.is_group {
            let mut c = cnode(0.0, 0.0, None);
            c.is_group = true;
            c.dir = node.dir.map(direction);
            c.pad_x = GROUP_PAD;
            c.pad_top = if view.label.height > 0.0 {
                view.label.height + 16.0
            } else {
                GROUP_PAD
            };
            c.pad_bottom = GROUP_PAD;
            c.min_w = (view.label.width + 2.0 * GROUP_PAD - 12.0).max(MIN_WIDTH);
            c
        } else {
            let name_h = view.label.height.max(js_round(name_size * 1.5));
            if node.attributes.is_empty() {
                view.w = (view.label.width + 2.0 * BOX_PAD_X).max(min_w);
                view.h = (name_h + 2.0 * BOX_PAD_Y).max(if below { 0.0 } else { bands + 8.0 });
            } else {
                let row_line = js_round(row_size * 1.5);
                let mut widths = [0.0_f64; 4];
                for attribute in node.attributes {
                    let cells = [
                        text::layout_label(
                            &cell_text(&attribute.kind),
                            true,
                            measurer,
                            row_size,
                            NO_WRAP,
                            base,
                        ),
                        text::layout_label(
                            &cell_text(&attribute.name),
                            true,
                            measurer,
                            row_size,
                            NO_WRAP,
                            base,
                        ),
                        text::layout_label(
                            &attribute.keys.join(", "),
                            false,
                            measurer,
                            key_size,
                            NO_WRAP,
                            base,
                        ),
                        text::layout_label(
                            &cell_text(&attribute.comment),
                            true,
                            measurer,
                            row_size,
                            COMMENT_WRAP,
                            base,
                        ),
                    ];
                    let mut h = row_line;
                    for (width, cell) in widths.iter_mut().zip(&cells) {
                        *width = width.max(cell.width);
                        h = h.max(cell.height);
                    }
                    view.rows.push(Row {
                        cells,
                        h: h + ROW_PAD,
                    });
                    view.h += h + ROW_PAD;
                }
                let mut columns = 0.0;
                let mut inner = 0.0;
                for &w in &widths {
                    if w == 0.0 {
                        continue;
                    }
                    inner += w + if columns > 0.0 { CELL_GAP } else { 0.0 };
                    columns += 1.0;
                }
                view.w = (inner + 2.0 * CELL_PAD_X)
                    .max(view.label.width + 2.0 * BOX_PAD_X)
                    .max(min_w);
                let spare = if columns > 0.0 {
                    (view.w - inner - 2.0 * CELL_PAD_X) / columns
                } else {
                    0.0
                };
                let mut x = CELL_PAD_X;
                for &w in &widths {
                    view.cols.push(x);
                    if w > 0.0 {
                        x += w + spare + CELL_GAP;
                    }
                }
                view.head_h = name_h + 2.0 * HEAD_PAD_Y;
                view.h += view.head_h;
            }
            let room = if own.is_empty() {
                0.0
            } else {
                2.0 * (MARKER_LENGTH + LOOP * 0.75 + 10.0 + reach)
            };
            if below {
                cnode(view.w, view.h + room, None)
            } else {
                cnode(view.w + room, view.h, None)
            }
        };
        c.seq = views.len();
        views.push(view);
        cnodes.push(c);
    }
    for (c, node) in cnodes.iter_mut().zip(&graph.nodes) {
        c.parent = node.parent.and_then(|parent| index.get(parent).copied());
    }

    let layout = compound_layout(
        &mut cnodes,
        &mut cedges,
        if turned {
            Dir::Tb
        } else {
            direction(model.direction)
        },
        &LayeredOptions {
            node_sep: number_option("nodeSpacing", NODE_SEP),
            edge_sep: EDGE_SEP,
            rank_sep: number_option("rankSpacing", RANK_SEP),
            port_sep: 24.0,
            ..LayeredOptions::default()
        },
    );

    spread_ends(&mut cedges, &cnodes, &views);

    let mut layout_width = layout.width;
    for (&i, own) in &loops_of {
        let c = &cnodes[i];
        if !c.is_group {
            continue;
        }
        let reach = own.iter().map(|&k| loops[k].width).fold(0.0, f64::max);
        layout_width =
            layout_width.max(c.x + c.w / 2.0 + MARKER_LENGTH + LOOP * 0.75 + 10.0 + reach);
    }

    let title = title_label(options.title.as_deref(), options, size, 4000.0);
    let title_height = if title.height > 0.0 {
        title.height + 12.0
    } else {
        0.0
    };
    let inner = layout_width.max(title.width);
    shift_layout(
        &mut cnodes,
        &mut cedges,
        pad + (inner - layout_width) / 2.0,
        pad + title_height,
    );
    let width = (inner + 2.0 * pad).ceil();
    let height = (layout.height + title_height + 2.0 * pad).ceil();
    let mut links = Vec::new();

    let mut crossings = Crossings::new();
    if cnodes.iter().any(|c| c.is_group) {
        for e in &cedges {
            mark_crossings(&mut crossings, &e.route);
        }
    }

    let background = esc(&theme.background);
    let surface = esc(&theme.surface);
    let border = esc(&theme.border);
    let muted_color = esc(&theme.muted);
    let box_attrs = format!(" fill=\"{surface}\" stroke=\"{border}\"");
    let mut clusters = String::new();
    let mut fills = Vec::new();
    let mut nodes_out = String::new();
    for (i, view) in views.iter_mut().enumerate() {
        let c = &cnodes[i];
        let node = &graph.nodes[i];
        let classes = class_names(node.css_classes.strip_prefix("default").map_or(
            node.css_classes.as_str(),
            |rest| {
                rest.strip_prefix(|ch: char| is_space(ch as u32))
                    .unwrap_or(rest)
            },
        ));
        if c.is_group {
            view.w = c.w;
            view.h = c.h;
            let x = c.x - c.w / 2.0;
            let y = c.y - c.h / 2.0;
            let title_x = cluster_title_x(c, view.label.width, &crossings, GROUP_PAD);
            if paints(&view.style.shape) {
                fills.push(Fill {
                    bounds: [x, y, x + c.w, y + c.h],
                    shape: view.style.shape.clone(),
                });
            }
            let _ = write!(
                clusters,
                "<g class=\"pele-cluster{classes}\" data-id=\"{}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"{} stroke=\"{border}\"{}/>{}</g>",
                esc_text(node.id),
                num(x),
                num(y),
                num(c.w),
                num(c.h),
                esc(&theme.surface_alt),
                if sets_fill(&view.style.shape) {
                    ""
                } else {
                    " fill-opacity=\"0.5\""
                },
                rounded(&view.style.shape, theme.radius),
                label_with_options(
                    &view.label,
                    title_x,
                    y + 8.0 + view.label.height / 2.0,
                    &format!(
                        " class=\"pele-cluster-label\"{}{}",
                        ink_fill(&view.style, Some(&theme.muted)),
                        view.style.text
                    ),
                    options,
                )
            );
            continue;
        }

        let (w, h) = (view.w, view.h);
        let (x0, y0) = (-w / 2.0, -h / 2.0);
        let text_attrs = &view.style.text;
        let color = ink_fill(&view.style, None);
        let muted = if color.is_empty() {
            format!(" fill=\"{muted_color}\"")
        } else {
            color.clone()
        };
        let shape = format!(
            "x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"",
            num(x0),
            num(y0),
            num(w),
            num(h)
        );
        let mut name = label_with_options(
            &view.label,
            0.0,
            if view.rows.is_empty() {
                0.0
            } else {
                y0 + view.head_h / 2.0
            },
            &format!(" class=\"pele-label\"{color}{text_attrs}"),
            options,
        );
        let display = text::decode_entities(if node.alias.is_empty() {
            node.label
        } else {
            node.alias
        });
        if format!("{classes} ").contains(" internal-link ") && !name.is_empty() {
            let safe = internal_link(&display);
            let target = esc(&display);
            name = format!(
                "<a class=\"internal-link\"{} data-href=\"{target}\">{name}</a>",
                if safe {
                    format!(" href=\"{target}\" rel=\"noopener\"")
                } else {
                    String::new()
                }
            );
            if safe {
                links.push(LinkInfo {
                    id: node.label.to_owned(),
                    href: display,
                    internal: true,
                });
            }
        }

        let body = if view.rows.is_empty() {
            format!(
                "<rect {shape}{}/>{name}",
                rounded(&format!("{box_attrs}{}", view.style.shape), theme.radius)
            )
        } else {
            let row_line = js_round(view.row_size * 1.5);
            let line_color = view
                .style
                .stroke
                .as_deref()
                .map_or_else(|| border.clone(), esc);
            let mut stripes = String::new();
            let mut cells = String::new();
            let mut y = y0 + view.head_h;
            let last = view.rows.len() - 1;
            for (r, row) in view.rows.iter().enumerate() {
                if r % 2 == 0 {
                    let span = format!("x=\"{}\" width=\"{}\"", num(x0), num(w));
                    if r == last {
                        let _ = write!(
                            stripes,
                            "<rect {span} y=\"{}\" height=\"{}\"{}/><rect {span} y=\"{}\" height=\"{}\"/>",
                            num(y),
                            num(row.h),
                            rounded_attr(theme.radius),
                            num(y),
                            num(row.h / 2.0)
                        );
                    } else {
                        let _ = write!(
                            stripes,
                            "<rect {span} y=\"{}\" height=\"{}\"/>",
                            num(y),
                            num(row.h)
                        );
                    }
                }
                let baseline = y + ROW_PAD / 2.0 + row_line / 2.0 + view.row_size * 0.35;
                let [kind, attribute, keys, comment] = &row.cells;
                cells.push_str(&text_left(
                    kind,
                    x0 + view.cols[0],
                    baseline,
                    &format!(" class=\"pele-er-type\"{muted}{text_attrs}"),
                    options,
                ));
                cells.push_str(&text_left(
                    attribute,
                    x0 + view.cols[1],
                    baseline,
                    &format!(" class=\"pele-er-name\"{color}{text_attrs}"),
                    options,
                ));
                cells.push_str(&text_left(
                    keys,
                    x0 + view.cols[2],
                    baseline,
                    &format!(
                        " class=\"pele-er-keys\" font-size=\"{}\"{muted}{text_attrs}",
                        view.key_size
                    ),
                    options,
                ));
                cells.push_str(&text_left(
                    comment,
                    x0 + view.cols[3],
                    baseline,
                    &format!(" class=\"pele-er-comment\"{muted}{text_attrs}"),
                    options,
                ));
                y += row.h;
            }
            let divider_y = num(y0 + view.head_h);
            let shade = match ink(&view.style.shape, "") {
                Some(tone) if tone == ON_LIGHT => ON_DARK.to_owned(),
                Some(tone) if tone == ON_DARK => ON_LIGHT.to_owned(),
                _ => background.clone(),
            };
            format!(
                "<rect {shape}{}/><g class=\"pele-er-stripes\" fill=\"{shade}\"{}>{stripes}</g><path class=\"pele-er-divider\" d=\"M{},{divider_y}H{}\" stroke=\"{line_color}\"/><rect {shape} fill=\"none\" stroke=\"{border}\"{}/>{name}<g class=\"pele-er-attributes\" font-size=\"{}\">{cells}</g>",
                rounded(&format!("{box_attrs}{}", view.style.shape), theme.radius),
                if view.style.shape.contains("fill:") {
                    " opacity=\"0.6\""
                } else {
                    ""
                },
                num(x0),
                num(x0 + w),
                rounded(&view.style.line, theme.radius),
                view.row_size
            )
        };
        let _ = write!(
            nodes_out,
            "<g class=\"pele-node pele-entity{classes}\" data-id=\"{}\" transform=\"translate({},{})\">{body}</g>",
            esc_text(node.label),
            num(c.x),
            num(c.y)
        );
    }

    for (&i, own) in &loops_of {
        let view = &views[i];
        let total: f64 = own.iter().map(|&k| loops[k].band).sum();
        let side = if view.below { view.w } else { view.h };
        let scale = 1.0_f64.min((side - 8.0).max(0.0) / total);
        let mut at = -total * scale / 2.0;
        for &k in own {
            let lp = &mut loops[k];
            lp.band *= scale;
            lp.at = at + lp.band / 2.0;
            at += lp.band;
        }
    }

    let mut edges_out = String::new();
    let mut casings = String::new();
    let mut labels_out = String::new();
    for (i, edge) in drawn.iter().enumerate() {
        let label = &edge_labels[i];
        let (d, mut start, mut end) = if let Some(k) = loop_of[i] {
            let lp = &loops[k];
            let ce = &mut cedges[i];
            let c = &cnodes[ce.src];
            let view = &views[ce.src];
            let spread = (lp.band / 2.0 - 6.0).clamp(1.0, 13.0);
            let far = MARKER_LENGTH + LOOP * 0.75 + 6.0;
            if view.below {
                let x = c.x + lp.at;
                let y = c.y + view.h / 2.0;
                let turn = num(y + MARKER_LENGTH);
                let out = num(y + MARKER_LENGTH + LOOP);
                ce.label_x = x;
                ce.label_y = y + far + label.height / 2.0;
                (
                    format!(
                        "M{},{}V{turn}C{},{out} {},{out} {},{turn}V{}",
                        num(x - spread),
                        num(y),
                        num(x - spread),
                        num(x + spread),
                        num(x + spread),
                        num(y)
                    ),
                    er_marker(edge.start_kind, x - spread, y, 0.0, -1.0, &background),
                    er_marker(edge.end_kind, x + spread, y, 0.0, -1.0, &background),
                )
            } else {
                let x = c.x + view.w / 2.0;
                let y = c.y + lp.at;
                let turn = num(x + MARKER_LENGTH);
                let out = num(x + MARKER_LENGTH + LOOP);
                ce.label_x = x + far + label.width / 2.0;
                ce.label_y = y;
                (
                    format!(
                        "M{},{}H{turn}C{out},{} {out},{} {turn},{}H{}",
                        num(x),
                        num(y - spread),
                        num(y - spread),
                        num(y + spread),
                        num(y + spread),
                        num(x)
                    ),
                    er_marker(edge.start_kind, x, y - spread, -1.0, 0.0, &background),
                    er_marker(edge.end_kind, x, y + spread, -1.0, 0.0, &background),
                )
            }
        } else {
            let ce = &cedges[i];
            let mut route = straighten(&ce.route);
            if route.len() < 6 {
                continue;
            }
            let last = route.len() - 3;
            inset(&mut route, 0, &views[ce.src], &cnodes[ce.src]);
            inset(&mut route, last, &views[ce.dst], &cnodes[ce.dst]);
            let dx = (route[last] - route[0]).abs();
            let dy = (route[last + 1] - route[1]).abs();
            let direct =
                route.len() == 6 && (dx < 0.01 || dy < 0.01 || dx.hypot(dy) <= 3.0 * MARKER_LENGTH);
            let stub = if direct { 0.0 } else { MARKER_LENGTH };
            let path = route_path(&route, None, stub, stub, None, None);
            let d = if stub > 0.0 {
                format!(
                    "M{},{}L{}L{},{}",
                    num(path.sx),
                    num(path.sy),
                    &path.d[1..],
                    num(path.ex),
                    num(path.ey)
                )
            } else {
                path.d.clone()
            };
            (
                d,
                er_marker(
                    edge.start_kind,
                    path.sx,
                    path.sy,
                    path.sdx,
                    path.sdy,
                    &background,
                ),
                er_marker(
                    edge.end_kind,
                    path.ex,
                    path.ey,
                    path.edx,
                    path.edy,
                    &background,
                ),
            )
        };
        let under = fill_under(&d, &[], &fills, None);
        let over = match under {
            FillUnder::Off => None,
            FillUnder::Border => Some(String::new()),
            FillUnder::Inside(fill) => Some(edge_ink(fill, "")),
        };
        if over.as_deref() == Some("") {
            casings.push_str(&casing(&d, &format!("{start}{end}"), "", false));
        }
        if let FillUnder::Inside(fill) = under
            && over.as_deref().is_some_and(|ink| !ink.is_empty())
        {
            start = hollow_on(&start, fill, &theme.background);
            end = hollow_on(&end, fill, &theme.background);
        }
        let _ = write!(
            edges_out,
            "<g class=\"pele-edge pele-er-{}\" data-id=\"{}\"{}><path d=\"{d}\"{}/>{start}{end}</g>",
            if edge.dashed {
                "non-identifying"
            } else {
                "identifying"
            },
            esc_text(&edge.id),
            over.as_deref()
                .filter(|ink| !ink.is_empty())
                .map_or_else(String::new, |ink| format!(" stroke=\"{ink}\"")),
            if edge.dashed {
                " stroke-dasharray=\"4 3\""
            } else {
                ""
            }
        );
        if label.width > 0.0 {
            labels_out.push_str(&edge_label_svg(
                &edge.id,
                label,
                cedges[i].label_x,
                cedges[i].label_y,
                "",
                options,
            ));
        }
    }

    let mut contents = label_with_options(
        &title,
        width / 2.0,
        pad + title.height / 2.0,
        " class=\"pele-title\" font-weight=\"bold\"",
        options,
    );
    if !clusters.is_empty() {
        let _ = write!(contents, "<g class=\"pele-clusters\">{clusters}</g>");
    }
    contents.push_str(&casings_svg(&casings, &theme.background));
    if !edges_out.is_empty() {
        let _ = write!(
            contents,
            "<g class=\"pele-edges\" fill=\"none\" stroke=\"{}\" stroke-linecap=\"round\">{edges_out}</g>",
            esc(&theme.line)
        );
    }
    if !labels_out.is_empty() {
        let _ = write!(
            contents,
            "<g class=\"pele-edge-labels\" font-size=\"{small_size}\">{labels_out}</g>"
        );
    }
    let _ = write!(contents, "<g class=\"pele-nodes\">{nodes_out}</g>");
    let mut effective = options.clone();
    effective.acc_title = model.acc_title.clone().or(effective.acc_title);
    effective.acc_descr = model.acc_descr.clone().or(effective.acc_descr);
    let svg = crate::svg::document("er", width, height, &effective, &contents);
    let mut result = RenderResult::new("er", svg, width, height);
    result.links = links;
    Ok(result)
}
