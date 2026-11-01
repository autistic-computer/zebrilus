use std::sync::LazyLock;

use regex::Regex;

pub const BOLD: u8 = 1;
pub const ITALIC: u8 = 2;
pub const MONO: u8 = 4;
pub const SUB: u8 = 8;
pub const SUP: u8 = 16;
pub const UNDERLINE: u8 = 32;
pub const LINE_HEIGHT: f64 = 1.5;

const JS_SPACE: &str = r"\u{0009}-\u{000d}\u{0020}\u{00a0}\u{1680}\u{2000}-\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}\u{feff}";

const REGULAR_METRICS: &str = "7q7q9vfgfgopij5b9999atg87q997q7qfgfgfgfgfgfgfgfgfgfg7q7qg8g8g8fgs7ijijk2k2ijgzlmk27qdwijfgn5k2lmijlmk2ijgzk2ijq8ijijgz7q7q7qd1fg99fgfgdwfgfg7qfgfg6666dw66n5fgfgfgfg99dw7qfgdwk2dwdwdw9a789ag8";
const BOLD_METRICS: &str = "7q99d6fgfgopk26m9999atg87q997q7qfgfgfgfgfgfgfgfgfgfg9999g8g8g8gzr3k2k2k2k2ijgzlmk27qfgk2gzn5k2lmijlmk2ijgzk2ijq8ijijgz997q99g8fg99fggzfggzfg99gzgz7q7qfg7qopgzgzgzgzatfg99gzfglmfgfgdwat7satg8";

static REGULAR_WIDTHS: LazyLock<[u16; 95]> = LazyLock::new(|| unpack(REGULAR_METRICS));
static BOLD_WIDTHS: LazyLock<[u16; 95]> = LazyLock::new(|| unpack(BOLD_METRICS));
static BREAKS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"(?i)<br[{JS_SPACE}]*/?>|\\n|\n")).unwrap());
static TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?i)^<(/?)([a-z][a-z0-9_-]*)(?:[{JS_SPACE}][^<>]*)?/?>"
    ))
    .unwrap()
});
static ICON: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"fa[bklrs]?:fa-[A-Za-z0-9_-]+").unwrap());
static ENTITY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"&(?:#([0-9]+)|#[xX]([0-9a-fA-F]+)|([A-Za-z0-9_]+));").unwrap());
static PLACEHOLDER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"ﬂ°(°?)([A-Za-z0-9_]+)¶ß").unwrap());
static MARKUP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[<\\\n*_]|fa[bklrs]?:fa-").unwrap());
static PIECE: LazyLock<Regex> = LazyLock::new(|| {
    let cjk = r"\u{2e80}-\u{9fff}\u{f900}-\u{faff}\u{ff00}-\u{ff60}";
    let close = r"、。，．：；！？）］｝」』】〕〉》〙〗ー…‥・ゝゞぁぃぅぇぉっゃゅょゎァィゥェォッャュョヮ,.:;!?)\]}";
    let open = r"（［｛「『【〔〈《〘〖(\[\{";
    Regex::new(&format!(
        r"[{JS_SPACE}]*(?:[{open}]*[{cjk}][{close}]*|[^{JS_SPACE}{cjk}\u{{200b}}\u{{00ad}}]+[\u{{200b}}\u{{00ad}}]*[{close}]*|[\u{{200b}}\u{{00ad}}]+)[{JS_SPACE}]*|[{JS_SPACE}]+"
    ))
    .unwrap()
});

pub trait TextMeasurer: Send + Sync {
    fn width(&self, text: &str, size: f64, style: u8) -> f64;
}

