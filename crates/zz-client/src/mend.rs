use std::cmp::Reverse;

pub const PENDING_LINK_URL: &str = "zz:pending-link";

const ZERO_WIDTH_SPACE: char = '\u{200B}';

pub fn mend(text: &str) -> Option<String> {
    let start = tail_start(text)?;
    let mended = close_hanging(&text[start..])?;
    if start == 0 {
        return Some(mended);
    }
    let mut repaired = String::with_capacity(start + mended.len());
    repaired.push_str(&text[..start]);
    repaired.push_str(&mended);
    Some(repaired)
}

fn tail_start(text: &str) -> Option<usize> {
    let mut fence: Option<(char, usize)> = None;
    let mut start = 0;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let next = offset + line.len();
        let body = line.trim_end();
        match fence {
            Some((ch, len)) => {
                if let Some((c, run, info)) = fence_marker(body)
                    && c == ch
                    && run >= len
                    && info.is_empty()
                {
                    fence = None;
                    start = next;
                }
            }
            None => {
                if let Some((ch, len, _)) = fence_marker(body) {
                    fence = Some((ch, len));
                } else if body.is_empty() {
                    start = next;
                }
            }
        }
        offset = next;
    }
    if fence.is_some() {
        return None;
    }
    let first = text[start..].lines().next().unwrap_or_default();
    (!first.starts_with("    ") && !first.starts_with('\t')).then_some(start)
}

fn fence_marker(line: &str) -> Option<(char, usize, &str)> {
    let body = line.trim_start_matches(' ');
    if line.len() - body.len() > 3 {
        return None;
    }
    let ch = body.chars().next()?;
    if ch != '`' && ch != '~' {
        return None;
    }
    let len = body.chars().take_while(|&c| c == ch).count();
    if len < 3 {
        return None;
    }
    let info = body[len..].trim();
    (ch != '`' || !info.contains('`')).then_some((ch, len, info))
}

struct OpenDelim {
    ch: char,
    len: usize,
    pos: usize,
}

fn close_hanging(text: &str) -> Option<String> {
    let cs: Vec<(usize, char)> = text.char_indices().collect();
    let n = cs.len();
    let at = |i: usize| cs.get(i).map(|&(_, c)| c);

    let mut delims: Vec<OpenDelim> = Vec::new();
    let mut brackets: Vec<usize> = Vec::new();
    let mut code: Option<(usize, usize)> = None;
    let mut last_content: Option<usize> = None;
    let mut pending_url: Option<usize> = None;

    let mut i = 0;
    while i < n {
        let c = cs[i].1;
        if code.is_none() && c == '\\' {
            if i + 1 < n {
                last_content = Some(i + 1);
            }
            i += 2;
            continue;
        }
        if c == '`' {
            let run = run_len(&cs, i);
            match code {
                Some((open, _)) if run == open => code = None,
                Some(_) => last_content = Some(i + run - 1),
                None => code = Some((run, i + run)),
            }
            i += run;
            continue;
        }
        if code.is_some() {
            last_content = Some(i);
            i += 1;
            continue;
        }
        match c {
            '*' | '_' | '~' => {
                let run = run_len(&cs, i);
                delimiter(&mut delims, &cs, run, i, &mut last_content);
                i += run;
            }
            '[' => {
                brackets.push(i);
                i += 1;
            }
            ']' => {
                if let Some(open) = brackets.pop() {
                    delims.retain(|d| d.pos < open);
                    if at(i + 1) == Some('(') {
                        let mut j = i + 2;
                        let mut depth = 0usize;
                        loop {
                            match at(j) {
                                Some('(') => depth += 1,
                                Some(')') if depth == 0 => break,
                                Some(')') => depth -= 1,
                                Some(_) => {}
                                None => {
                                    pending_url = Some(i);
                                    break;
                                }
                            }
                            j += 1;
                        }
                        if pending_url.is_some() {
                            break;
                        }
                        last_content = Some(j);
                        i = j + 1;
                        continue;
                    }
                }
                last_content = Some(i);
                i += 1;
            }
            c if c.is_whitespace() => i += 1,
            _ => {
                last_content = Some(i);
                i += 1;
            }
        }
    }

    let mut base = text;
    if let Some(close) = pending_url {
        base = &text[..cs[close].0];
    }
    let setext = setext_partial(base);
    let head = match setext.then(|| base.rfind('\n')).flatten() {
        Some(nl) => &base[..nl],
        None => base,
    };
    let end = if code.is_some() || pending_url.is_some() {
        head.trim_end().len()
    } else {
        attach_point(head)
    };

    let mut pending: Vec<(usize, String)> = Vec::new();
    if let Some(close) = pending_url {
        pending.push((close, format!("]({PENDING_LINK_URL})")));
    } else {
        if let Some((ticks, content)) = code
            && last_content.is_some_and(|lc| lc >= content)
        {
            let trailing = head[..end].chars().rev().take_while(|&c| c == '`').count();
            if trailing < ticks {
                pending.push((content, "`".repeat(ticks - trailing)));
            }
        }
        if let Some(&open) = brackets.last()
            && last_content.is_some_and(|lc| lc > open)
        {
            pending.push((open, format!("]({PENDING_LINK_URL})")));
        }
    }
    for d in &delims {
        if last_content.is_some_and(|lc| lc >= d.pos) {
            pending.push((d.pos, d.ch.to_string().repeat(d.len)));
        }
    }
    pending.sort_by_key(|&(pos, _)| Reverse(pos));
    let closers: String = pending.into_iter().map(|(_, s)| s).collect();

    if closers.is_empty() && !setext {
        return None;
    }
    let mut repaired = String::with_capacity(base.len() + closers.len() + 3);
    repaired.push_str(&base[..end]);
    repaired.push_str(&closers);
    repaired.push_str(&base[end..]);
    if setext {
        repaired.push(ZERO_WIDTH_SPACE);
    }
    Some(repaired)
}

