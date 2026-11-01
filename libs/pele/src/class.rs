use std::collections::HashMap;
use std::rc::Rc;
use std::sync::LazyLock;

use indexmap::{IndexMap, IndexSet};
use regex::Regex;

use crate::components::wrap_components;
use crate::compound::{CEdge, CNode, Dir, cnode, compound_layout, direction};
use crate::fit_width::{runs_across, tighten, turn_to_fit};
use crate::flow_shapes::end_side;
use crate::layout::LayeredOptions;
use crate::svg::cluster::{
    Crossings, cluster_title_x, js_round, mark_crossings, struck_title, title_crossed,
};
use crate::svg::edges::{EdgePath, edge_label_svg, marker, marker_trim, route_path};
use crate::svg::ink::ink_fill;
use crate::svg::theme::{ResolvedStyle, class_names, resolve_style, rounded, rounded_attr};
use crate::svg::title::with_title;
use crate::text::{BOLD, ITALIC};
use crate::{LinkInfo, PeleError, RenderOptions, RenderResult, svg, text};

const MAX_DEPTH: usize = 128;
const MAX_NODES: usize = 20_000;
const MAX_MEMBERS: usize = 50_000;
const NODE_SEP: f64 = 40.0;
const EDGE_SEP: f64 = 16.0;
const RANK_SEP: f64 = 56.0;
const GROUP_PAD: f64 = 20.0;
const LOOP: f64 = 26.0;
const PAD_X: f64 = 12.0;
const TITLE_PAD: f64 = 8.0;
const SECTION_PAD: f64 = 6.0;
const EMPTY_SECTION: f64 = 10.0;
const MIN_WIDTH: f64 = 64.0;
const FOLD: f64 = 8.0;
const TRIANGLE: f64 = 12.0;
const DIAMOND: f64 = 16.0;
const HALF: f64 = 6.0;
const RING: f64 = 5.0;
const TARGETS: [&str; 4] = ["_self", "_blank", "_parent", "_top"];
const BLANK: &str = "about:blank";
const LINK_SCHEMES: [&str; 4] = ["http", "https", "mailto", "tel"];

const LETTERS: &str = "4q 0 b 0 5 0 6 m 2 u 2 cp 5 b f 4 8 0 2 0 3m 4 2 1 3 3 9 0 2 2 2 0 2 j 2 2a 2 3u 9 4d a \
    11 3 0 8 12 21 q 6 2 1a 16 10 1 2 2q 2 0 g 1 8 1 b 2 3 0 h 0 2 t u 2g c 0 p w a 1 5 0 6 \
    l 5 0 a 0 4 0 o o 20 0 2 a 2g 1h 4 0 j 0 8 9 g 6 2 6 6 7 3 1 3 l 2 6 2 0 4 3 4 0 h 0 e \
    1 2 2 f 1 k 5 5 1 3 l 2 6 2 1 2 1 2 1 w 3 2 0 k 2 h 8 2 2 2 l 2 6 2 1 2 4 4 0 j 0 g 1 \
    10 7 3 1 3 l 2 6 2 1 2 4 4 0 v 1 2 2 g 0 i 0 2 5 4 2 2 3 4 1 2 0 2 1 4 1 4 2 4 b n 0 1h \
    7 2 2 2 m 2 9 2 4 4 0 r 1 7 1 10 7 2 2 2 m 2 9 2 4 4 0 x 0 2 1 g 1 j 7 2 2 2 14 3 0 h 0 \
    i 1 p 5 6 h 4 n 2 8 2 0 3 6 1n 1b 2 1 d 6 1n 1 2 0 3 1 2 0 3 0 7 3 2 6 2 2 2 0 2 0 3 1 \
    2 3 2 1 a 0 3 4 2 0 m 3 x 0 1s 7 2 z s 4 38 16 l 0 h 5 5 3 4 0 4 1 8 2 5 c d 0 i 11 2 0 \
    6 0 3 16 2 98 2 3 3 6 2 0 2 3 3 14 2 3 3 w 2 3 3 6 2 0 2 3 3 e 2 1k 2 3 3 1u 12 f h 2c \
    d h7 3 g 2 p 6 22 m c 2 3 f h f h f c 2 2 g 1f 10 0 5 0 1w 2f 9 14 2 0 6 1x b s 1g t 3 \
    4 c 17 m 6 1l m a 1g 2b 0 2m 1a i 6 1k t e 1 b 17 r z 16 2 b z 30 3 2 3 4 1 a 5b 1t 7p \
    3 5 3 11 3 5 3 7 2 0 2 0 2 0 2 u 3 1g 2 6 2 0 4 2 2 6 4 3 3 5 5 c 6 2 2 6 39 0 e 0 h c \
    2u 0 5 0 3 9 2 0 4 4 7 0 2 0 2 0 2 3 2 a 3 3 6 4 5 0 1h 1 22k 1a 2 1a 2 3o 7 3 4 1 d 11 \
    2 0 6 0 3 1j 8 0 h m a 6 2 6 2 6 2 6 2 6 2 6 2 6 2 6 29 0 d2 1 17 4 6 1 5 2d 7 2 2 2h 2 \
    3 6 14 4 2l i q 1i f e9 52t 23 g5o 1g wc 1w 19 3 7g 4 f b 1 l 1a h o 9 1x 1e 8 3 2u 3 3 \
    2 3 d a 26 9 2 2 2 3 2 m u 1f f 1d 1r 5 4 0 f r b m q s 8 1a t 0 1d 14 o 2 2 7 l m 4 0 \
    6 1b 2 0 4 1 3 4 3 0 2 0 p 2 3 a 8 2 d 5 3 5 3 5 a 6 2 6 42 y u 8mb d m 5 1c 6it a5 3 \
    2x 13 6 d 4 6 0 2 9 2 c 2 4 2 0 2 1 2 1 2 2z y a2 j 1r 3 1h 15 b 39 4 2 3q 11 p 7 p c \
    2g 4 5 3 5 3 5 3 2";
static LETTER_RANGES: LazyLock<Vec<u32>> = LazyLock::new(|| {
    let parts: Vec<u32> = LETTERS
        .split(' ')
        .map(|part| u32::from_str_radix(part, 36).expect("packed letter table"))
        .collect();
    let mut out = Vec::with_capacity(parts.len());
    let mut prev = 0;
    for pair in parts.chunks(2) {
        let start = prev + pair[0];
        prev = start + pair[1];
        out.push(start);
        out.push(prev);
    }
    out
});

static RE_HTML_ENTITIES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"&#([A-Za-z0-9_]+);?").unwrap());
static RE_HTML_CTRL_ENTITY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)&(?:newline|tab);").unwrap());
static RE_WS_ESCAPE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:\\|%5[cC])(?:%(?:6[eE]|72|74)|[nrt])").unwrap());
static RE_INVALID_PROTOCOL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[^A-Za-z0-9_]*(?i:javascript|data|vbscript)").unwrap());
static RE_SCHEME_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([A-Za-z][A-Za-z0-9+.-]*):").unwrap());

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

fn is_line_end(c: u32) -> bool {
    matches!(c, 10 | 13 | 0x2028 | 0x2029)
}

fn is_unicode_letter(code: u32) -> bool {
    if !(0xaa..=0xffdc).contains(&code) {
        return false;
    }
    let table = &*LETTER_RANGES;
    let (mut lo, mut hi) = (0_isize, (table.len() / 2) as isize - 1);
    while lo <= hi {
        let mid = ((lo + hi) / 2) as usize;
        if code < table[mid * 2] {
            hi = mid as isize - 1;
        } else if code > table[mid * 2 + 1] {
            lo = mid as isize + 1;
        } else {
            return true;
        }
    }
    false
}

fn js_space(ch: char) -> bool {
    is_space(ch as u32)
}