impl<F: Fn(&str, f64, u8) -> f64 + Send + Sync> TextMeasurer for F {
    fn width(&self, text: &str, size: f64, style: u8) -> f64 {
        self(text, size, style)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MetricsMeasurer;

impl TextMeasurer for MetricsMeasurer {
    fn width(&self, text: &str, size: f64, style: u8) -> f64 {
        measure(text, size, style)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    pub text: String,
    pub style: u8,
    pub width: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Label {
    pub lines: Vec<Vec<Span>>,
    pub widths: Vec<f64>,
    pub width: f64,
    pub height: f64,
    pub size: f64,
    pub line_height: f64,
}

fn js_space(ch: char) -> bool {
    matches!(ch, '\u{9}'..='\u{d}' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
}

fn unpack(packed: &str) -> [u16; 95] {
    let mut widths = [0; 95];
    for (index, value) in widths.iter_mut().enumerate() {
        *value = u16::from_str_radix(&packed[index * 2..index * 2 + 2], 36).unwrap();
    }
    widths
}

pub fn measure(text: &str, size: f64, style: u8) -> f64 {
    let mono = style & MONO != 0;
    let effective = if style & (SUB | SUP) != 0 {
        size / 1.2
    } else {
        size
    };
    let widths = if style & BOLD != 0 {
        &*BOLD_WIDTHS
    } else {
        &*REGULAR_WIDTHS
    };
    let mut units = 0_u64;
    for code in text.encode_utf16() {
        units += match code {
            0..32 => 0,
            32..127 => {
                if mono {
                    600
                } else {
                    u64::from(widths[usize::from(code - 32)])
                }
            }
            0xad | 0x300..=0x36f | 0x200b..=0x200f | 0xdc00..=0xdfff | 0xfe00..=0xfe0f | 0xfeff => {
                0
            }
            0x1100..=0x115f
            | 0x2e80..=0xa4cf
            | 0xac00..=0xd7a3
            | 0xf900..=0xfaff
            | 0xfe30..=0xfe4f
            | 0xff00..=0xff60
            | 0xffe0..=0xffe6
            | 0xd800..=0xdbff => {
                if mono {
                    1200
                } else {
                    1000
                }
            }
            _ => {
                if mono {
                    600
                } else {
                    580
                }
            }
        };
    }
    (units as f64 * effective) / 1000.0
}

pub fn label(raw: &str, size: f64, max_width: Option<f64>) -> Label {
    label_with_measurer(raw, size, max_width, &MetricsMeasurer)
}

pub fn label_with_measurer(
    raw: &str,
    size: f64,
    max_width: Option<f64>,
    measurer: &dyn TextMeasurer,
) -> Label {
    let markdown = raw.starts_with('`') && raw.ends_with('`') && raw.len() >= 2;
    let raw = if markdown {
        &raw[1..raw.len() - 1]
    } else {
        raw
    };
    layout_label(
        raw,
        markdown,
        measurer,
        size,
        max_width.unwrap_or(f64::INFINITY),
        0,
    )
}

pub fn layout_label(
    raw: &str,
    markdown: bool,
    measurer: &dyn TextMeasurer,
    size: f64,
    max_width: f64,
    base: u8,
) -> Label {
    if raw.is_empty() {
        return Label::default();
    }
    if !MARKUP.is_match(raw) && !raw.contains('&') && !raw.contains('ﬂ') {
        let plain = raw.trim_matches(js_space);
        let width = measurer.width(plain, size, base);
        let breakable = plain.chars().any(|ch| {
            ch == ' '
                || ch == '\u{200b}'
                || ch == '\u{ad}'
                || matches!(ch, '\u{2e80}'..='\u{9fff}' | '\u{f900}'..='\u{faff}' | '\u{ff00}'..='\u{ff60}')
        });
        if width <= max_width || !breakable {
            return Label {
                lines: vec![vec![Span {
                    text: plain.to_owned(),
                    style: base,
                    width,
                }]],
                widths: vec![width],
                width,
                height: (size * LINE_HEIGHT + 0.5).floor(),
                size,
                line_height: (size * LINE_HEIGHT + 0.5).floor(),
            };
        }
    }
    let source = if markdown {
        dedent(&raw.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        raw.to_owned()
    };
    let mut lines = Vec::new();
    let mut widths = Vec::new();
    let mut open = Open {
        style: base,
        shifts: Vec::new(),
    };
    for part in BREAKS.split(source.trim_matches(js_space)) {
        let mut line = inline(part.trim_matches(js_space), markdown, base, &mut open);
        let width: f64 = line
            .iter_mut()
            .map(|span| {
                span.width = measurer.width(&span.text, size, span.style);
                span.width
            })
            .sum();
        if width > max_width {
            wrap(&line, measurer, size, max_width, &mut lines, &mut widths);
        } else {
            lines.push(line);
            widths.push(width);
        }
    }
    let line_height = (size * if lines.len() > 1 { 1.3 } else { LINE_HEIGHT } + 0.5).floor();
    Label {
        width: widths.iter().copied().fold(0.0, f64::max),
        height: lines.len() as f64 * line_height,
        lines,
        widths,
        size,
        line_height,
    }
}

fn dedent(source: &str) -> String {
    let indent = source
        .lines()
        .filter(|line| !line.trim_matches(js_space).is_empty())
        .map(|line| line.chars().take_while(|ch| js_space(*ch)).count())
        .min()
        .unwrap_or(0);
    source
        .split('\n')
        .map(|line| {
            let remove = indent.min(line.chars().take_while(|ch| js_space(*ch)).count());
            let start = line
                .char_indices()
                .nth(remove)
                .map_or(line.len(), |(at, _)| at);
            &line[start..]
        })
        .collect::<Vec<_>>()
        .join("\n")
}

struct Open {
    style: u8,
    shifts: Vec<u8>,
}

fn inline(source: &str, markdown: bool, base: u8, open: &mut Open) -> Vec<Span> {
    let mut line = Vec::new();
    let mut style = open.style;
    let mut marked = 0_u8;
    let mut text = String::new();
    let mut offset = 0;
    while offset < source.len() {
        let rest = &source[offset..];
        let ch = rest.chars().next().unwrap();
        if ch == '<' {
            if let Some(tag) = TAG.captures(rest) {
                let name = tag[2].to_ascii_lowercase();
                let bit = match name.as_str() {
                    "b" | "strong" => BOLD,
                    "i" | "em" | "cite" | "dfn" | "var" => ITALIC,
                    "code" | "kbd" | "samp" | "tt" => MONO,
                    "sub" => SUB,
                    "sup" => SUP,
                    "u" | "ins" => UNDERLINE,
                    _ => 0,
                };
                let closing = &tag[1] == "/";
                if bit & (SUB | SUP) != 0 {
                    push_icons(
                        &mut line,
                        &text,
                        style | open.shifts.last().copied().unwrap_or(0),
                    );
                    text.clear();
                    if !closing {
                        open.shifts.push(bit);
                    } else if let Some(at) = open.shifts.iter().rposition(|value| *value == bit) {
                        open.shifts.remove(at);
                    }
                } else if bit != 0 {
                    push_icons(
                        &mut line,
                        &text,
                        style | open.shifts.last().copied().unwrap_or(0),
                    );
                    text.clear();
                    if closing {
                        open.style = (open.style & !bit) | (base & bit);
                        style = (style & !bit) | (base & bit);
                    } else {
                        open.style |= bit;
                        style |= bit;
                    }
                }
                if bit != 0 || is_html_tag(&name) {
                    offset += tag.get(0).unwrap().len();
                    continue;
                }
            }
        } else if markdown && matches!(ch, '*' | '_') {
            let doubled = rest.chars().nth(1) == Some(ch);
            let bit = if doubled { BOLD } else { ITALIC };
            let length = if doubled { 2 } else { 1 };
            let marker = if doubled {
                ch.to_string().repeat(2)
            } else {
                ch.to_string()
            };
            let previous = source[..offset].chars().next_back().unwrap_or(' ');
            let next = rest[length..].chars().next().unwrap_or(' ');
            let opening = marked & bit == 0;
            let can_open = opening && !js_space(next) && rest[length..].contains(&marker);
            let can_close = !opening && !js_space(previous);
            let wordy = ch == '_' && word_character(previous) && word_character(next);
            if (can_open || can_close) && !wordy {
                push_icons(
                    &mut line,
                    &text,
                    style | open.shifts.last().copied().unwrap_or(0),
                );
                text.clear();
                if can_open {
                    marked |= bit;
                    style |= bit;
                } else {
                    marked &= !bit;
                    style = (style & !bit) | (base & bit);
                }
                offset += length;
                continue;
            }
        }
        text.push(ch);
        offset += ch.len_utf8();
    }
    push_icons(
        &mut line,
        &text,
        style | open.shifts.last().copied().unwrap_or(0),
    );
    line
}

fn word_character(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn is_html_tag(tag: &str) -> bool {
    const TAGS: &str = "a abbr address article aside audio b bdi bdo big blockquote body br button caption center cite code col colgroup data dd del details dfn dialog div dl dt em embed fieldset figcaption figure font footer form h1 h2 h3 h4 h5 h6 head header hr html i iframe img input ins kbd label legend li link main map mark meta meter nav noscript object ol optgroup option output p picture pre progress q rp rt ruby s samp script section select small source span strike strong style sub summary sup table tbody td template textarea tfoot th thead time title tr track tt u ul var video wbr";
    TAGS.split(' ').any(|known| known == tag)
}

fn push_plain(line: &mut Vec<Span>, text: &str, style: u8) {
    if text.is_empty() {
        return;
    }
    let decoded = decode_entities(text);
    if let Some(last) = line.last_mut().filter(|span| span.style == style) {
        last.text.push_str(&decoded);
    } else {
        line.push(Span {
            text: decoded,
            style,
            width: 0.0,
        });
    }
}

fn push_icons(line: &mut Vec<Span>, text: &str, style: u8) {
    let mut last = 0;
    for icon in ICON.find_iter(text) {
        push_plain(line, &text[last..icon.start()], style);
        last = icon.end();
        if text[last..].starts_with(' ') {
            last += 1;
        } else if last == text.len()
            && let Some(before) = line.last_mut()
        {
            before
                .text
                .truncate(before.text.trim_end_matches(js_space).len());
        }
    }
    push_plain(line, &text[last..], style);
}

fn wrap(
    line: &[Span],
    measurer: &dyn TextMeasurer,
    size: f64,
    max: f64,
    out: &mut Vec<Vec<Span>>,
    widths: &mut Vec<f64>,
) {
    let mut current = Vec::new();
    let mut width = 0.0;
    for (index, span) in line.iter().enumerate() {
        let pieces: Vec<&str> = PIECE
            .find_iter(&span.text)
            .map(|piece| piece.as_str())
            .collect();
        let soft = span.text.contains('\u{ad}');
        let mut own = vec![0.0; pieces.len()];
        let mut rest = vec![0.0; pieces.len() + 1];
        if soft {
            for position in (0..pieces.len()).rev() {
                let shy = pieces[position].ends_with('\u{ad}');
                let end = pieces[position].trim_end_matches(js_space);
                let visible = if shy {
                    end.strip_suffix('\u{ad}').unwrap_or(end)
                } else {
                    end
                };
                own[position] = measurer.width(visible, size, span.style);
                rest[position] = if !shy {
                    own[position]
                } else if position + 1 == pieces.len() {
                    if index + 1 == line.len() {
                        own[position]
                    } else {
                        f64::INFINITY
                    }
                } else {
                    own[position] + rest[position + 1]
                };
            }
        }
        for (position, piece) in pieces.iter().enumerate() {
            let mut part = *piece;
            let part_width = if soft {
                own[position]
            } else {
                measurer.width(part.trim_end_matches(js_space), size, span.style)
            };
            let fits = if soft && part.ends_with('\u{ad}') {
                let hyphenated = format!("{}-", part.strip_suffix('\u{ad}').unwrap());
                width + measurer.width(&hyphenated, size, span.style) <= max
                    || width + rest[position] <= max
            } else {
                width + part_width <= max
            };
            if width > 0.0 && !fits {
                commit(
                    &mut current,
                    &mut width,
                    measurer,
                    size,
                    out,
                    widths,
                    part.trim_start_matches(js_space) == part,
                );
                part = part.trim_start_matches(js_space);
            }
            if let Some(last) = current.last_mut().filter(|value| value.style == span.style) {
                last.text.push_str(part);
                if part_width > 0.0 || part.trim_end_matches(js_space) != part {
                    let before = last.width;
                    last.width = measurer.width(&last.text, size, last.style);
                    width += last.width - before;
                }
            } else {
                let part_width = measurer.width(part, size, span.style);
                current.push(Span {
                    text: part.to_owned(),
                    style: span.style,
                    width: part_width,
                });
                width += part_width;
            }
        }
    }
    if !current.is_empty() || out.is_empty() {
        commit(&mut current, &mut width, measurer, size, out, widths, false);
    }
}

fn commit(
    current: &mut Vec<Span>,
    width: &mut f64,
    measurer: &dyn TextMeasurer,
    size: f64,
    out: &mut Vec<Vec<Span>>,
    widths: &mut Vec<f64>,
    hyphen: bool,
) {
    if let Some(last) = current.last_mut() {
        let mut trimmed = last.text.trim_end_matches(js_space).to_owned();
        if hyphen && last.text.ends_with('\u{ad}') {
            trimmed.pop();
            trimmed.push('-');
        }
        if trimmed != last.text {
            last.text = trimmed;
            let measured = measurer.width(&last.text, size, last.style);
            *width += measured - last.width;
            last.width = measured;
        }
    }
    out.push(std::mem::take(current));
    widths.push(*width);
    *width = 0.0;
}

pub fn decode_entities(text: &str) -> String {
    let restored = PLACEHOLDER.replace_all(text, |capture: &regex::Captures<'_>| {
        format!(
            "&{}{};",
            if capture[1].is_empty() { "" } else { "#" },
            &capture[2]
        )
    });
    ENTITY
        .replace_all(&restored, |capture: &regex::Captures<'_>| {
            let numeric = if let Some(decimal) = capture.get(1) {
                decimal.as_str().parse::<u32>().ok()
            } else if let Some(hex) = capture.get(2) {
                u32::from_str_radix(hex.as_str(), 16).ok()
            } else {
                named_entity(capture.get(3).unwrap().as_str())
            };
            numeric
                .filter(|code| *code != 0)
                .and_then(char::from_u32)
                .map_or_else(|| capture[0].to_owned(), |value| value.to_string())
        })
        .into_owned()
}

fn named_entity(name: &str) -> Option<u32> {
    const NAMED: &str = "amp:38,lt:60,gt:62,quot:34,apos:39,nbsp:160,shy:173,iexcl:161,cent:162,pound:163,curren:164,yen:165,sect:167,copy:169,laquo:171,not:172,reg:174,deg:176,plusmn:177,sup2:178,sup3:179,micro:181,para:182,middot:183,sup1:185,raquo:187,frac14:188,frac12:189,frac34:190,iquest:191,times:215,divide:247,ndash:8211,mdash:8212,lsquo:8216,rsquo:8217,sbquo:8218,ldquo:8220,rdquo:8221,bdquo:8222,dagger:8224,Dagger:8225,bull:8226,hellip:8230,permil:8240,prime:8242,Prime:8243,lsaquo:8249,rsaquo:8250,euro:8364,trade:8482,larr:8592,uarr:8593,rarr:8594,darr:8595,harr:8596,lArr:8656,uArr:8657,rArr:8658,dArr:8659,hArr:8660,forall:8704,part:8706,exist:8707,empty:8709,nabla:8711,isin:8712,notin:8713,ni:8715,prod:8719,sum:8721,minus:8722,radic:8730,infin:8734,and:8743,or:8744,cap:8745,cup:8746,int:8747,there4:8756,sim:8764,cong:8773,asymp:8776,ne:8800,equiv:8801,le:8804,ge:8805,sub:8834,sup:8835,sube:8838,supe:8839,oplus:8853,otimes:8855,perp:8869,sdot:8901,loz:9674,spades:9824,clubs:9827,hearts:9829,diams:9830,check:10003,cross:10007,AMP:38,LT:60,GT:62,QUOT:34,lsqb:91,rsqb:93,lcub:123,rcub:125,verbar:124,VerticalLine:124,midast:42,DiacriticalGrave:96,Hat:94,UnderBar:95,equals:61,colon:58,semi:59,num:35,percnt:37,lpar:40,rpar:41,lbrack:91,rbrack:93,lbrace:123,rbrace:125,vert:124,sol:47,bsol:92,quest:63,excl:33,ast:42,plus:43,comma:44,period:46,dollar:36,commat:64,grave:96,hat:94,lowbar:95,tilde:126,Alpha:913,Beta:914,Gamma:915,Delta:916,Epsilon:917,Zeta:918,Eta:919,Theta:920,Iota:921,Kappa:922,Lambda:923,Mu:924,Nu:925,Xi:926,Omicron:927,Pi:928,Rho:929,Sigma:931,Tau:932,Upsilon:933,Phi:934,Chi:935,Psi:936,Omega:937,alpha:945,beta:946,gamma:947,delta:948,epsilon:949,zeta:950,eta:951,theta:952,iota:953,kappa:954,lambda:955,mu:956,nu:957,xi:958,omicron:959,pi:960,rho:961,sigmaf:962,sigma:963,tau:964,upsilon:965,phi:966,chi:967,psi:968,omega:969";
    NAMED.split(',').find_map(|pair| {
        let (known, number) = pair.split_once(':')?;
        (known == name).then(|| number.parse().unwrap())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arial_advances_match_upstream_metrics() {
        assert_eq!(measure("Hello", 16.0, 0), 36.448);
        assert_eq!(measure("Hello", 16.0, BOLD), 39.12);
        assert_eq!(measure("Hello", 16.0, MONO), 48.0);
        assert_eq!(measure("日本語", 16.0, 0), 48.0);
        assert_eq!(measure("👋", 16.0, 0), 16.0);
        assert_eq!(measure("a\u{301}", 16.0, 0), measure("a", 16.0, 0));
    }

    #[test]
    fn entities_are_decoded_once_and_invalid_codes_preserved() {
        assert_eq!(
            decode_entities("&amp;lt; &#x1f44b; &#0; &#55296; &bogus;"),
            "&lt; 👋 &#0; &#55296; &bogus;"
        );
        assert_eq!(decode_entities("ﬂ°°35¶ß ﬂ°quot¶ß"), "# \"");
    }

    #[test]
    fn labels_preserve_style_across_html_breaks() {
        let output = label("<strong>A<br/>B</strong><sup>2</sup>", 16.0, None);
        assert_eq!(output.lines.len(), 2);
        assert_eq!(output.lines[0][0].style, BOLD);
        assert_eq!(output.lines[1][0].style, BOLD);
        assert_eq!(output.lines[1][1].style, SUP);
        assert_eq!(output.height, 42.0);
    }

    #[test]
    fn wrapping_uses_words_cjk_and_discretionary_hyphens() {
        let output = label("one two three", 16.0, Some(45.0));
        assert!(output.lines.len() >= 2);
        assert!(output.widths.iter().all(|width| *width <= 45.0));
        let cjk = label("日本語", 16.0, Some(17.0));
        assert_eq!(cjk.lines.len(), 3);
        let soft = label("hy\u{ad}phen", 16.0, Some(30.0));
        assert_eq!(soft.lines[0][0].text, "hy-");
    }

    #[test]
    fn custom_measurer_controls_layout() {
        let fixed = |text: &str, _: f64, _: u8| text.chars().count() as f64 * 10.0;
        let output = label_with_measurer("aa bb", 12.0, Some(25.0), &fixed);
        assert_eq!(output.width, 20.0);
        assert_eq!(output.lines.len(), 2);
    }

    #[test]
    fn unknown_generic_tags_remain_literal_and_html_is_inert() {
        let output = label(
            "List<T> <script>alert(1)</script> fa:fa-home ok",
            12.0,
            None,
        );
        assert_eq!(output.lines[0][0].text, "List<T> alert(1) ok");
    }

    #[test]
    fn markdown_dedent_handles_unicode_whitespace_without_splitting_utf8() {
        let output = label("`\n a\n\u{2003}b\n`", 12.0, None);
        assert_eq!(output.lines.len(), 2);
        assert_eq!(output.lines[0][0].text, "a");
        assert_eq!(output.lines[1][0].text, "b");
    }

    #[test]
    fn empty_and_whitespace_only_labels_match_upstream_model_shape() {
        assert_eq!(label("", 16.0, None), Label::default());
        let spaces = label("  ", 16.0, None);
        assert_eq!(spaces.lines.len(), 1);
        assert_eq!(spaces.lines[0].len(), 1);
        assert_eq!(spaces.lines[0][0].text, "");
        assert_eq!(spaces.height, 24.0);
    }

    #[test]
    fn javascript_whitespace_keeps_next_line_and_trims_bom() {
        assert_eq!(label(" \u{85} ", 16.0, None).lines[0][0].text, "\u{85}");
        assert_eq!(label("\u{feff}", 16.0, None).lines[0][0].text, "");
    }
}