fn attach_point(head: &str) -> usize {
    let mut end = head.trim_end().len();
    loop {
        let markers = head[..end]
            .chars()
            .rev()
            .take_while(|&c| c == '*' || c == '_' || c == '~')
            .count();
        if markers == 0 {
            return end;
        }
        let before = end - markers;
        if head[..before]
            .chars()
            .next_back()
            .is_some_and(|c| !c.is_whitespace())
        {
            return end;
        }
        end = head[..before].trim_end().len();
    }
}

fn run_len(cs: &[(usize, char)], i: usize) -> usize {
    let c = cs[i].1;
    cs[i..].iter().take_while(|&&(_, x)| x == c).count()
}

fn delimiter(
    delims: &mut Vec<OpenDelim>,
    cs: &[(usize, char)],
    run: usize,
    i: usize,
    last_content: &mut Option<usize>,
) {
    let c = cs[i].1;
    let end = i + run;
    if c == '~' && run > 2 {
        *last_content = Some(end - 1);
        return;
    }
    let prev = i.checked_sub(1).map(|p| cs[p].1);
    let next = cs.get(end).map(|&(_, c)| c);
    let word = |c: Option<char>| c.is_some_and(char::is_alphanumeric);
    if word(prev) && word(next) && (c == '_' || (c == '*' && run == 1)) {
        *last_content = Some(end - 1);
        return;
    }
    let can_close = prev.is_some_and(|c| !c.is_whitespace());
    let can_open = next.is_some_and(|c| !c.is_whitespace());
    let mut rest = run;
    while can_close
        && rest > 0
        && let Some(k) = delims.iter().rposition(|d| d.ch == c)
    {
        let take = rest.min(delims[k].len);
        delims[k].len -= take;
        rest -= take;
        let keep = if delims[k].len == 0 { k } else { k + 1 };
        delims.truncate(keep);
    }
    if rest == 0 {
        return;
    }
    if can_open && (c != '~' || rest == 2) {
        delims.push(OpenDelim {
            ch: c,
            len: rest,
            pos: end,
        });
    } else {
        *last_content = Some(end - 1);
    }
}

fn setext_partial(text: &str) -> bool {
    let Some(nl) = text.rfind('\n') else {
        return false;
    };
    let last = text[nl + 1..].trim_start();
    let underline = |c: char| !last.is_empty() && last.len() <= 2 && last.chars().all(|x| x == c);
    if !(underline('-') || underline('=')) {
        return false;
    }
    let head = &text[..nl];
    head.lines().next().is_some_and(paragraph_line)
        && head
            .lines()
            .last()
            .is_some_and(|l| !l.trim().is_empty() && paragraph_line(l))
}