fn js_trim(text: &str) -> &str {
    text.trim_matches(js_space)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum T {
    End,
    Error,
    ClassDiagram,
    Newline,
    Eof,
    Sqs,
    Str,
    Sqe,
    Dot,
    GenericType,
    Label,
    AccTitle,
    AccTitleValue,
    AccDescr,
    AccDescrValue,
    AccDescrMultilineValue,
    StructStart,
    StructStop,
    Namespace,
    StyleSeparator,
    AnnotationStart,
    AnnotationEnd,
    Class,
    Member,
    NoteFor,
    Note,
    ClassDef,
    Alpha,
    Comma,
    DirectionTb,
    DirectionBt,
    DirectionRl,
    DirectionLr,
    Aggregation,
    Extension,
    Composition,
    Dependency,
    Lollipop,
    Line,
    DottedLine,
    Callback,
    Link,
    LinkTarget,
    Click,
    CallbackName,
    CallbackArgs,
    Href,
    Style,
    CssClass,
    Colon,
    Brkt,
    Pct,
    Minus,
    UnicodeText,
    BquoteStr,
    EdgeState,
    EofInStruct,
    OpenInStruct,
    Plus,
    Equals,
    Punctuation,
}

const TOKEN_NAMES: [&str; 61] = [
    "$end",
    "INVALID",
    "CLASS_DIAGRAM",
    "NEWLINE",
    "EOF",
    "SQS",
    "STR",
    "SQE",
    "DOT",
    "GENERICTYPE",
    "LABEL",
    "acc_title",
    "acc_title_value",
    "acc_descr",
    "acc_descr_value",
    "acc_descr_multiline_value",
    "STRUCT_START",
    "STRUCT_STOP",
    "NAMESPACE",
    "STYLE_SEPARATOR",
    "ANNOTATION_START",
    "ANNOTATION_END",
    "CLASS",
    "MEMBER",
    "NOTE_FOR",
    "NOTE",
    "CLASSDEF",
    "ALPHA",
    "COMMA",
    "direction_tb",
    "direction_bt",
    "direction_rl",
    "direction_lr",
    "AGGREGATION",
    "EXTENSION",
    "COMPOSITION",
    "DEPENDENCY",
    "LOLLIPOP",
    "LINE",
    "DOTTED_LINE",
    "CALLBACK",
    "LINK",
    "LINK_TARGET",
    "CLICK",
    "CALLBACK_NAME",
    "CALLBACK_ARGS",
    "HREF",
    "STYLE",
    "CSSCLASS",
    "COLON",
    "BRKT",
    "PCT",
    "MINUS",
    "UNICODE_TEXT",
    "BQUOTE_STR",
    "EDGE_STATE",
    "EOF_IN_STRUCT",
    "OPEN_IN_STRUCT",
    "PLUS",
    "EQUALS",
    "PUNCTUATION",
];

const DIRECTIONS: [(&str, T); 4] = [
    ("TB", T::DirectionTb),
    ("BT", T::DirectionBt),
    ("RL", T::DirectionRl),
    ("LR", T::DirectionLr),
];

fn alphanum(t: T) -> bool {
    matches!(t, T::UnicodeText | T::Alpha | T::Minus)
}

fn style_part(t: T) -> bool {
    matches!(
        t,
        T::Alpha | T::Colon | T::Brkt | T::Style | T::Pct | T::Label
    )
}

fn end_type(t: T) -> Option<u8> {
    match t {
        T::Aggregation => Some(0),
        T::Extension => Some(1),
        T::Composition => Some(2),
        T::Dependency => Some(3),
        T::Lollipop => Some(4),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum S {
    Initial,
    Str,
    Bq,
    Generic,
    CallbackName,
    CallbackArgs,
    AccTitle,
    AccDescr,
    AccDescrMultiline,
    Cls,
    ClassBody,
    Namespace,
    NamespaceBody,
}

impl S {
    fn at_end(self) -> T {
        match self {
            Self::Initial | Self::Bq | Self::Cls | Self::Namespace => T::Eof,
            Self::Str => T::Str,
            Self::Generic => T::GenericType,
            Self::CallbackName => T::CallbackName,
            Self::CallbackArgs => T::CallbackArgs,
            Self::AccTitle => T::AccTitleValue,
            Self::AccDescr => T::AccDescrValue,
            Self::AccDescrMultiline => T::AccDescrMultilineValue,
            Self::ClassBody | Self::NamespaceBody => T::EofInStruct,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Tok {
    kind: T,
    start: usize,
    end: usize,
}

fn index_of(src: &[u16], word: &str, from: usize) -> Option<usize> {
    let word: Vec<u16> = word.encode_utf16().collect();
    if word.is_empty() || from > src.len() {
        return None;
    }
    src[from..]
        .windows(word.len())
        .position(|window| window == word.as_slice())
        .map(|at| from + at)
}

struct Lexer<'a> {
    src: &'a [u16],
    p: usize,
    state: S,
    stack: Vec<S>,
    toks: Vec<Tok>,
    dir_next: Option<usize>,
    line_end: Option<usize>,
    dir_failed: usize,
}

impl Lexer<'_> {
    fn at(&self, i: usize) -> u32 {
        self.src.get(i).map_or(u32::MAX, |&c| u32::from(c))
    }

    fn starts(&self, word: &str, at: usize) -> bool {
        word.bytes()
            .enumerate()
            .all(|(k, b)| self.src.get(at + k) == Some(&u16::from(b)))
    }

    fn emit(&mut self, kind: T, start: usize, end: usize) -> bool {
        self.toks.push(Tok { kind, start, end });
        self.p = end;
        true
    }

    fn push(&mut self, state: S) {
        self.stack.push(self.state);
        self.state = state;
    }

    fn pop(&mut self) {
        if let Some(state) = self.stack.pop() {
            self.state = state;
        }
    }

    fn kw(&self, word: &str) -> bool {
        self.starts(word, self.p) && !is_word(self.at(self.p + word.len()))
    }

    fn skip_ws(&self, mut q: usize) -> usize {
        while q < self.src.len() && is_space(u32::from(self.src[q])) {
            q += 1;
        }
        q
    }

    fn until(&self, ch: u16) -> usize {
        self.src[self.p..]
            .iter()
            .position(|&c| c == ch)
            .map_or(self.src.len(), |k| self.p + k)
    }

    fn space(&mut self, leaves: bool) {
        let n = self.src.len();
        let mut q = self.p;
        let mut k = None;
        while q < n && is_space(u32::from(self.src[q])) {
            if self.src[q] == 10 {
                k = Some(q);
            }
            q += 1;
        }
        let Some(k) = k else {
            self.p = q;
            return;
        };
        if leaves {
            self.pop();
        }
        self.emit(T::Newline, self.p, k + 1);
    }

    fn keyword(&mut self, c: u32) -> bool {
        let p = self.p;
        match c {
            99 => {
                if self.kw("cssClass") {
                    return self.emit(T::CssClass, p, p + 8);
                }
                if self.kw("callback") {
                    return self.emit(T::Callback, p, p + 8);
                }
                if self.kw("click") {
                    return self.emit(T::Click, p, p + 5);
                }
            }
            108 if self.kw("link") => return self.emit(T::Link, p, p + 4),
            110 => {
                if self.kw("note for") {
                    return self.emit(T::NoteFor, p, p + 8);
                }
                if self.kw("note") {
                    return self.emit(T::Note, p, p + 4);
                }
            }
            60 if self.at(p + 1) == 60 => return self.emit(T::AnnotationStart, p, p + 2),
            62 if self.at(p + 1) == 62 => return self.emit(T::AnnotationEnd, p, p + 2),
            104 if self.kw("href") => return self.emit(T::Href, p, p + 4),
            _ => {}
        }
        false
    }

    fn common(&mut self, c: u32) -> bool {
        if self.keyword(c) {
            return true;
        }
        let p = self.p;
        let n = self.src.len();
        let d = self.at(p + 1);
        match c {
            34 => {
                self.push(S::Str);
                self.p += 1;
                return true;
            }
            126 => {
                self.push(S::Generic);
                self.p += 1;
                return true;
            }
            96 => {
                self.push(S::Bq);
                self.p += 1;
                return true;
            }
            95 => {
                for target in TARGETS {
                    if self.kw(target) {
                        return self.emit(T::LinkTarget, p, p + target.len());
                    }
                }
            }
            60 => {
                return if d == 124 {
                    self.emit(T::Extension, p, p + 2)
                } else {
                    self.emit(T::Dependency, p, p + 1)
                };
            }
            124 => return d == 62 && self.emit(T::Extension, p, p + 2),
            62 => return self.emit(T::Dependency, p, p + 1),
            42 => return self.emit(T::Composition, p, p + 1),
            111 if !is_word(d) => return self.emit(T::Aggregation, p, p + 1),
            40 => return d == 41 && self.emit(T::Lollipop, p, p + 2),
            45 => {
                return if d == 45 {
                    self.emit(T::Line, p, p + 2)
                } else {
                    self.emit(T::Minus, p, p + 1)
                };
            }
            46 => {
                return if d == 46 {
                    self.emit(T::DottedLine, p, p + 2)
                } else {
                    self.emit(T::Dot, p, p + 1)
                };
            }
            58 => {
                let mut q = p + 1;
                while q < n && !matches!(self.src[q], 58 | 10 | 59) {
                    q += 1;
                }
                if q > p + 1 {
                    return self.emit(T::Label, p, q);
                }
                return d == 58 && self.at(p + 2) == 58 && self.emit(T::StyleSeparator, p, p + 3);
            }
            43 => return self.emit(T::Plus, p, p + 1),
            37 => return self.emit(T::Pct, p, p + 1),
            61 => return self.emit(T::Equals, p, p + 1),
            91 => return self.emit(T::Sqs, p, p + 1),
            93 => return self.emit(T::Sqe, p, p + 1),
            33 | 35 | 36 | 38 | 39 | 44 | 63 | 92 | 47 => {
                return self.emit(T::Punctuation, p, p + 1);
            }
            _ => {}
        }
        if is_word(c) {
            let mut q = p + 1;
            while is_word(self.at(q)) {
                q += 1;
            }
            return self.emit(T::Alpha, p, q);
        }
        is_unicode_letter(c) && self.emit(T::UnicodeText, p, p + 1)
    }

    fn direction_end(&self, key: &str, line_end: usize, first: usize) -> Option<usize> {
        let n = self.src.len();
        let mut q = line_end.checked_sub(9)?;
        loop {
            if q < first.max(self.p) {
                return None;
            }
            if self.starts("direction", q) {
                let mut r = q + 9;
                while r < n && is_space(u32::from(self.src[r])) {
                    r += 1;
                }
                if r > q + 9 && self.starts(key, r) {
                    let mut e = r + key.len();
                    while e < n && self.src[e] != 10 {
                        e += 1;
                    }
                    return Some(e);
                }
            }
            q = q.checked_sub(1)?;
        }
    }

    fn direction(&mut self) -> bool {
        let Some(mut next) = self.dir_next else {
            return false;
        };
        if self.p < self.dir_failed {
            return false;
        }
        if next < self.p {
            match index_of(self.src, "direction", self.p) {
                Some(found) => {
                    next = found;
                    self.dir_next = Some(found);
                }
                None => {
                    self.dir_next = None;
                    return false;
                }
            }
        }
        let line_end = match self.line_end {
            Some(end) if end >= self.p => end,
            _ => {
                let end = self.src[self.p..]
                    .iter()
                    .position(|&c| is_line_end(u32::from(c)))
                    .map_or(self.src.len(), |k| self.p + k);
                self.line_end = Some(end);
                end
            }
        };
        if next >= line_end {
            return false;
        }
        for (key, kind) in DIRECTIONS {
            if let Some(end) = self.direction_end(key, line_end, next) {
                return self.emit(kind, self.p, end);
            }
        }
        self.dir_failed = line_end;
        false
    }

    fn initial(&mut self, c: u32) -> bool {
        if self.direction() {
            return true;
        }
        if is_space(c) {
            self.space(false);
            return true;
        }
        let p = self.p;
        let n = self.src.len();
        match c {
            37 if self.at(p + 1) == 37 => {
                let mut e = self.until(10);
                while e < n && matches!(self.src[e], 10 | 13) {
                    e += 1;
                }
                self.p = e;
                return true;
            }
            97 => {
                let title = self.starts("accTitle", p);
                if title || self.starts("accDescr", p) {
                    let q = self.skip_ws(p + 8);
                    let d = self.at(q);
                    if d == 58 {
                        let end = self.skip_ws(q + 1);
                        self.emit(if title { T::AccTitle } else { T::AccDescr }, p, end);
                        self.push(if title { S::AccTitle } else { S::AccDescr });
                        return true;
                    }
                    if d == 123 && !title {
                        self.p = self.skip_ws(q + 1);
                        self.push(S::AccDescrMultiline);
                        return true;
                    }
                }
            }
            99 => {
                if self.kw("classDiagram-v2") {
                    return self.emit(T::ClassDiagram, p, p + 15);
                }
                if self.kw("classDiagram") {
                    return self.emit(T::ClassDiagram, p, p + 12);
                }
                if self.starts("call", p) && is_space(self.at(p + 4)) {
                    self.p = self.skip_ws(p + 4);
                    self.push(S::CallbackName);
                    return true;
                }
                if self.kw("classDef") {
                    return self.emit(T::ClassDef, p, p + 8);
                }
                if self.kw("class") {
                    self.push(S::Cls);
                    return self.emit(T::Class, p, p + 5);
                }
            }
            91 if self.starts("*]", p + 1) => return self.emit(T::EdgeState, p, p + 3),
            115 if self.kw("style") => return self.emit(T::Style, p, p + 5),
            110 if self.kw("namespace") => {
                self.push(S::Namespace);
                return self.emit(T::Namespace, p, p + 9);
            }
            44 => return self.emit(T::Comma, p, p + 1),
            35 => return self.emit(T::Brkt, p, p + 1),
            _ => {}
        }
        self.common(c) || (c == 58 && self.emit(T::Colon, p, p + 1))
    }

    fn step(&mut self, c: u32) -> bool {
        let p = self.p;
        let n = self.src.len();
        match self.state {
            S::Initial => return self.initial(c),
            S::Cls => {
                if is_space(c) {
                    self.space(true);
                } else if c == 125 {
                    self.pop();
                    self.pop();
                    self.emit(T::StructStop, p, p + 1);
                } else if c == 123 {
                    self.push(S::ClassBody);
                    self.emit(T::StructStart, p, p + 1);
                } else {
                    return self.common(c);
                }
            }
            S::ClassBody => {
                if c == 34 {
                    self.push(S::Str);
                    self.p += 1;
                } else if c == 125 {
                    self.pop();
                    self.emit(T::StructStop, p, p + 1);
                } else if c == 91 && self.starts("*]", p + 1) {
                    self.emit(T::EdgeState, p, p + 3);
                } else if c == 123 {
                    self.emit(T::OpenInStruct, p, p + 1);
                } else if c == 10 {
                    self.p += 1;
                } else {
                    let mut q = p + 1;
                    while q < n && !matches!(self.src[q], 123 | 125 | 10) {
                        q += 1;
                    }
                    self.emit(T::Member, p, q);
                }
            }
            S::Namespace => {
                if is_space(c) {
                    self.space(true);
                } else if c == 123 {
                    self.push(S::NamespaceBody);
                    self.emit(T::StructStart, p, p + 1);
                } else if c == 125 {
                    self.pop();
                } else if c == 110 && self.kw("namespace") {
                    self.push(S::Namespace);
                    self.emit(T::Namespace, p, p + 9);
                } else {
                    return self.common(c);
                }
            }
            S::NamespaceBody => {
                if is_space(c) {
                    self.space(false);
                } else if c == 125 {
                    self.pop();
                    self.emit(T::StructStop, p, p + 1);
                } else if c == 110 && self.kw("namespace") {
                    self.push(S::Namespace);
                    self.emit(T::Namespace, p, p + 9);
                } else if c == 91 && self.starts("*]", p + 1) {
                    self.emit(T::EdgeState, p, p + 3);
                } else if c == 99 && self.kw("class") {
                    self.push(S::Cls);
                    self.emit(T::Class, p, p + 5);
                } else {
                    return self.common(c);
                }
            }
            S::Str => {
                if c == 34 {
                    self.pop();
                    self.p += 1;
                } else {
                    let end = self.until(34);
                    self.emit(T::Str, p, end);
                }
            }
            S::Generic => {
                if c == 34 {
                    self.push(S::Str);
                    self.p += 1;
                } else if self.keyword(c) {
                } else if c == 126 {
                    self.pop();
                    self.p += 1;
                } else {
                    let end = self.until(126);
                    self.emit(T::GenericType, p, end);
                }
            }
            S::Bq => {
                if c == 34 {
                    self.push(S::Str);
                    self.p += 1;
                } else if self.keyword(c) {
                } else if c == 126 {
                    self.push(S::Generic);
                    self.p += 1;
                } else if c == 96 {
                    self.pop();
                    self.p += 1;
                } else {
                    let end = self.until(96);
                    self.emit(T::BquoteStr, p, end);
                }
            }
            S::CallbackName => {
                if c == 40 {
                    let q = self.skip_ws(p + 1);
                    self.pop();
                    if self.at(q) == 41 {
                        self.p = q + 1;
                    } else {
                        self.push(S::CallbackArgs);
                        self.p += 1;
                    }
                } else {
                    let end = self.until(40);
                    self.emit(T::CallbackName, p, end);
                }
            }
            S::CallbackArgs => {
                if c == 41 {
                    self.pop();
                    self.p += 1;
                } else {
                    let end = self.until(41);
                    self.emit(T::CallbackArgs, p, end);
                }
            }
            S::AccTitle | S::AccDescr => {
                let kind = if self.state == S::AccTitle {
                    T::AccTitleValue
                } else {
                    T::AccDescrValue
                };
                let end = self.until(10);
                self.emit(kind, p, end);
                self.pop();
            }
            S::AccDescrMultiline => {
                if c == 125 {
                    self.pop();
                    self.p += 1;
                } else {
                    let end = self.until(125);
                    self.emit(T::AccDescrMultilineValue, p, end);
                }
            }
        }
        true
    }
}

fn tokenize(src: &[u16]) -> Vec<Tok> {
    let mut lexer = Lexer {
        src,
        p: 0,
        state: S::Initial,
        stack: Vec::new(),
        toks: Vec::new(),
        dir_next: index_of(src, "direction", 0),
        line_end: None,
        dir_failed: 0,
    };
    let n = src.len();
    loop {
        if lexer.p >= n {
            let kind = lexer.state.at_end();
            lexer.toks.push(Tok {
                kind,
                start: n,
                end: n,
            });
            break;
        }
        let c = u32::from(src[lexer.p]);
        if !lexer.step(c) {
            let p = lexer.p;
            lexer.toks.push(Tok {
                kind: T::Error,
                start: p,
                end: p,
            });
            return lexer.toks;
        }
    }
    lexer.toks.push(Tok {
        kind: T::End,
        start: n,
        end: n,
    });
    lexer.toks
}

type Edit = (usize, usize, usize);

fn drop_color_semicolons(text: Vec<u16>, keyword: &str, steps: &mut Vec<Vec<Edit>>) -> Vec<u16> {
    let mut edits = Vec::new();
    let length = keyword.len();
    let mut from = index_of(&text, keyword, 0);
    let mut out = Vec::new();
    let mut last = 0;
    while let Some(start) = from {
        let line_end = text[start..]
            .iter()
            .position(|&c| is_line_end(u32::from(c)))
            .map_or(text.len(), |k| start + k);
        let mut after_colon = false;
        let mut hash = None;
        for (j, &c) in text.iter().enumerate().take(line_end).skip(start + length) {
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
            && let Some(semi) = (hash + 1..line_end).rev().find(|&k| text[k] == 59)
        {
            edits.push((semi, semi + 1, 0));
            out.extend_from_slice(&text[last..semi]);
            last = semi + 1;
        }
        from = if line_end >= text.len() {
            None
        } else {
            index_of(&text, keyword, line_end + 1)
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

fn encode_entities(text: Vec<u16>) -> (Vec<u16>, Vec<Vec<Edit>>) {
    let mut steps = Vec::new();
    if !text.contains(&35) {
        return (text, steps);
    }
    let text = drop_color_semicolons(text, "style", &mut steps);
    let text = drop_color_semicolons(text, "classDef", &mut steps);
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
                let before = out.len();
                out.extend("ﬂ°".encode_utf16());
                if digits {
                    out.extend("°".encode_utf16());
                }
                out.extend_from_slice(inner);
                out.extend("¶ß".encode_utf16());
                edits.push((i, j + 1, out.len() - before));
                i = j + 1;
                continue;
            }
        }
        out.push(text[i]);
        i += 1;
    }
    steps.push(edits);
    (out, steps)
}

fn step_input(edits: &[Edit], at: usize) -> usize {
    let at = at as isize;
    let mut shift = 0_isize;
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

fn input_offset(steps: &[Vec<Edit>], mut at: usize) -> usize {
    for edits in steps.iter().rev() {
        at = step_input(edits, at);
    }
    at
}

fn line_end_len(bytes: &[u8], at: usize) -> usize {
    match bytes[at] {
        b'\n' | b'\r' => 1,
        0xe2 if bytes.get(at + 1) == Some(&0x80)
            && matches!(bytes.get(at + 2), Some(0xa8 | 0xa9)) =>
        {
            3
        }
        _ => 0,
    }
}

fn is_visibility(ch: u8) -> bool {
    matches!(ch, b'#' | b'+' | b'~' | b'-')
}

#[derive(Clone, Debug, Default)]
struct Member {
    id: String,
    method: bool,
    visibility: Option<char>,
    classifier: Option<char>,
    parameters: String,
    return_type: String,
}

impl Member {
    fn new(input: &str, method: bool) -> Self {
        let mut member = Self {
            method,
            ..Self::default()
        };
        if !method || !member.parse_method(input) {
            member.parse_attribute(input);
        }
        member.id = if member.id.starts_with(' ') {
            format!(" {}", js_trim(&member.id))
        } else {
            js_trim(&member.id).to_owned()
        };
        member
    }

    fn parse_method(&mut self, input: &str) -> bool {
        let b = input.as_bytes();
        let n = b.len();
        let mut start = 0;
        while start < n {
            let mut end = start;
            while end < n && line_end_len(b, end) == 0 {
                end += 1;
            }
            let lo = start as isize;
            let mut close = end as isize - 1;
            while close > lo && b[close as usize] != b')' {
                close -= 1;
            }
            let mut open = close - 1;
            while open > lo && b[open as usize] != b'(' {
                open -= 1;
            }
            if open > lo {
                let (open, close) = (open as usize, close as usize);
                let visible = is_visibility(b[start]) && open > start + 1;
                if visible {
                    self.visibility = Some(char::from(b[start]));
                }
                self.id = input[if visible { start + 1 } else { start }..open].to_owned();
                self.parameters = js_trim(&input[open + 1..close]).to_owned();
                let mut rest = close + 1;
                let mut classifier = None;
                if rest < n {
                    let next = input[rest..].chars().next().unwrap_or(' ');
                    if next == '$' || next == '*' || js_space(next) {
                        classifier = (!js_space(next)).then_some(next);
                        rest += next.len_utf8();
                    }
                }
                let mut stop = rest;
                while stop < n && line_end_len(b, stop) == 0 {
                    stop += 1;
                }
                self.return_type = js_trim(&input[rest..stop]).to_owned();
                if classifier.is_none()
                    && let Some(last) = self.return_type.chars().last()
                    && (last == '$' || last == '*')
                {
                    classifier = Some(last);
                    self.return_type.pop();
                }
                self.classifier = classifier;
                return true;
            }
            start = if end < n {
                end + line_end_len(b, end)
            } else {
                n
            };
        }
        self.method = false;
        false
    }

    fn parse_attribute(&mut self, input: &str) {
        let b = input.as_bytes();
        if let Some(&first) = b.first()
            && is_visibility(first)
        {
            self.visibility = Some(char::from(first));
        }
        if let Some(&last) = b.last()
            && (last == b'$' || last == b'*')
        {
            self.classifier = Some(char::from(last));
        }
        let from = usize::from(self.visibility.is_some());
        let to = b.len() - usize::from(self.classifier.is_some());
        self.id = input[from.min(to)..from.max(to)].to_owned();
    }

    fn display_text(&self) -> String {
        let mut out = String::new();
        if let Some(visibility) = self.visibility {
            out.push(visibility);
        }
        out.push_str(&parse_generic_types(&self.id));
        if self.method {
            out.push('(');
            out.push_str(&parse_generic_types(js_trim(&self.parameters)));
            out.push(')');
            if !self.return_type.is_empty() {
                out.push_str(" : ");
                out.push_str(&parse_generic_types(&self.return_type));
            }
        }
        js_trim(&out).to_owned()
    }
}

fn tildes(text: &str) -> usize {
    text.matches('~').count()
}

fn pair_tildes(text: &str) -> String {
    let count = tildes(text);
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
    let mut output: Vec<String> = Vec::new();
    let mut i = 0;
    while i < sets.len() {
        let mut set = sets[i].to_owned();
        if sets[i] == ","
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

#[derive(Clone, Debug)]
struct ClassNode {
    id: String,
    kind: String,
    label: String,
    css_classes: String,
    methods: Vec<Member>,
    members: Vec<Member>,
    annotations: Vec<String>,
    styles: Vec<Rc<str>>,
    parent: Option<String>,
    link: Option<String>,
    link_target: Option<String>,
    tooltip: Option<String>,
}

#[derive(Clone, Debug)]
struct Relation {
    id1: String,
    id2: String,
    title1: String,
    title2: String,
    title: Option<String>,
    type1: Option<u8>,
    type2: Option<u8>,
    dotted: bool,
}

#[derive(Clone, Debug)]
struct Note {
    id: String,
    class: Option<String>,
    text: String,
    index: usize,
    parent: Option<String>,
}

#[derive(Clone, Debug)]
struct Namespace {
    id: String,
    label: String,
    parent: Option<String>,
    explicit: bool,
}

#[derive(Clone, Debug)]
struct Interface {
    id: String,
    label: String,
    class_id: String,
}

#[derive(Debug, Default)]
struct Db {
    direction: String,
    classes: IndexMap<String, ClassNode>,
    relations: Vec<Relation>,
    notes: IndexMap<String, Note>,
    namespaces: IndexMap<String, Namespace>,
    interfaces: Vec<Interface>,
    style_classes: IndexMap<String, Vec<String>>,
    acc_title: Option<String>,
    acc_descr: Option<String>,
    namespace_stack: Vec<(String, usize)>,
    with_css_class: HashMap<String, IndexSet<usize>>,
    member_count: usize,
}

fn split_name(id: &str) -> (&str, &str) {
    match id.find('~') {
        Some(at) if at > 0 => {
            let rest = &id[at + 1..];
            let end = rest.find('~').unwrap_or(rest.len());
            (&id[..at], &rest[..end])
        }
        _ => (id, ""),
    }
}

fn lookup_id(id: &str) -> String {
    if id.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        split_name(&format!("classId-{id}")).0.to_owned()
    } else {
        split_name(id).0.to_owned()
    }
}

fn cleanup_label(label: &str) -> String {
    js_trim(label.strip_prefix(':').unwrap_or(label)).to_owned()
}

impl Db {
    fn add_class(&mut self, id: &str) -> usize {
        let (name, kind) = split_name(id);
        if let Some(at) = self.classes.get_index_of(name) {
            return at;
        }
        let at = self.classes.len();
        self.classes.insert(
            name.to_owned(),
            ClassNode {
                id: name.to_owned(),
                kind: kind.to_owned(),
                label: name.to_owned(),
                css_classes: "default".to_owned(),
                methods: Vec::new(),
                members: Vec::new(),
                annotations: Vec::new(),
                styles: Vec::new(),
                parent: None,
                link: None,
                link_target: None,
                tooltip: None,
            },
        );
        self.tag(at, "default");
        at
    }

    fn tag(&mut self, at: usize, css_class: &str) {
        self.with_css_class
            .entry(css_class.to_owned())
            .or_default()
            .insert(at);
    }

    fn set_class_label(&mut self, id: &str, label: String) {
        let at = self.add_class(id);
        self.classes[at].label = label;
    }

    fn add_relation(&mut self, mut relation: Relation) {
        if relation.type1 == Some(4) && relation.type2.is_none() {
            self.add_class(&relation.id2);
            relation.id1 = self.add_interface(&relation.id1, &relation.id2);
        } else if relation.type2 == Some(4) && relation.type1.is_none() {
            self.add_class(&relation.id1);
            relation.id2 = self.add_interface(&relation.id2, &relation.id1);
        } else {
            self.add_class(&relation.id1);
            self.add_class(&relation.id2);
        }
        relation.id1 = split_name(&relation.id1).0.to_owned();
        relation.id2 = split_name(&relation.id2).0.to_owned();
        relation.title1 = js_trim(&relation.title1).to_owned();
        relation.title2 = js_trim(&relation.title2).to_owned();
        self.relations.push(relation);
    }

    fn add_interface(&mut self, label: &str, class_id: &str) -> String {
        let id = format!("interface{}", self.interfaces.len());
        self.interfaces.push(Interface {
            id: id.clone(),
            label: label.to_owned(),
            class_id: class_id.to_owned(),
        });
        id
    }

    fn add_annotation(&mut self, class_name: &str, annotation: String) {
        let at = self.add_class(class_name);
        self.classes[at].annotations.push(annotation);
    }

    fn add_member(&mut self, class_name: &str, member: &str) {
        let at = self.add_class(class_name);
        let node = &mut self.classes[at];
        let text = js_trim(member);
        if text.starts_with("<<") && text.ends_with(">>") {
            node.annotations.push(text[2..text.len() - 2].to_owned());
        } else if text.find(')').is_some_and(|at| at > 0) {
            let parsed = Member::new(text, true);
            self.member_count += 1;
            if parsed.method {
                node.methods.push(parsed);
            } else {
                node.members.push(parsed);
            }
        } else if !text.is_empty() {
            self.member_count += 1;
            node.members.push(Member::new(text, false));
        }
    }

    fn add_note(&mut self, text: String, class: Option<String>) -> String {
        let index = self.notes.len();
        let id = format!("note{index}");
        self.notes.insert(
            id.clone(),
            Note {
                id: id.clone(),
                class,
                text,
                index,
                parent: None,
            },
        );
        id
    }

    fn set_css_class(&mut self, ids: &str, class_name: &str) {
        for id in ids.split(',') {
            let Some(at) = self.classes.get_index_of(lookup_id(id).as_str()) else {
                continue;
            };
            let node = &mut self.classes[at];
            node.css_classes.push(' ');
            node.css_classes.push_str(class_name);
            for word in class_name.split(' ') {
                self.tag(at, word);
            }
        }
    }

    fn define_class(&mut self, ids: &[String], style: &[String]) {
        for id in ids {
            let styles = self.style_classes.entry(id.clone()).or_default();
            styles.extend(style.iter().cloned());
            let Some(tagged) = self.with_css_class.get(id) else {
                continue;
            };
            let parts: Vec<Rc<str>> = style
                .iter()
                .flat_map(|s| s.split(','))
                .map(Rc::from)
                .collect();
            for &at in tagged {
                self.classes[at].styles.extend(parts.iter().cloned());
            }
        }
    }

    fn set_tooltip(&mut self, ids: &str, tooltip: Option<String>) {
        let Some(tooltip) = tooltip else {
            return;
        };
        for id in ids.split(',') {
            if let Some(node) = self.classes.get_mut(split_name(id).0) {
                node.tooltip = Some(tooltip.clone());
            }
        }
    }

    fn set_link(&mut self, ids: &str, link: &str, target: Option<String>) {
        let url = js_trim(&text::decode_entities(link)).to_owned();
        for id in ids.split(',') {
            if let Some(node) = self.classes.get_mut(lookup_id(id).as_str()) {
                node.link = (!url.is_empty()).then(|| sanitize_url(&url));
                node.link_target = Some(target.clone().unwrap_or_else(|| "_blank".to_owned()));
            }
        }
        self.set_css_class(ids, "clickable");
    }

    fn set_click_event(&mut self, ids: &str) {
        self.set_css_class(ids, "clickable");
    }

    fn add_namespace(&mut self, id: &str, label: Option<&str>) -> Result<String, PeleError> {
        let (prefix, depth) = self
            .namespace_stack
            .last()
            .map_or((None, 0), |(name, depth)| (Some(name.clone()), *depth));
        let depth = depth + 1 + id.matches('.').count();
        if depth > MAX_DEPTH {
            return Err(PeleError::limit(format!(
                "Class namespaces are nested more than {MAX_DEPTH} deep."
            )));
        }
        let qualified = match &prefix {
            Some(prefix) if !prefix.is_empty() => format!("{prefix}.{id}"),
            _ => id.to_owned(),
        };
        self.namespace_stack.push((qualified.clone(), depth));
        let label = label.filter(|label| !label.is_empty());
        if let Some(existing) = self.namespaces.get_mut(&qualified) {
            existing.explicit = true;
            if let Some(label) = label {
                existing.label = label.to_owned();
            }
            return Ok(qualified);
        }
        let prefix = prefix.filter(|prefix| !prefix.is_empty());
        let mut parent_id = prefix.clone();
        let mut from = prefix.as_ref().map_or(0, |prefix| prefix.len() + 1);
        loop {
            let dot = qualified[from..].find('.').map(|at| from + at);
            let leaf = dot.is_none();
            let end = dot.unwrap_or(qualified.len());
            let current = qualified[..end].to_owned();
            if let Some(node) = self.namespaces.get_mut(&current) {
                if leaf {
                    node.explicit = true;
                }
            } else {
                let part = &qualified[from..end];
                self.namespaces.insert(
                    current.clone(),
                    Namespace {
                        id: current.clone(),
                        label: if leaf {
                            label.unwrap_or(part).to_owned()
                        } else {
                            part.to_owned()
                        },
                        parent: parent_id.clone(),
                        explicit: leaf,
                    },
                );
            }
            if let Some(parent) = &parent_id
                && self.namespaces.contains_key(parent)
                && let Some(node) = self.namespaces.get_mut(&current)
                && node.parent.is_none()
            {
                node.parent = Some(parent.clone());
            }
            if leaf {
                return Ok(qualified);
            }
            parent_id = Some(current);
            from = end + 1;
        }
    }

    fn pop_namespace(&mut self) {
        self.namespace_stack.pop();
    }

    fn add_classes_to_namespace(&mut self, id: &str, classes: &[String], notes: &[String]) {
        if !self.namespaces.contains_key(id) {
            return;
        }
        for name in classes {
            if let Some(node) = self.classes.get_mut(split_name(name).0) {
                node.parent = Some(id.to_owned());
            }
        }
        for name in notes {
            if let Some(note) = self.notes.get_mut(name) {
                note.parent = Some(id.to_owned());
            }
        }
    }

    fn set_css_style(&mut self, id: &str, styles: &[String]) {
        if let Some(node) = self.classes.get_mut(id) {
            for s in styles {
                node.styles.extend(s.split(',').map(Rc::from));
            }
        }
    }
}

const NAME_TOKENS: &str = "'UNICODE_TEXT', 'NUM', 'ALPHA', 'MINUS', 'BQUOTE_STR'";

struct Parser<'a> {
    src: &'a [u16],
    original: &'a [u16],
    steps: &'a [Vec<Edit>],
    toks: Vec<Tok>,
    i: usize,
    db: Db,
}

impl Parser<'_> {
    fn t(&self) -> T {
        self.toks.get(self.i).map_or(T::End, |tok| tok.kind)
    }

    fn text(&self, at: usize) -> String {
        self.toks.get(at).map_or_else(String::new, |tok| {
            String::from_utf16_lossy(&self.src[tok.start..tok.end])
        })
    }

    fn take(&mut self) -> String {
        let text = self.text(self.i);
        self.i += 1;
        text
    }

    fn locate(&self, offset: usize) -> (usize, usize) {
        let mut end = self.src.len();
        while end > 0 && is_space(u32::from(self.src[end - 1])) {
            end -= 1;
        }
        let at = if end > 0 && offset >= end {
            input_offset(self.steps, end - 1) + 1
        } else {
            input_offset(self.steps, offset)
        };
        let at = at.min(self.original.len());
        let mut line = 1;
        let mut line_start = 0;
        for (k, &c) in self.original[..at].iter().enumerate() {
            if c == 10 {
                line += 1;
                line_start = k + 1;
            }
        }
        (line, at - line_start + 1)
    }

    fn fail(&self, expected: &str) -> PeleError {
        let tok = self.toks.get(self.i).copied().unwrap_or(Tok {
            kind: T::End,
            start: self.src.len(),
            end: self.src.len(),
        });
        let (line, column) = self.locate(tok.start);
        if tok.kind == T::Error {
            return PeleError::syntax("Lexical error: Unrecognized text.", line, column);
        }
        PeleError::syntax(
            format!(
                "Parse error: Expecting {expected}, got '{}'",
                TOKEN_NAMES[tok.kind as usize]
            ),
            line,
            column,
        )
    }

    fn expect(&mut self, kind: T) -> Result<String, PeleError> {
        if self.t() != kind {
            return Err(self.fail(&format!("'{}'", TOKEN_NAMES[kind as usize])));
        }
        Ok(self.take())
    }

    fn alpha_num(&mut self) -> Result<String, PeleError> {
        if !alphanum(self.t()) {
            return Err(self.fail("'UNICODE_TEXT', 'NUM', 'ALPHA', 'MINUS'"));
        }
        Ok(self.take())
    }

    fn name(&mut self, generic: bool) -> Result<String, PeleError> {
        let mut out = String::new();
        loop {
            if self.t() == T::BquoteStr {
                out.push_str(&self.take());
                break;
            }
            if !alphanum(self.t()) {
                return Err(self.fail(NAME_TOKENS));
            }
            out.push_str(&self.take());
            let t = self.t();
            if t == T::Dot {
                out.push('.');
                self.i += 1;
            } else if !alphanum(t) && t != T::BquoteStr {
                break;
            }
        }
        if generic && self.t() == T::GenericType {
            out.push('~');
            out.push_str(&self.take());
            out.push('~');
        }
        Ok(out)
    }

    fn class_label(&mut self) -> Result<String, PeleError> {
        self.i += 1;
        let label = self.expect(T::Str)?;
        self.expect(T::Sqe)?;
        Ok(label)
    }

    fn members(&mut self) -> Result<Vec<String>, PeleError> {
        if self.t() != T::Member {
            return Err(self.fail("'STRUCT_STOP', 'MEMBER'"));
        }
        let mut list = Vec::new();
        while self.t() == T::Member {
            list.push(self.take());
        }
        self.expect(T::StructStop)?;
        Ok(list)
    }

    fn add_members(&mut self, id: &str, members: &[String]) {
        for member in members {
            self.db.add_member(id, member);
        }
    }

    fn body(&mut self, id: &str) -> Result<(), PeleError> {
        if self.t() == T::StructStop {
            self.i += 1;
        } else {
            let members = self.members()?;
            self.add_members(id, &members);
        }
        Ok(())
    }

    fn class_statement(&mut self) -> Result<String, PeleError> {
        self.i += 1;
        let id = self.name(true)?;
        self.db.add_class(&id);
        if self.t() == T::Sqs {
            let label = self.class_label()?;
            self.db.set_class_label(&id, label);
        }
        match self.t() {
            T::StyleSeparator => {
                self.i += 1;
                let css = self.alpha_num()?;
                self.db.set_css_class(&id, &css);
                if self.t() == T::StructStart {
                    self.i += 1;
                    let members = self.members()?;
                    self.add_members(&id, &members);
                }
            }
            T::StructStart => {
                self.i += 1;
                self.body(&id)?;
            }
            T::AnnotationStart => {
                self.i += 1;
                let annotation = self.alpha_num()?;
                self.expect(T::AnnotationEnd)?;
                self.db.add_annotation(&id, annotation);
                if self.t() == T::StructStart {
                    self.i += 1;
                    self.body(&id)?;
                }
            }
            _ => {}
        }
        Ok(id)
    }

    fn note_statement(&mut self) -> Result<String, PeleError> {
        let kind = self.t();
        self.i += 1;
        if kind == T::Note {
            let text = self.expect(T::Str)?;
            return Ok(self.db.add_note(text, None));
        }
        let id = self.name(true)?;
        let text = self.expect(T::Str)?;
        Ok(self.db.add_note(text, Some(id)))
    }

    fn namespace_statement(&mut self) -> Result<(), PeleError> {
        let mut open: Vec<(String, Vec<String>, Vec<String>)> = Vec::new();
        loop {
            self.i += 1;
            let ns = self.name(false)?;
            let id = if self.t() == T::Sqs {
                let label = self.class_label()?;
                self.db.add_namespace(&ns, Some(&label))?
            } else {
                self.db.add_namespace(&ns, None)?
            };
            self.expect(T::StructStart)?;
            if self.t() == T::Newline {
                self.i += 1;
            }
            open.push((id, Vec::new(), Vec::new()));
            let mut item = true;
            while item {
                let t = self.t();
                if t == T::Namespace {
                    break;
                }
                if t == T::Class {
                    let id = self.class_statement()?;
                    open.last_mut().expect("open namespace").1.push(id);
                } else if t == T::Note || t == T::NoteFor {
                    let id = self.note_statement()?;
                    open.last_mut().expect("open namespace").2.push(id);
                } else {
                    return Err(self.fail("'NAMESPACE', 'CLASS', 'NOTE_FOR', 'NOTE'"));
                }
                loop {
                    if self.t() == T::Newline {
                        let next = self.toks.get(self.i + 1).map_or(T::End, |tok| tok.kind);
                        self.i += 1;
                        if matches!(next, T::Namespace | T::Class | T::Note | T::NoteFor) {
                            break;
                        }
                    }
                    self.expect(T::StructStop)?;
                    let (done, classes, notes) = open.pop().expect("open namespace");
                    self.db.add_classes_to_namespace(&done, &classes, &notes);
                    self.db.pop_namespace();
                    if open.is_empty() {
                        item = false;
                        break;
                    }
                }
            }
            if open.is_empty() {
                return Ok(());
            }
        }
    }

    fn relation_end(&mut self) -> Option<u8> {
        let kind = end_type(self.t())?;
        self.i += 1;
        Some(kind)
    }

    fn class_or_relation(&mut self) -> Result<(), PeleError> {
        let id1 = self.name(true)?;
        let mut t = self.t();
        if t == T::Label {
            let label = cleanup_label(&self.take());
            self.db.add_member(&id1, &label);
            return Ok(());
        }
        let mut title1 = "none".to_owned();
        if t == T::Str {
            title1 = self.take();
        } else if end_type(t).is_none() && t != T::Line && t != T::DottedLine {
            return Ok(());
        }
        let type1 = self.relation_end();
        t = self.t();
        if t != T::Line && t != T::DottedLine {
            return Err(self.fail(
                "'AGGREGATION', 'EXTENSION', 'COMPOSITION', 'DEPENDENCY', 'LOLLIPOP', 'LINE', 'DOTTED_LINE'",
            ));
        }
        self.i += 1;
        let type2 = self.relation_end();
        let title2 = if self.t() == T::Str {
            self.take()
        } else {
            "none".to_owned()
        };
        let id2 = self.name(true)?;
        let title = (self.t() == T::Label).then(|| cleanup_label(&self.take()));
        self.db.add_relation(Relation {
            id1,
            id2,
            title1,
            title2,
            title,
            type1,
            type2,
            dotted: t == T::DottedLine,
        });
        Ok(())
    }

    fn styles(&mut self) -> Result<Vec<String>, PeleError> {
        let mut out = Vec::new();
        loop {
            if !style_part(self.t()) {
                return Err(self.fail(
                    "'ALPHA', 'NUM', 'COLON', 'UNIT', 'SPACE', 'BRKT', 'STYLE', 'PCT', 'LABEL'",
                ));
            }
            let mut style = self.take();
            while style_part(self.t()) {
                style.push_str(&self.take());
            }
            out.push(style);
            if self.t() != T::Comma {
                return Ok(out);
            }
            self.i += 1;
        }
    }

    fn tooltip(&mut self, id: &str) {
        if self.t() == T::Str {
            let tip = self.take();
            self.db.set_tooltip(id, Some(tip));
        }
    }

    fn link_statement(&mut self, id: &str) -> Result<(), PeleError> {
        let href = self.expect(T::Str)?;
        let tip = (self.t() == T::Str).then(|| self.take());
        let target = (self.t() == T::LinkTarget).then(|| self.take());
        self.db.set_link(id, &href, target);
        self.db.set_tooltip(id, tip);
        Ok(())
    }

    fn statement(&mut self) -> Result<(), PeleError> {
        let t = self.t();
        match t {
            T::UnicodeText | T::Alpha | T::Minus | T::BquoteStr => self.class_or_relation(),
            T::Namespace => self.namespace_statement(),
            T::Class => self.class_statement().map(drop),
            T::Member => {
                self.i += 1;
                Ok(())
            }
            T::AnnotationStart => {
                self.i += 1;
                let annotation = self.alpha_num()?;
                self.expect(T::AnnotationEnd)?;
                let id = self.name(true)?;
                self.db.add_annotation(&id, annotation);
                Ok(())
            }
            T::Callback => {
                self.i += 1;
                let id = self.name(true)?;
                self.expect(T::Str)?;
                self.db.set_click_event(&id);
                self.tooltip(&id);
                Ok(())
            }
            T::Link => {
                self.i += 1;
                let id = self.name(true)?;
                self.link_statement(&id)
            }
            T::Click => {
                self.i += 1;
                let id = self.name(true)?;
                if self.t() == T::Href {
                    self.i += 1;
                    return self.link_statement(&id);
                }
                self.expect(T::CallbackName)?;
                if self.t() == T::CallbackArgs {
                    self.i += 1;
                }
                self.db.set_click_event(&id);
                self.tooltip(&id);
                Ok(())
            }
            T::Style => {
                self.i += 1;
                let id = self.expect(T::Alpha)?;
                let styles = self.styles()?;
                self.db.set_css_style(&id, &styles);
                Ok(())
            }
            T::CssClass => {
                self.i += 1;
                let ids = self.expect(T::Str)?;
                let class = self.expect(T::Alpha)?;
                self.db.set_css_class(&ids, &class);
                Ok(())
            }
            T::Note | T::NoteFor => self.note_statement().map(drop),
            T::ClassDef => {
                self.i += 1;
                let mut ids = vec![self.expect(T::Alpha)?];
                while self.t() == T::Comma {
                    self.i += 1;
                    ids.push(self.expect(T::Alpha)?);
                }
                let styles = self.styles()?;
                self.db.define_class(&ids, &styles);
                Ok(())
            }
            T::DirectionTb | T::DirectionBt | T::DirectionRl | T::DirectionLr => {
                self.i += 1;
                self.db.direction = match t {
                    T::DirectionTb => "TB",
                    T::DirectionBt => "BT",
                    T::DirectionRl => "RL",
                    _ => "LR",
                }
                .to_owned();
                Ok(())
            }
            T::AccTitle => {
                self.i += 1;
                let value = self.expect(T::AccTitleValue)?;
                self.db.acc_title = Some(js_trim(&value).to_owned());
                Ok(())
            }
            T::AccDescr => {
                self.i += 1;
                let value = self.expect(T::AccDescrValue)?;
                self.db.acc_descr = Some(acc_description(js_trim(&value)));
                Ok(())
            }
            T::AccDescrMultilineValue => {
                let value = self.take();
                self.db.acc_descr = Some(acc_description(js_trim(&value)));
                Ok(())
            }
            _ => Err(self.fail(
                "'acc_title', 'acc_descr', 'acc_descr_multiline_value', 'NAMESPACE', 'ANNOTATION_START', 'CLASS', 'MEMBER', 'NOTE_FOR', 'NOTE', 'CLASSDEF', 'direction_tb', 'CALLBACK', 'LINK', 'CLICK', 'STYLE', 'CSSCLASS', 'ALPHA', 'MINUS', 'BQUOTE_STR'",
            )),
        }
    }

    fn parse(&mut self) -> Result<(), PeleError> {
        let header = self.t() == T::ClassDiagram;
        if header {
            self.i += 1;
            self.expect(T::Newline)?;
        }
        loop {
            self.statement()?;
            if self.t() != T::Newline {
                break;
            }
            self.i += 1;
            if matches!(self.t(), T::Eof | T::End) {
                break;
            }
        }
        if header {
            self.expect(T::Eof)?;
        }
        self.expect(T::End)?;
        Ok(())
    }
}

fn acc_description(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        out.push(ch);
        if ch == '\n' && chars.peek().is_some_and(|next| js_space(*next)) {
            while chars.peek().is_some_and(|next| js_space(*next)) {
                chars.next();
            }
        }
    }
    out
}

fn parse(source: &str) -> Result<Db, PeleError> {
    let original: Vec<u16> = format!("{source}\n").encode_utf16().collect();
    let (src, steps) = encode_entities(original.clone());
    let toks = tokenize(&src);
    let mut parser = Parser {
        src: &src,
        original: &original,
        steps: &steps,
        toks,
        i: 0,
        db: Db {
            direction: "TB".to_owned(),
            ..Db::default()
        },
    };
    parser.parse()?;
    Ok(parser.db)
}

fn is_ctrl(ch: char) -> bool {
    matches!(ch, '\u{0}'..='\u{1f}' | '\u{7f}'..='\u{9f}' | '\u{2000}'..='\u{200d}' | '\u{feff}' | '\u{fffe}' | '\u{ffff}')
}

fn decode_uri(uri: &str) -> Option<String> {
    let bytes = uri.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let digit = |at: usize| bytes.get(at).and_then(|b| char::from(*b).to_digit(16));
            let (Some(high), Some(low)) = (digit(i + 1), digit(i + 2)) else {
                return None;
            };
            out.push((high * 16 + low) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn js_char_code(digits: &str) -> u16 {
    let lower = digits.to_ascii_lowercase();
    let radix = |text: &str, base: u32| {
        if text.is_empty() {
            return f64::NAN;
        }
        text.chars()
            .try_fold(0.0, |acc, ch| {
                ch.to_digit(base)
                    .map(|d| acc * f64::from(base) + f64::from(d))
            })
            .unwrap_or(f64::NAN)
    };
    let value = if let Some(hex) = lower.strip_prefix("0x") {
        radix(hex, 16)
    } else if let Some(octal) = lower.strip_prefix("0o") {
        radix(octal, 8)
    } else if let Some(binary) = lower.strip_prefix("0b") {
        radix(binary, 2)
    } else if digits == "Infinity" {
        f64::INFINITY
    } else {
        let (mantissa, exponent) = lower.split_once('e').unwrap_or((&lower, ""));
        let numeric = |text: &str| !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit());
        if numeric(mantissa) && (exponent.is_empty() && !lower.contains('e') || numeric(exponent)) {
            lower.parse().unwrap_or(f64::NAN)
        } else {
            f64::NAN
        }
    };
    if !value.is_finite() {
        return 0;
    }
    value.trunc().rem_euclid(65536.0) as u16
}

fn replace_html_entities(text: &str) -> String {
    let mut units: Vec<u16> = Vec::with_capacity(text.len());
    let mut last = 0;
    for capture in RE_HTML_ENTITIES.captures_iter(text) {
        let whole = capture.get(0).expect("match");
        units.extend(text[last..whole.start()].encode_utf16());
        units.push(js_char_code(&capture[1]));
        last = whole.end();
    }
    units.extend(text[last..].encode_utf16());
    char::decode_utf16(units).filter_map(Result::ok).collect()
}

fn unwrap_url(url: &str, percent: bool) -> Option<String> {
    let mut out = js_trim(url).to_owned();
    if percent {
        out = decode_uri(&out).unwrap_or(out);
    }
    for _ in 0..32 {
        out.retain(|ch| !is_ctrl(ch));
        out = replace_html_entities(&out);
        out = RE_HTML_CTRL_ENTITY.replace_all(&out, "").into_owned();
        out.retain(|ch| !is_ctrl(ch));
        out = RE_WS_ESCAPE.replace_all(&out, "").into_owned();
        out = js_trim(&out).to_owned();
        if percent {
            out = decode_uri(&out).unwrap_or(out);
        }
        let more = out.chars().any(is_ctrl)
            || RE_HTML_ENTITIES.is_match(&out)
            || RE_HTML_CTRL_ENTITY.is_match(&out)
            || RE_WS_ESCAPE.is_match(&out);
        if !more {
            return Some(out);
        }
    }
    None
}

fn scheme_prefix(url: &str) -> Option<&str> {
    let mut line_start = 0;
    loop {
        let line_end = url[line_start..]
            .find(['\n', '\r', '\u{2028}', '\u{2029}'])
            .map_or(url.len(), |k| line_start + k);
        let line = &url[line_start..line_end];
        let found = line
            .char_indices()
            .rev()
            .filter(|(at, _)| *at > 0)
            .find_map(|(at, _)| {
                let rest = &line[at..];
                if rest.starts_with(':') {
                    Some(at + 1)
                } else if rest.starts_with("&colon;") {
                    Some(at + 7)
                } else {
                    None
                }
            });
        if let Some(end) = found {
            return Some(&url[line_start..line_start + end]);
        }
        if line_end >= url.len() {
            return None;
        }
        line_start = line_end + url[line_end..].chars().next().map_or(1, char::len_utf8);
    }
}

fn percent_encode(text: &str, set: &[u8]) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_ascii() && (' '..='~').contains(&ch) && ch != ' ' && !set.contains(&(ch as u8)) {
            out.push(ch);
        } else {
            let mut buffer = [0; 4];
            for byte in ch.encode_utf8(&mut buffer).bytes() {
                out.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    out
}

const FRAGMENT_SET: &[u8] = b"\"<>`";
const QUERY_SET: &[u8] = b"\"#<>'";
const PATH_SET: &[u8] = b"\"#<>?`{}";
const USERINFO_SET: &[u8] = b"\"#<>?`{}/:;=@[\\]^|";

fn normalize_path(path: &str) -> String {
    let parts: Vec<&str> = path[1..].split('/').collect();
    let mut out: Vec<&str> = Vec::new();
    for (k, segment) in parts.iter().enumerate() {
        let last = k + 1 == parts.len();
        match *segment {
            "." => {
                if last {
                    out.push("");
                }
            }
            ".." => {
                out.pop();
                if last {
                    out.push("");
                }
            }
            other => out.push(other),
        }
    }
    format!("/{}", out.join("/"))
}

fn normalize_http(url: &str) -> Option<String> {
    let colon = url.find(':')?;
    let scheme = url[..colon].to_ascii_lowercase();
    let default_port = if scheme == "https" { 443 } else { 80 };
    let rest = url[colon + 1..].trim_start_matches('/');
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    let (userinfo, host_port) = match authority.rfind('@') {
        Some(at) => (Some(&authority[..at]), &authority[at + 1..]),
        None => (None, authority),
    };
    let (host, port) = if host_port.starts_with('[') {
        let close = host_port.find(']')?;
        (
            &host_port[..=close],
            host_port[close + 1..].strip_prefix(':'),
        )
    } else {
        match host_port.rfind(':') {
            Some(at) => (&host_port[..at], Some(&host_port[at + 1..])),
            None => (host_port, None),
        }
    };
    if host.is_empty()
        || host
            .chars()
            .any(|ch| ch.is_ascii_control() || " <>^|%\\".contains(ch))
    {
        return None;
    }
    let mut out = format!("{scheme}://");
    if let Some(info) = userinfo.filter(|info| !info.is_empty()) {
        let (user, password) = info.split_once(':').unwrap_or((info, ""));
        out.push_str(&percent_encode(user, USERINFO_SET));
        if !password.is_empty() {
            out.push(':');
            out.push_str(&percent_encode(password, USERINFO_SET));
        }
        out.push('@');
    }
    out.push_str(&host.to_lowercase());
    if let Some(port) = port.filter(|port| !port.is_empty()) {
        if !port.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let value: u32 = port.parse().ok().filter(|value| *value <= 65535)?;
        if value != default_port {
            out.push_str(&format!(":{value}"));
        }
    }
    let (before, fragment) = match tail.split_once('#') {
        Some((before, fragment)) => (before, Some(fragment)),
        None => (tail, None),
    };
    let (path, query) = match before.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (before, None),
    };
    let path = if path.is_empty() {
        "/".to_owned()
    } else {
        normalize_path(path)
    };
    out.push_str(&percent_encode(&path, PATH_SET));
    if let Some(query) = query {
        out.push('?');
        out.push_str(&percent_encode(query, QUERY_SET));
    }
    if let Some(fragment) = fragment {
        out.push('#');
        out.push_str(&percent_encode(fragment, FRAGMENT_SET));
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
    let Some(scheme) = scheme_prefix(decoded.trim_start_matches(js_space)) else {
        return kept;
    };
    let protocol = js_trim(&scheme.to_lowercase()).to_owned();
    if RE_INVALID_PROTOCOL.is_match(&protocol) {
        return BLANK.to_owned();
    }
    let normalized = kept.trim_start_matches(js_space).replace('\\', "/");
    if protocol == "mailto:" || protocol.contains("://") {
        return normalized;
    }
    if protocol == "http:" || protocol == "https:" {
        return normalize_http(&normalized).unwrap_or_else(|| BLANK.to_owned());
    }
    normalized
}

fn allowed(url: &str) -> bool {
    let mut start = url.chars();
    if matches!(start.next(), Some('/' | '\\')) && matches!(start.next(), Some('/' | '\\')) {
        return false;
    }
    RE_SCHEME_NAME
        .captures(url)
        .is_none_or(|scheme| LINK_SCHEMES.contains(&scheme[1].to_ascii_lowercase().as_str()))
}

fn link_url(url: &str) -> Option<String> {
    let decoded = unwrap_url(url, true)?;
    (allowed(url) && allowed(&decoded)).then(|| url.to_owned())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    Rect,
    ClassBox,
    Note,
    Interface,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EndMarker {
    Aggregation,
    Extension,
    Composition,
    Dependency,
    Lollipop,
    None,
}

impl EndMarker {
    fn of(kind: Option<u8>) -> Self {
        match kind {
            Some(0) => Self::Aggregation,
            Some(1) => Self::Extension,
            Some(2) => Self::Composition,
            Some(3) => Self::Dependency,
            Some(4) => Self::Lollipop,
            _ => Self::None,
        }
    }

    fn trim(self) -> f64 {
        match self {
            Self::Extension => TRIANGLE,
            Self::Composition | Self::Aggregation => DIAMOND,
            Self::Lollipop => 2.0 * RING,
            Self::Dependency => marker_trim("arrow_point"),
            Self::None => 0.0,
        }
    }

    fn draw(self, x: f64, y: f64, dx: f64, dy: f64, color: &str) -> String {
        let n = svg::num;
        let nx = -dy;
        let ny = dx;
        match self {
            Self::Extension => {
                let bx = x - dx * TRIANGLE;
                let by = y - dy * TRIANGLE;
                format!(
                    "<path class=\"pele-marker\" d=\"M{},{}L{},{}L{},{}Z\" stroke-linejoin=\"round\"/>",
                    n(x),
                    n(y),
                    n(bx + nx * HALF),
                    n(by + ny * HALF),
                    n(bx - nx * HALF),
                    n(by - ny * HALF)
                )
            }
            Self::Composition | Self::Aggregation => {
                let mx = x - (dx * DIAMOND) / 2.0;
                let my = y - (dy * DIAMOND) / 2.0;
                let half = HALF - 1.0;
                format!(
                    "<path class=\"pele-marker\" d=\"M{},{}L{},{}L{},{}L{},{}Z\" stroke-linejoin=\"round\"{}/>",
                    n(x),
                    n(y),
                    n(mx + nx * half),
                    n(my + ny * half),
                    n(x - dx * DIAMOND),
                    n(y - dy * DIAMOND),
                    n(mx - nx * half),
                    n(my - ny * half),
                    if self == Self::Composition {
                        format!(" fill=\"{color}\"")
                    } else {
                        String::new()
                    }
                )
            }
            Self::Lollipop => format!(
                "<circle class=\"pele-marker\" cx=\"{}\" cy=\"{}\" r=\"{}\"/>",
                n(x - dx * RING),
                n(y - dy * RING),
                n(RING)
            ),
            Self::Dependency => marker("arrow_point", x, y, dx, dy, color, "none"),
            Self::None => String::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pattern {
    Solid,
    Dashed,
    Dotted,
}

struct GraphNode<'a> {
    id: &'a str,
    label: &'a str,
    is_group: bool,
    shape: Shape,
    parent_id: Option<&'a str>,
    parent: Option<usize>,
    class: Option<&'a ClassNode>,
}

struct GraphEdge<'a> {
    id: String,
    source: Option<usize>,
    target: Option<usize>,
    label: Option<&'a str>,
    start: EndMarker,
    end: EndMarker,
    start_label: &'a str,
    end_label: &'a str,
    pattern: Pattern,
}

fn declared_ancestor<'a>(
    db: &'a Db,
    declared: &mut HashMap<&'a str, Option<&'a str>>,
    id: Option<&'a str>,
    hierarchical: bool,
) -> Option<&'a str> {
    if hierarchical {
        return id;
    }
    let mut chain = Vec::new();
    let mut current = id.filter(|id| !id.is_empty());
    let mut found = None;
    while let Some(at) = current {
        if let Some(&known) = declared.get(at) {
            found = known;
            break;
        }
        let Some(ns) = db.namespaces.get(at) else {
            break;
        };
        if ns.explicit {
            found = Some(ns.id.as_str());
            break;
        }
        chain.push(ns.id.as_str());
        current = ns.parent.as_deref().filter(|id| !id.is_empty());
    }
    for visited in chain {
        declared.insert(visited, found);
    }
    found
}

fn shown(title: &str) -> &str {
    if title == "none" { "" } else { title }
}

fn build_graph(db: &Db, hierarchical: bool) -> (Vec<GraphNode<'_>>, Vec<GraphEdge<'_>>) {
    let mut nodes: Vec<GraphNode<'_>> = Vec::new();
    let mut edges = Vec::new();
    let mut namespace_at = HashMap::new();
    let mut class_at: HashMap<&str, usize> = HashMap::new();
    let mut interface_at: HashMap<&str, usize> = HashMap::new();
    let mut declared = HashMap::new();
    for ns in db.namespaces.values() {
        if !hierarchical && !ns.explicit {
            continue;
        }
        namespace_at.insert(ns.id.as_str(), nodes.len());
        nodes.push(GraphNode {
            id: &ns.id,
            label: if hierarchical { &ns.label } else { &ns.id },
            is_group: true,
            shape: Shape::Rect,
            parent_id: if hierarchical {
                ns.parent.as_deref()
            } else {
                None
            },
            parent: None,
            class: None,
        });
    }
    let group = |namespace_at: &HashMap<&str, usize>, id: Option<&str>| {
        id.and_then(|id| namespace_at.get(id).copied())
    };
    for node in &mut nodes {
        node.parent = group(&namespace_at, node.parent_id);
    }
    for class in db.classes.values() {
        let parent_id = declared_ancestor(db, &mut declared, class.parent.as_deref(), hierarchical);
        class_at.insert(&class.id, nodes.len());
        nodes.push(GraphNode {
            id: &class.id,
            label: &class.label,
            is_group: false,
            shape: Shape::ClassBox,
            parent_id,
            parent: group(&namespace_at, parent_id),
            class: Some(class),
        });
    }
    for note in db.notes.values() {
        let parent_id = declared_ancestor(db, &mut declared, note.parent.as_deref(), hierarchical);
        if let Some(&target) = note.class.as_deref().and_then(|class| class_at.get(class)) {
            edges.push(GraphEdge {
                id: format!("edgeNote{}", note.index),
                source: Some(nodes.len()),
                target: Some(target),
                label: None,
                start: EndMarker::None,
                end: EndMarker::None,
                start_label: "",
                end_label: "",
                pattern: Pattern::Dotted,
            });
        }
        nodes.push(GraphNode {
            id: &note.id,
            label: &note.text,
            is_group: false,
            shape: Shape::Note,
            parent_id,
            parent: group(&namespace_at, parent_id),
            class: None,
        });
    }
    for item in &db.interfaces {
        let owner = class_at.get(item.class_id.as_str()).copied();
        interface_at.insert(&item.id, nodes.len());
        let parent = owner.and_then(|owner| nodes[owner].parent);
        nodes.push(GraphNode {
            id: &item.id,
            label: &item.label,
            is_group: false,
            shape: Shape::Interface,
            parent_id: None,
            parent,
            class: None,
        });
    }
    for (count, relation) in db.relations.iter().enumerate() {
        let (type1, type2) = (relation.type1, relation.type2);
        let start_interface = type1 == Some(4) && type2.is_none();
        let end_interface = !start_interface && type2 == Some(4) && type1.is_none();
        let start_at = if start_interface {
            &interface_at
        } else {
            &class_at
        };
        let end_at = if end_interface {
            &interface_at
        } else {
            &class_at
        };
        edges.push(GraphEdge {
            id: format!("id_{}_{}_{}", relation.id1, relation.id2, count + 1),
            source: start_at.get(relation.id1.as_str()).copied(),
            target: end_at.get(relation.id2.as_str()).copied(),
            label: relation.title.as_deref(),
            start: EndMarker::of(type1),
            end: EndMarker::of(type2),
            start_label: shown(&relation.title1),
            end_label: shown(&relation.title2),
            pattern: if relation.dotted {
                Pattern::Dashed
            } else {
                Pattern::Solid
            },
        });
    }
    (nodes, edges)
}

pub fn render(source: &str, options: &RenderOptions) -> Result<RenderResult, PeleError> {
    let db = parse(source)?;
    if db.relations.len() > options.max_edges {
        return Err(PeleError::limit(format!(
            "Edge limit exceeded. The limit is {}.",
            options.max_edges
        )));
    }
    if db.classes.len() + db.notes.len() + db.interfaces.len() + db.namespaces.len() > MAX_NODES {
        return Err(PeleError::limit(format!(
            "Class diagrams are limited to {MAX_NODES} classes, notes and namespaces."
        )));
    }
    if db.member_count > MAX_MEMBERS {
        return Err(PeleError::limit(format!(
            "Class diagrams are limited to {MAX_MEMBERS} members."
        )));
    }
    let fitted = |down: bool| tighten(options, |tight| draw(&db, options, down, tight));
    if runs_across(&db.direction) {
        turn_to_fit(options, fitted)
    } else {
        fitted(false)
    }
}

struct Line {
    text: String,
    width: f64,
    classifier: Option<char>,
}

struct View {
    style: ResolvedStyle,
    label: text::Label,
    w: f64,
    h: f64,
    annotations: Vec<String>,
    attributes: Vec<Line>,
    methods: Vec<Line>,
    title_h: f64,
    attributes_h: f64,
    font_size: f64,
}

impl View {
    fn plain(label: text::Label, w: f64, h: f64, font_size: f64) -> Self {
        Self {
            style: ResolvedStyle::default(),
            label,
            w,
            h,
            annotations: Vec::new(),
            attributes: Vec::new(),
            methods: Vec::new(),
            title_h: 0.0,
            attributes_h: 0.0,
            font_size,
        }
    }
}

#[derive(Default)]
struct Styler {
    effect: HashMap<String, String>,
    from_defs: HashMap<String, Vec<(String, String)>>,
}

fn keep(effect: &mut HashMap<String, String>, kept: &mut IndexMap<String, String>, decl: &str) {
    let prop = if let Some(prop) = effect.get(decl) {
        prop.clone()
    } else {
        let one = resolve_style(&[decl]);
        let prop = if one.shape.is_empty() && one.text.is_empty() {
            String::new()
        } else {
            decl[..decl.find(':').unwrap_or(0)].trim().to_lowercase()
        };
        effect.insert(decl.to_owned(), prop.clone());
        prop
    };
    if !prop.is_empty() {
        kept.insert(prop, decl.to_owned());
    }
}

impl Styler {
    fn style(&mut self, db: &Db, class: &ClassNode) -> ResolvedStyle {
        let mut kept = IndexMap::new();
        if let Some(base) = self.from_defs.get(&class.css_classes) {
            for (prop, decl) in base {
                kept.insert(prop.clone(), decl.clone());
            }
        } else {
            for name in class.css_classes.split(' ') {
                if let Some(def) = db.style_classes.get(name) {
                    for style in def {
                        for part in style.split(',') {
                            keep(&mut self.effect, &mut kept, part);
                        }
                    }
                }
            }
            let base = kept.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            self.from_defs.insert(class.css_classes.clone(), base);
        }
        for style in &class.styles {
            keep(&mut self.effect, &mut kept, style);
        }
        let declarations: Vec<&str> = kept.values().map(String::as_str).collect();
        resolve_style(&declarations)
    }
}

fn member_line(member: &Member, options: &RenderOptions, size: f64) -> Line {
    let text = js_trim(&text::decode_entities(&member.display_text())).to_owned();
    let style = if member.classifier == Some('*') {
        ITALIC
    } else {
        0
    };
    Line {
        width: options.measure(&text, size, style),
        text,
        classifier: member.classifier,
    }
}

#[derive(Clone, Copy, Debug)]
struct End {
    edge: usize,
    at: usize,
    toward: f64,
    order: usize,
    room: f64,
    side: usize,
    after: bool,
}

struct Card {
    text: String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    parent: usize,
}

fn draw(
    db: &Db,
    options: &RenderOptions,
    turned: bool,
    tight: bool,
) -> Result<RenderResult, PeleError> {
    let n = svg::num;
    let theme = &options.theme;
    let conf = &options.config["class"];
    let (graph_nodes, graph_edges) = build_graph(db, conf["hierarchicalNamespaces"] != false);
    let hide_empty = conf["hideEmptyMembersBox"] == true;
    let size = options.font_size;
    let small = js_round(size * 0.875);
    let tiny = js_round(size * 0.8);
    let tiny_height = js_round(tiny * 1.4);
    let measurer = options.text_measurer();
    let pad = options.padding;
    let number_option = |key: &str, fallback: f64| {
        conf[key]
            .as_f64()
            .filter(|value| *value > 0.0)
            .map_or(fallback, |value| value.min(2000.0))
    };
    let card_width = |raw: &str| {
        if raw.is_empty() {
            0.0
        } else {
            options.measure(&text::decode_entities(raw), tiny, 0)
        }
    };
    let root_dir = if turned {
        Dir::Tb
    } else {
        direction(&db.direction)
    };
    let mut styler = Styler::default();

    let count = graph_nodes.len();
    let mut children = vec![0_usize; count];
    for node in &graph_nodes {
        if let Some(parent) = node.parent {
            children[parent] += 1;
        }
    }
    let mut crossed = vec![false; count];
    for edge in &graph_edges {
        let (Some(source), Some(target)) = (edge.source, edge.target) else {
            continue;
        };
        let a = graph_nodes[source].parent;
        let b = graph_nodes[target].parent;
        if a == b {
            continue;
        }
        for start in [a, b] {
            let mut g = start;
            while let Some(at) = g.filter(|&at| !crossed[at]) {
                crossed[at] = true;
                g = graph_nodes[at].parent;
            }
        }
    }

    let mut views = Vec::with_capacity(count);
    let mut cnodes: Vec<CNode> = Vec::with_capacity(count);
    for (i, node) in graph_nodes.iter().enumerate() {
        let (view, mut c) = if node.is_group {
            let label = text::layout_label(node.label, false, measurer, size, 4000.0, 0);
            if children[i] > 0 {
                let mut c = cnode(0.0, 0.0, None);
                c.is_group = true;
                c.pad_x = GROUP_PAD;
                c.pad_top = if label.height > 0.0 {
                    label.height + 16.0
                } else {
                    GROUP_PAD
                };
                c.pad_bottom = GROUP_PAD;
                c.min_w = label.width + 2.0 * GROUP_PAD - 12.0
                    + if crossed[i] && !tight {
                        (label.width + 24.0).min(240.0)
                    } else {
                        0.0
                    };
                (View::plain(label, 0.0, 0.0, size), c)
            } else {
                let w = (label.width + 2.0 * GROUP_PAD).max(80.0);
                let h = label.height + 2.0 * GROUP_PAD;
                (View::plain(label, w, h, size), cnode(w, h, None))
            }
        } else if let Some(class) = node.class {
            let view = class_view(db, class, &mut styler, options, hide_empty);
            let c = cnode(view.w, view.h, None);
            (view, c)
        } else if node.shape == Shape::Note {
            let label = text::layout_label(node.label, true, measurer, small, 260.0, 0);
            let w = (label.width + 2.0 * PAD_X).max(40.0);
            let h = label.height + 2.0 * TITLE_PAD;
            (View::plain(label, w, h, small), cnode(w, h, None))
        } else {
            let label = text::layout_label(node.label, false, measurer, small, 200.0, 0);
            let w = label.width.max(12.0) + 4.0;
            let h = label.height.max(12.0);
            (View::plain(label, w, h, small), cnode(w, h, None))
        };
        c.parent = node.parent;
        c.seq = i;
        views.push(view);
        cnodes.push(c);
    }

    let mut cedges: Vec<CEdge> = Vec::new();
    let mut drawn: Vec<&GraphEdge<'_>> = Vec::new();
    let mut edge_labels = Vec::new();
    let mut start_room = Vec::new();
    let mut end_room = Vec::new();
    let mut crowd = vec![0.0_f64; count * 4];
    let meet = |crowd: &mut Vec<f64>, item: usize, slot: usize, room: f64| {
        crowd[item * 4 + slot] += 1.0;
        if room > crowd[item * 4 + slot + 1] {
            crowd[item * 4 + slot + 1] = room;
        }
    };
    for edge in &graph_edges {
        let (Some(source), Some(target)) = (edge.source, edge.target) else {
            continue;
        };
        let label = edge.label.map_or_else(text::Label::default, |raw| {
            text::layout_label(raw, true, measurer, small, 200.0, 0)
        });
        let start_w = card_width(edge.start_label);
        let end_w = card_width(edge.end_label);
        if source == target {
            cnodes[source].w += 2.0
                * (LOOP
                    + if label.width > 0.0 {
                        label.width + 12.0
                    } else {
                        0.0
                    });
        } else {
            meet(&mut crowd, source, 0, start_w);
            meet(&mut crowd, target, 2, end_w);
        }
        start_room.push(start_w);
        end_room.push(end_w);
        drawn.push(edge);
        cedges.push(CEdge {
            src: source,
            dst: target,
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

    if root_dir.vertical() {
        for i in 0..views.len() {
            if graph_nodes[i].class.is_none() {
                continue;
            }
            let mut need = 0.0_f64;
            for slot in [0, 2] {
                let crowded = crowd[i * 4 + slot];
                let room = crowd[i * 4 + slot + 1];
                if crowded > 1.0 {
                    need = need
                        .max((crowded - 1.0) * if room > 0.0 { room + 24.0 } else { 14.0 } + 24.0);
                }
            }
            need = need.min(480.0);
            if need > views[i].w {
                cnodes[i].w += need - views[i].w;
                views[i].w = need;
            }
        }
    }

    let lettered = start_room
        .iter()
        .zip(&end_room)
        .any(|(start, end)| *start > 0.0 || *end > 0.0);
    let layout_options = LayeredOptions {
        node_sep: number_option("nodeSpacing", NODE_SEP),
        edge_sep: EDGE_SEP,
        rank_sep: number_option(
            "rankSpacing",
            if lettered { RANK_SEP + 16.0 } else { RANK_SEP },
        ),
        port_sep: 20.0,
        apart: true,
        ..LayeredOptions::default()
    };
    let layout = compound_layout(&mut cnodes, &mut cedges, root_dir, &layout_options);

    let mut sides: IndexMap<usize, Vec<End>> = IndexMap::new();
    let mut attach = |cedges: &mut [CEdge],
                      edge: usize,
                      at: usize,
                      item: usize,
                      room: f64,
                      order: usize|
     -> Option<End> {
        let c = &cnodes[item];
        let route = &mut cedges[edge].route;
        let side = end_side(route, at, c)?;
        let slack = (c.w - views[item].w) / 2.0;
        if side == 1 {
            route[at] -= slack;
        } else if side == 3 {
            route[at] += slack;
        }
        let across = side & 1;
        let toward = route[if at == 0 { 3 } else { at - 3 } + across];
        let bend = toward - route[at + across];
        let end = End {
            edge,
            at,
            toward,
            order,
            room,
            side,
            after: if across == 1 { bend > 0.0 } else { bend >= 0.0 },
        };
        sides.entry(item * 4 + side).or_default().push(end);
        Some(end)
    };
    let mut starts: Vec<Option<End>> = Vec::with_capacity(cedges.len());
    let mut ends: Vec<Option<End>> = Vec::with_capacity(cedges.len());
    for i in 0..cedges.len() {
        let (src, dst, length) = (cedges[i].src, cedges[i].dst, cedges[i].route.len());
        let routed = src != dst && length >= 6;
        starts.push(if routed {
            attach(&mut cedges, i, 0, src, start_room[i], i)
        } else {
            None
        });
        ends.push(if routed {
            attach(&mut cedges, i, length - 3, dst, end_room[i], i)
        } else {
            None
        });
    }
    let on_border = |item: usize, x: f64, y: f64| {
        let mut g = cnodes[item].parent;
        let mut hops = 0;
        while let Some(at) = g {
            if hops >= 10_000 {
                break;
            }
            let c = &cnodes[at];
            let dx = (x - c.x).abs();
            let dy = (y - c.y).abs();
            if ((dy - c.h / 2.0).abs() < 0.5 && dx <= c.w / 2.0 + 0.5)
                || ((dx - c.w / 2.0).abs() < 0.5 && dy <= c.h / 2.0 + 0.5)
            {
                return true;
            }
            g = c.parent;
            hops += 1;
        }
        false
    };
    let mut gaps: Vec<f64> = Vec::new();
    for (&key, list) in &mut sides {
        if list.len() < 2 {
            continue;
        }
        let item = key >> 2;
        let across = key & 1;
        let length = if across == 1 {
            views[item].h
        } else {
            views[item].w
        };
        list.sort_by(|a, b| {
            a.toward
                .partial_cmp(&b.toward)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.order.cmp(&b.order))
        });
        let mut total = 0.0;
        for j in 0..list.len() - 1 {
            let (a, b) = (&list[j], &list[j + 1]);
            let mut gap = if across == 1 { 20.0 } else { 24.0 };
            if a.room > 0.0 && a.after {
                gap += if across == 1 {
                    tiny_height + 6.0
                } else {
                    a.room
                };
            }
            if b.room > 0.0 && !b.after {
                gap += if across == 1 {
                    tiny_height + 6.0
                } else {
                    b.room
                };
            }
            if gaps.len() <= j {
                gaps.resize(j + 1, 0.0);
            }
            gaps[j] = gap;
            total += gap;
        }
        let scale = ((length - 24.0) / total).min(1.0);
        if scale <= 0.0 {
            continue;
        }
        let mut at = if across == 1 {
            cnodes[item].y
        } else {
            cnodes[item].x
        } - (total * scale) / 2.0;
        for (j, end) in list.iter().enumerate() {
            let route = &mut cedges[end.edge].route;
            let tip = end.at;
            let step: isize = if tip == 0 { 3 } else { -3 };
            let first = tip as isize + step;
            let mut border = None;
            let mut k = first;
            while k >= 0
                && (k as usize) < route.len()
                && (route[k as usize + across] - route[first as usize + across]).abs() < 0.5
            {
                if on_border(item, route[k as usize], route[k as usize + 1]) {
                    border = Some(k);
                    break;
                }
                k += step;
            }
            if let Some(border) = border
                && (route[first as usize + across] - at).abs() <= 24.0
            {
                let mut k = first;
                while k != border + step {
                    route[k as usize + across] = at;
                    k += step;
                }
            }
            route[tip + across] = at;
            at += gaps.get(j).copied().unwrap_or(0.0) * scale;
        }
    }

    let straighten = |route: &mut Vec<f64>, end: Option<End>, room: f64, first: bool| {
        let Some(end) = end else {
            return;
        };
        if room <= 0.0 {
            return;
        }
        let across = end.side & 1;
        let at = if first { 0 } else { route.len() - 3 };
        let along = at + 1 - across;
        let reach = (route[if first { 3 } else { at - 3 } + 1 - across] - route[along]).abs();
        let length = if across == 1 {
            room + 10.0
        } else {
            tiny_height + 6.0
        }
        .min(reach * 0.35);
        if length < 4.0 {
            return;
        }
        let mut point = [route[at], route[at + 1], route[at + 2]];
        point[1 - across] += if end.side == 0 || end.side == 3 {
            -1.0
        } else {
            1.0
        } * length;
        let insert = if first { 3 } else { at };
        route.splice(insert..insert, point);
    };
    for i in 0..cedges.len() {
        straighten(&mut cedges[i].route, starts[i], start_room[i], true);
        straighten(&mut cedges[i].route, ends[i], end_room[i], false);
    }

    let (mut min_x, mut min_y, mut max_x, mut max_y) =
        (0.0_f64, 0.0_f64, layout.width, layout.height);
    let mut cards: Vec<Card> = Vec::new();
    let mut card =
        |parent: usize, raw: &str, width: f64, x: f64, y: f64, side: usize, after: bool| {
            if raw.is_empty() {
                return;
            }
            let h = tiny_height;
            let (mut cx, mut cy) = (x, y);
            if side & 1 == 1 {
                cx += if side == 3 { -1.0 } else { 1.0 } * (5.0 + width / 2.0);
                cy += if after { 1.0 } else { -1.0 } * (7.0 + h / 2.0);
            } else {
                cx += if after { 1.0 } else { -1.0 } * (9.0 + width / 2.0);
                cy += if side == 0 { -1.0 } else { 1.0 } * (3.0 + h / 2.0);
            }
            cards.push(Card {
                text: text::decode_entities(raw),
                x: cx,
                y: cy,
                w: width,
                h,
                parent,
            });
            min_x = min_x.min(cx - width / 2.0);
            max_x = max_x.max(cx + width / 2.0);
            min_y = min_y.min(cy - h / 2.0);
            max_y = max_y.max(cy + h / 2.0);
        };
    let mut loop_seen: HashMap<usize, usize> = HashMap::new();
    let mut loop_at = vec![0_usize; cedges.len()];
    for i in 0..cedges.len() {
        let edge = drawn[i];
        let ce = &mut cedges[i];
        if ce.src == ce.dst {
            let seen = loop_seen.entry(ce.src).or_insert(0);
            let k = *seen;
            *seen += 1;
            loop_at[i] = k;
            let c = &cnodes[ce.src];
            let view = &views[ce.src];
            let x = c.x + view.w / 2.0;
            let spread = (view.h / 2.0 - 4.0).min(8.0 + k as f64 * 6.0);
            card(
                ce.src,
                edge.start_label,
                start_room[i],
                x,
                c.y - spread,
                1,
                false,
            );
            card(
                ce.dst,
                edge.end_label,
                end_room[i],
                x,
                c.y + spread,
                1,
                true,
            );
            ce.label_x = c.x + view.w / 2.0 + LOOP + k as f64 * 8.0 + 4.0 + ce.label_w / 2.0;
            ce.label_y = c.y;
        } else if ce.route.len() >= 6 {
            let last = ce.route.len() - 3;
            let (from, to) = (starts[i], ends[i]);
            card(
                ce.src,
                edge.start_label,
                start_room[i],
                ce.route[0],
                ce.route[1],
                from.map_or(2, |end| end.side),
                from.is_none_or(|end| end.after),
            );
            card(
                ce.dst,
                edge.end_label,
                end_room[i],
                ce.route[last],
                ce.route[last + 1],
                to.map_or(0, |end| end.side),
                to.is_none_or(|end| end.after),
            );
        }
    }

    if let Some(available) = options.max_width.filter(|available| *available > 0.0)
        && max_x - min_x + 2.0 * pad > available
    {
        let mut all = cnodes.clone();
        all.extend(cards.iter().map(|item| {
            let mut c = cnode(item.w, item.h, Some(item.parent));
            c.x = item.x;
            c.y = item.y;
            c
        }));
        if let Some(wrapped) = wrap_components(
            &mut all,
            &mut cedges,
            (available - 2.0 * pad).max(1.0),
            number_option("nodeSpacing", NODE_SEP),
        ) {
            let placed = all.split_off(cnodes.len());
            cnodes = all;
            for (item, c) in cards.iter_mut().zip(placed) {
                item.x = c.x;
                item.y = c.y;
            }
            min_x = 0.0;
            min_y = 0.0;
            max_x = wrapped.width;
            max_y = wrapped.height;
        }
    }

    let ox = pad - min_x;
    let oy = pad - min_y;
    let width = (max_x - min_x + 2.0 * pad).ceil();
    let height = (max_y - min_y + 2.0 * pad).ceil();
    let mut links = Vec::new();

    let mut crossings = Crossings::new();
    if graph_nodes.first().is_some_and(|node| node.is_group) {
        for e in &cedges {
            mark_crossings(&mut crossings, &e.route);
        }
    }

    let surface_alt = svg::esc(&theme.surface_alt);
    let border = svg::esc(&theme.border);
    let muted = svg::esc(&theme.muted);
    let line = svg::esc(&theme.line);
    let radius = rounded_attr(theme.radius);
    let mut clusters = String::new();
    let mut struck_titles = String::new();
    let mut nodes_out = String::new();
    for (i, view) in views.iter().enumerate() {
        let c = &cnodes[i];
        let node = &graph_nodes[i];
        let x = c.x + ox;
        let y = c.y + oy;
        let id = svg::esc_text(node.id);
        let (w, h) = if c.is_group {
            (c.w, c.h)
        } else {
            (view.w, view.h)
        };
        if node.is_group {
            let title_x = if c.is_group {
                cluster_title_x(c, view.label.width, &crossings, GROUP_PAD)
            } else {
                c.x
            };
            let title_y = y - h / 2.0 + 8.0 + view.label.height / 2.0;
            let title_attrs = format!(" class=\"pele-cluster-label\" fill=\"{muted}\"");
            let struck = c.is_group && title_crossed(c, view.label.width, title_x, &crossings);
            if struck {
                struck_titles.push_str(&struck_title(
                    &view.label,
                    title_x + ox,
                    title_y,
                    &title_attrs,
                    &id,
                    &cnodes,
                    i,
                    options,
                ));
            }
            clusters.push_str(&format!(
                "<g class=\"pele-cluster pele-namespace\" data-id=\"{id}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{surface_alt}\" fill-opacity=\"0.5\" stroke=\"{border}\"{radius}/>{}</g>",
                n(x - w / 2.0),
                n(y - h / 2.0),
                n(w),
                n(h),
                if struck {
                    String::new()
                } else {
                    svg::label_with_options(&view.label, title_x + ox, title_y, &title_attrs, options)
                }
            ));
            continue;
        }
        let at = format!(" transform=\"translate({},{})\"", n(x), n(y));
        if let Some(class) = node.class {
            nodes_out.push_str(&class_svg(view, class, &id, &at, options, &mut links));
        } else if node.shape == Shape::Note {
            let r = w / 2.0;
            let t = -h / 2.0;
            nodes_out.push_str(&format!(
                "<g class=\"pele-node pele-note\" data-id=\"{id}\"{at}><path d=\"M{},{}H{}L{},{}V{}H{}Z\" fill=\"{surface_alt}\" stroke=\"{border}\" stroke-linejoin=\"round\"/><path d=\"M{},{}V{}H{}\" fill=\"none\" stroke=\"{border}\" stroke-linejoin=\"round\"/>{}</g>",
                n(-r),
                n(t),
                n(r - FOLD),
                n(r),
                n(t + FOLD),
                n(-t),
                n(-r),
                n(r - FOLD),
                n(t),
                n(t + FOLD),
                n(r),
                svg::label_with_options(
                    &view.label,
                    0.0,
                    0.0,
                    &format!(" class=\"pele-label\" font-size=\"{}\"", n(small)),
                    options
                )
            ));
        } else {
            nodes_out.push_str(&format!(
                "<g class=\"pele-node pele-interface\" data-id=\"{id}\"{at}>{}</g>",
                svg::label_with_options(
                    &view.label,
                    0.0,
                    0.0,
                    &format!(" class=\"pele-label\" font-size=\"{}\"", n(small)),
                    options
                )
            ));
        }
    }

    let mut edges_out = String::new();
    let mut labels_out = String::new();
    for i in 0..cedges.len() {
        let edge = drawn[i];
        let label = &edge_labels[i];
        let start_trim = edge.start.trim();
        let end_trim = edge.end.trim();
        let ce = &mut cedges[i];
        let path = if ce.src == ce.dst {
            let c = &cnodes[ce.src];
            let view = &views[ce.src];
            let k = loop_at[i] as f64;
            let x = c.x + ox + view.w / 2.0;
            let y = c.y + oy;
            let spread = (view.h / 2.0 - 4.0).min(8.0 + k * 6.0);
            let reach = LOOP + k * 8.0;
            EdgePath {
                d: format!(
                    "M{},{}C{},{} {},{} {},{}",
                    n(x + start_trim),
                    n(y - spread),
                    n(x + reach + start_trim),
                    n(y - spread),
                    n(x + reach + end_trim),
                    n(y + spread),
                    n(x + end_trim),
                    n(y + spread)
                ),
                tokens: None,
                sx: x,
                sy: y - spread,
                sdx: -1.0,
                sdy: 0.0,
                ex: x,
                ey: y + spread,
                edx: -1.0,
                edy: 0.0,
            }
        } else {
            for point in ce.route.as_chunks_mut::<3>().0 {
                point[0] += ox;
                point[1] += oy;
            }
            route_path(&ce.route, None, start_trim, end_trim, None, None)
        };
        let attrs = match edge.pattern {
            Pattern::Dashed => " stroke-dasharray=\"6 4\"",
            Pattern::Dotted => " stroke-dasharray=\"2 4\"",
            Pattern::Solid => "",
        };
        edges_out.push_str(&format!(
            "<g class=\"pele-edge{}\" data-id=\"{}\"><path d=\"{}\"{attrs}/>{}{}</g>",
            if edge.pattern == Pattern::Dotted {
                " pele-note-edge"
            } else {
                " pele-relation"
            },
            svg::esc_text(&edge.id),
            path.d,
            edge.start.draw(path.sx, path.sy, path.sdx, path.sdy, &line),
            edge.end.draw(path.ex, path.ey, path.edx, path.edy, &line),
        ));
        if label.width > 0.0 {
            labels_out.push_str(&edge_label_svg(
                &edge.id,
                label,
                ce.label_x + ox,
                ce.label_y + oy,
                "",
                options,
            ));
        }
    }

    let mut cards_out = String::new();
    for item in &cards {
        cards_out.push_str(&format!(
            "<text class=\"pele-cardinality\" x=\"{}\" y=\"{}\" text-anchor=\"middle\">{}</text>",
            n(item.x + ox),
            n(item.y + oy + tiny * 0.35),
            svg::esc(&item.text)
        ));
    }

    let mut content = String::new();
    if !clusters.is_empty() {
        content.push_str(&format!("<g class=\"pele-clusters\">{clusters}</g>"));
    }
    if !edges_out.is_empty() {
        content.push_str(&format!(
            "<g class=\"pele-edges\" fill=\"none\" stroke=\"{line}\" stroke-linecap=\"round\">{edges_out}</g>"
        ));
    }
    if !struck_titles.is_empty() {
        content.push_str(&format!(
            "<g class=\"pele-cluster-titles\">{struck_titles}</g>"
        ));
    }
    if !labels_out.is_empty() {
        content.push_str(&format!(
            "<g class=\"pele-edge-labels\" font-size=\"{}\">{labels_out}</g>",
            n(small)
        ));
    }
    if !cards_out.is_empty() {
        content.push_str(&format!(
            "<g class=\"pele-cardinalities\" font-size=\"{}\" fill=\"{muted}\">{cards_out}</g>",
            n(tiny)
        ));
    }
    content.push_str(&format!("<g class=\"pele-nodes\">{nodes_out}</g>"));
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
    effective.acc_title = db.acc_title.clone().or(effective.acc_title);
    effective.acc_descr = db.acc_descr.clone().or(effective.acc_descr);
    let svg = svg::document(
        "class",
        titled.width,
        titled.height,
        &effective,
        &titled.content,
    );
    let mut result = RenderResult::new("class", svg, titled.width, titled.height);
    result.links = links;
    Ok(result)
}

fn class_view(
    db: &Db,
    class: &ClassNode,
    styler: &mut Styler,
    options: &RenderOptions,
    hide_empty: bool,
) -> View {
    let size = options.font_size;
    let tiny = js_round(size * 0.8);
    let tiny_height = js_round(tiny * 1.4);
    let style = styler.style(db, class);
    let font_size = style.font_size.unwrap_or(size);
    let member_size = js_round(font_size * 0.875);
    let line_height = js_round(member_size * 1.5);
    let generic = if class.kind.is_empty() {
        String::new()
    } else {
        format!(
            "&lt;{}&gt;",
            parse_generic_types(&class.kind)
                .replace('<', "&lt;")
                .replace('>', "&gt;")
        )
    };
    let label = text::layout_label(
        &format!("{}{generic}", class.label),
        false,
        options.text_measurer(),
        font_size,
        4000.0,
        BOLD | if style.italic { ITALIC } else { 0 },
    );
    let annotations: Vec<String> = class
        .annotations
        .iter()
        .map(|annotation| format!("«{}»", js_trim(&text::decode_entities(annotation))))
        .collect();
    let attributes: Vec<Line> = class
        .members
        .iter()
        .map(|member| member_line(member, options, member_size))
        .collect();
    let methods: Vec<Line> = class
        .methods
        .iter()
        .map(|member| member_line(member, options, member_size))
        .collect();
    let mut inner = label.width;
    let mut annotations_h = 0.0;
    for annotation in &annotations {
        inner = inner.max(options.measure(annotation, tiny, 0));
        annotations_h += tiny_height;
    }
    for line in attributes.iter().chain(&methods) {
        inner = inner.max(line.width);
    }
    let sections = !(hide_empty && attributes.is_empty() && methods.is_empty());
    let section = |count: usize| {
        if !sections {
            0.0
        } else if count > 0 {
            2.0 * SECTION_PAD + count as f64 * line_height
        } else {
            EMPTY_SECTION
        }
    };
    let title_h = 2.0 * TITLE_PAD + annotations_h + label.height.max(js_round(font_size * 1.5));
    let attributes_h = section(attributes.len());
    let methods_h = section(methods.len());
    View {
        style,
        w: (inner + 2.0 * PAD_X).max(MIN_WIDTH),
        h: title_h + attributes_h + methods_h,
        label,
        annotations,
        attributes,
        methods,
        title_h,
        attributes_h,
        font_size,
    }
}

fn member_lines(
    list: &[Line],
    kind: &str,
    from: f64,
    left: f64,
    member_size: f64,
    line_height: f64,
    style: &ResolvedStyle,
) -> String {
    let n = svg::num;
    let mut out = String::new();
    let mut baseline = from + SECTION_PAD + line_height / 2.0 + member_size * 0.35;
    for line in list {
        let mark = match line.classifier {
            Some('$') => format!(
                " pele-static\"{}",
                if style.text.is_empty() {
                    " style=\"text-decoration:underline\"".to_owned()
                } else {
                    format!(
                        "{}text-decoration:underline\"",
                        &style.text[..style.text.len() - 1]
                    )
                }
            ),
            Some('*') => format!(" pele-abstract\" font-style=\"italic\"{}", style.text),
            _ => format!("\"{}", style.text),
        };
        out.push_str(&format!(
            "<text class=\"pele-member pele-{kind}{mark} x=\"{}\" y=\"{}\">{}</text>",
            n(left + PAD_X),
            n(baseline),
            svg::esc(&line.text)
        ));
        baseline += line_height;
    }
    out
}

fn class_svg(
    view: &View,
    class: &ClassNode,
    id: &str,
    at: &str,
    options: &RenderOptions,
    links: &mut Vec<LinkInfo>,
) -> String {
    let n = svg::num;
    let theme = &options.theme;
    let size = options.font_size;
    let tiny = js_round(size * 0.8);
    let tiny_height = js_round(tiny * 1.4);
    let (w, h, style) = (view.w, view.h, &view.style);
    let left = -w / 2.0;
    let top = -h / 2.0;
    let member_size = js_round(view.font_size * 0.875);
    let line_height = js_round(member_size * 1.5);
    let border = svg::esc(theme.node_stroke());
    let mut body = format!(
        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\" stroke=\"{border}\"{}/>",
        n(left),
        n(top),
        n(w),
        n(h),
        svg::esc(theme.node_fill()),
        rounded(&style.shape, theme.radius)
    );
    let ink = ink_fill(style, None);
    let first = top + view.title_h;
    let second = first + view.attributes_h;
    if h > view.title_h {
        let stroke = style.stroke.as_ref().map_or(String::new(), |stroke| {
            format!(" style=\"stroke:{}\"", svg::esc(stroke))
        });
        body.push_str(&format!(
            "<path class=\"pele-divider\" d=\"M{},{}H{}M{},{}H{}\" fill=\"none\" stroke=\"{border}\"{stroke}/>",
            n(left),
            n(first),
            n(-left),
            n(left),
            n(second),
            n(-left)
        ));
    }
    let annotation_fill = if ink.is_empty() {
        format!(" fill=\"{}\"", svg::esc(&theme.muted))
    } else {
        ink.clone()
    };
    let mut y = top + TITLE_PAD;
    for annotation in &view.annotations {
        body.push_str(&format!(
            "<text class=\"pele-annotation\" x=\"0\" y=\"{}\" text-anchor=\"middle\" font-size=\"{}\"{annotation_fill}>{}</text>",
            n(y + tiny_height / 2.0 + tiny * 0.35),
            n(tiny),
            svg::esc(annotation)
        ));
        y += tiny_height;
    }
    let title_size = if view.font_size == size {
        String::new()
    } else {
        format!(" font-size=\"{}\"", n(view.font_size))
    };
    body.push_str(&svg::label_with_options(
        &view.label,
        0.0,
        (y + first - TITLE_PAD) / 2.0,
        &format!(
            " class=\"pele-label\"{title_size} font-weight=\"bold\"{ink}{}",
            style.text
        ),
        options,
    ));
    if !view.attributes.is_empty() || !view.methods.is_empty() {
        body.push_str(&format!(
            "<g class=\"pele-members\" font-size=\"{}\"{ink}>{}{}</g>",
            n(member_size),
            member_lines(
                &view.attributes,
                "attribute",
                first,
                left,
                member_size,
                line_height,
                style
            ),
            member_lines(
                &view.methods,
                "method",
                second,
                left,
                member_size,
                line_height,
                style
            ),
        ));
    }
    if let Some(tooltip) = &class.tooltip {
        body = format!("<title>{}</title>{body}", svg::esc_text(tooltip));
    }
    if let Some(url) = &class.link
        && let Some(href) = link_url(url)
    {
        let target = class
            .link_target
            .as_deref()
            .filter(|target| TARGETS.contains(target))
            .map_or(String::new(), |target| format!(" target=\"{target}\""));
        body = format!(
            "<a href=\"{}\"{target} rel=\"noopener\">{body}</a>",
            svg::esc(&href)
        );
        links.push(LinkInfo {
            id: text::decode_entities(&class.id),
            href,
            internal: false,
        });
    }
    let classes = class
        .css_classes
        .strip_prefix("default")
        .map_or(class.css_classes.as_str(), |rest| {
            rest.strip_prefix(js_space).unwrap_or(rest)
        });
    format!(
        "<g class=\"pele-node pele-class{}\" data-id=\"{id}\"{at}>{body}</g>",
        class_names(classes)
    )
}