fn paragraph_line(line: &str) -> bool {
    let body = line.trim_start();
    if body.starts_with('>') || body.starts_with('#') {
        return false;
    }
    let bullet = body.starts_with("- ") || body.starts_with("* ") || body.starts_with("+ ");
    let ordered = body.find(['.', ')']).is_some_and(|i| {
        i > 0 && body[..i].bytes().all(|b| b.is_ascii_digit()) && body[i + 1..].starts_with(' ')
    });
    !bullet && !ordered
}

#[cfg(test)]
mod tests {
    use super::*;

    const CORPUS: &[&str] = &[
        "Here is **bold** and *em* and `code` and ~~gone~~ text.\n",
        "See [the docs](https://zzmux.sh/docs) for **more**, or `zz ls`.\n",
        "- one **two**\n- three `four`\n\nAfter the *list*.\n",
        "Intro:\n\n```rust\nfn main() { let a = **b; }\n```\n\nOutro **done**.\n",
        "# Title\n\nA ~~struck~~ word and an ![image](https://zzmux.sh/i.png).\n",
        "Steps\n\n1. first\n2. **second**\n\nDone _at last_.\n",
        "A paragraph\n- and a bullet that interrupts it\n- with `code`\n",
        "**outer *inner* rest** and a snake_case_name, 2*3=6.\n",
        "Glob **`*.rs`** and a literal * star, then ~~`a * b`~~.\n",
        "Line one\n\nLine two ends on [a link](https://zzmux.sh/a(b)c)\n",
    ];

    #[track_caller]
    fn mends(input: &str, expected: &str) {
        assert_eq!(mend(input).as_deref(), Some(expected), "{input:?}");
    }

    #[track_caller]
    fn stays(input: &str) {
        assert_eq!(mend(input), None, "{input:?}");
    }

    fn prefixes(doc: &str) -> impl Iterator<Item = &str> {
        (0..=doc.len())
            .filter(|&i| doc.is_char_boundary(i))
            .map(|i| &doc[..i])
    }

    fn keeps_every_char(mended: &str, source: &str) -> bool {
        let mut chars = mended.chars();
        source.chars().all(|c| chars.any(|m| m == c))
    }

    #[test]
    fn complete_text_needs_no_repair() {
        stays("");
        stays("plain words, no markers");
        stays("a **b** and *c* and `d` and ~~e~~");
        stays("[docs](https://zzmux.sh) done");
        for doc in CORPUS {
            stays(doc);
        }
    }

    #[test]
    fn emphasis_closes() {
        mends("**bold", "**bold**");
        mends("some *em", "some *em*");
        mends("a __b", "a __b__");
        mends("a _b", "a _b_");
        mends("***both", "***both***");
        mends("~~gone", "~~gone~~");
    }

    #[test]
    fn half_streamed_closers_complete() {
        mends("**bold*", "**bold**");
        mends("__b_", "__b__");
        mends("~~gone~", "~~gone~~");
    }

    #[test]
    fn nested_closers_come_innermost_first() {
        mends("**a *b", "**a *b***");
        mends("*a **b", "*a **b***");
        mends("_a **b", "_a **b**_");
    }

    #[test]
    fn bare_openers_stay_literal_until_content() {
        stays("**");
        stays("text **");
        stays("text ** ");
        stays("*");
        stays("~~");
        stays("`");
    }

    #[test]
    fn closers_insert_before_trailing_whitespace() {
        mends("**bold ", "**bold** ");
        mends("*em\n", "*em*\n");
    }

    #[test]
    fn isolated_marker_runs_do_not_swallow_the_closer() {
        mends("**outer *", "**outer** *");
        mends("**a **", "**a** **");
        mends("**a * *", "**a** * *");
    }

    #[test]
    fn intraword_markers_and_escapes_stay_literal() {
        stays("2*3 equals 6");
        stays("snake_case_name");
        stays("20~25 degrees");
        stays(r"\*not emphasis");
        stays(r"a \** b");
    }

    #[test]
    fn list_markers_are_not_openers() {
        stays("* item one");
        stays("- a\n* b");
    }

    #[test]
    fn inline_code_closes_and_shields_markers() {
        mends("`code", "`code`");
        mends("call `a ** b", "call `a ** b`");
        mends("``a`", "``a``");
        stays("`a``");
        stays("`done` after");
    }

    #[test]
    fn links_mend_to_pending_sentinel() {
        mends("[docs](https://zzmux.sh/lo", "[docs](zz:pending-link)");
        mends("[docs](", "[docs](zz:pending-link)");
        mends("see [do", "see [do](zz:pending-link)");
        mends("![alt](https://zzmux.sh/i.p", "![alt](zz:pending-link)");
        mends("[a](https://zzmux.sh/(y", "[a](zz:pending-link)");
        stays("see [");
        stays("[x] task-like");
        stays("[a](https://zzmux.sh/(y)) done");
    }

    #[test]
    fn emphasis_and_links_nest() {
        mends("[**a", "[**a**](zz:pending-link)");
        mends("**a [b", "**a [b](zz:pending-link)**");
        mends("**a [b](htt", "**a [b](zz:pending-link)**");
        stays("[**a] done");
    }

    #[test]
    fn setext_partials_get_zero_width_space() {
        mends("para\n-", "para\n-\u{200B}");
        mends("para\n--", "para\n--\u{200B}");
        mends("para\n=", "para\n=\u{200B}");
        mends("**b\n-", "**b**\n-\u{200B}");
        stays("para\n---");
        stays("-");
        stays("\n-");
        stays("- one **two**\n-");
        stays("# Title\n-");
    }

    #[test]
    fn fenced_code_is_skipped() {
        stays("```\n**not bold\n");
        stays("```rust\nlet a = *b;\n");
        stays("~~~\n**x\n");
        stays("intro\n\n```\n**x\n");
        stays("```\ncode\n```");
        mends(
            "```\ncode\n```\nthen **bold",
            "```\ncode\n```\nthen **bold**",
        );
        mends(
            "```\ncode\n```\n\nthen **bold",
            "```\ncode\n```\n\nthen **bold**",
        );
        mends("```x``` and **b", "```x``` and **b**");
    }

    #[test]
    fn indented_code_is_skipped() {
        stays("intro\n\n    let a = **b;");
        stays("intro\n\n\tlet a = **b;");
    }

    #[test]
    fn only_the_last_block_is_mended() {
        stays("**settled\n\ndone.");
        mends("**settled\n\nnow **bold", "**settled\n\nnow **bold**");
        mends("*a\n \n*b", "*a\n \n*b*");
    }

    #[test]
    fn mended_text_parses_as_markup() {
        for (source, plain) in [
            ("**bold", "bold"),
            ("some *em", "some em"),
            ("a __b", "a b"),
            ("call `a ** b", "call "),
            ("~~gone", "gone"),
            ("**a *b", "a b"),
            ("**outer *inner", "outer inner"),
            ("**bold ", "bold"),
            ("see [do", "see do"),
            ("[docs](https://zzmux.sh/lo", "docs"),
            ("**a [b](htt", "a b"),
            ("**outer *", "outer *"),
        ] {
            let mended = mend(source).unwrap_or_else(|| panic!("{source:?} needs a repair"));
            let ast = markdown::to_mdast(&mended, &markdown::ParseOptions::gfm())
                .unwrap_or_else(|e| panic!("{mended:?}: {e}"));
            let mut text = String::new();
            plain_text(&ast, &mut text);
            assert_eq!(text, plain, "{source:?} -> {mended:?}");
        }
    }

    fn plain_text(node: &markdown::mdast::Node, out: &mut String) {
        if let markdown::mdast::Node::Text(text) = node {
            out.push_str(&text.value);
        }
        for child in node.children().into_iter().flatten() {
            plain_text(child, out);
        }
    }

    #[test]
    fn streaming_prefixes_are_idempotent() {
        for doc in CORPUS {
            for prefix in prefixes(doc) {
                let Some(mended) = mend(prefix) else { continue };
                assert_eq!(
                    mend(&mended),
                    None,
                    "not idempotent: {prefix:?} -> {mended:?}"
                );
            }
        }
    }

    #[test]
    fn streaming_prefixes_keep_their_text() {
        for doc in CORPUS {
            for prefix in prefixes(doc) {
                let Some(mended) = mend(prefix) else { continue };
                assert!(
                    mended.contains(PENDING_LINK_URL) || keeps_every_char(&mended, prefix),
                    "dropped text: {prefix:?} -> {mended:?}"
                );
            }
        }
    }

    #[test]
    fn streaming_prefixes_never_repair_to_themselves() {
        for doc in CORPUS {
            for prefix in prefixes(doc) {
                let mended = mend(prefix);
                assert!(
                    mended.as_deref() != Some(prefix),
                    "no-op repair allocates: {prefix:?}"
                );
            }
        }
    }
}
