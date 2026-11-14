use std::sync::Arc;

use ala::{
    AnyElement, App, HighlightStyle, InteractiveText, IntoElement, SharedString, StyledText, Window,
};

use zz_client::agent_launch::interactive_shell;
use zz_protocol::{CommandInvocation, PaneId};

use crate::{
    mention::{ref_end, ref_highlight, ref_spans},
    target::{RefTarget, ResolvedTarget, TargetHost, drive_path, resolve_target},
    text::{InlineLink, InlineSource, LinkClick, TextLinks},
};

pub const REF_SCHEME: &str = "zzref:";

pub type RefOpener = Arc<dyn Fn(&ResolvedTarget, &mut Window, &mut App) + Send + Sync>;

#[derive(Clone)]
pub struct ChatRefs {
    pub hosts: Vec<TargetHost>,
    pub cwd: Option<String>,
    pub open: RefOpener,
    pub view: Option<super::preview::ViewOpener>,
}

impl ChatRefs {
    fn resolve(&self, token: &str) -> Option<ResolvedTarget> {
        resolve_target(token, &self.hosts)
    }

    fn resolve_code(&self, code: &str) -> Option<ResolvedTarget> {
        if let Some(token) = code.strip_prefix('@') {
            return (ref_end(code, 0) == code.len())
                .then(|| self.resolve(token))
                .flatten();
        }
        if code.starts_with(['%', '$']) && !code.contains(char::is_whitespace) {
            return self.resolve(code);
        }
        let path = self.code_path(code)?;
        self.resolve(&path)
    }

    fn code_path(&self, code: &str) -> Option<String> {
        if code.contains(char::is_whitespace) || code.contains("://") || !code.contains('/') {
            return None;
        }
        let bare = code.split(':').next().unwrap_or(code);
        let name = bare.trim_end_matches('/').rsplit('/').next()?;
        let file = name.rsplit_once('.').is_some_and(|(stem, extension)| {
            !stem.is_empty() && extension.chars().all(|c| c.is_ascii_alphanumeric())
        });
        if !file && !bare.ends_with('/') {
            return None;
        }
        if code.starts_with(['/', '~']) {
            return Some(code.to_owned());
        }
        let cwd = self.cwd.as_deref()?.trim_end_matches('/');
        let relative = code.strip_prefix("./").unwrap_or(code);
        Some(format!("{cwd}/{relative}"))
    }

    pub fn resolve_link(&self, url: &str) -> Option<ResolvedTarget> {
        if let Some(token) = url.strip_prefix(REF_SCHEME) {
            return self.resolve_code(token);
        }
        let path = self.link_path(url.trim())?;
        self.resolve(&path.replace('\\', "\\\\"))
    }

    fn link_path(&self, url: &str) -> Option<String> {
        let local = match strip_file_scheme(url) {
            Some(rest) => {
                let rest = rest.strip_prefix("localhost").unwrap_or(rest);
                let rest = rest.strip_prefix('/').unwrap_or(rest);
                if drive_path(rest) {
                    rest.to_owned()
                } else {
                    format!("/{rest}")
                }
            }
            None if has_scheme(url) => return None,
            None => url.to_owned(),
        };
        let (local, fragment) = local
            .split_once('#')
            .map_or((local.as_str(), None), |(path, fragment)| {
                (path, Some(fragment))
            });
        let line = fragment
            .and_then(|fragment| fragment.strip_prefix('L'))
            .map(|line| {
                line.split(|c: char| !c.is_ascii_digit())
                    .next()
                    .unwrap_or("")
            })
            .filter(|line| !line.is_empty());
        let path = percent_decode(local);
        if path.is_empty() {
            return None;
        }
        let path = if path.starts_with(['/', '~']) || drive_path(&path) {
            path
        } else {
            let cwd = self.cwd.as_deref()?.trim_end_matches('/');
            let relative = path.strip_prefix("./").unwrap_or(&path);
            format!("{cwd}/{relative}")
        };
        Some(match line {
            Some(line) => format!("{path}:{line}"),
            None => path,
        })
    }
}

fn strip_file_scheme(url: &str) -> Option<&str> {
    let (scheme, rest) = url.split_at_checked(7)?;
    scheme.eq_ignore_ascii_case("file://").then_some(rest)
}

fn has_scheme(url: &str) -> bool {
    let Some((scheme, rest)) = url.split_once(':') else {
        return false;
    };
    let mut characters = scheme.chars();
    let named = characters.next().is_some_and(|c| c.is_ascii_alphabetic())
        && characters.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    let line = rest.split(':').next().unwrap_or(rest);
    let file_line =
        scheme.contains('.') && !line.is_empty() && line.chars().all(|c| c.is_ascii_digit());
    named && !drive_path(url) && !file_line
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let escape = bytes
            .get(index + 1..index + 3)
            .filter(|pair| pair.iter().all(u8::is_ascii_hexdigit))
            .and_then(|pair| std::str::from_utf8(pair).ok())
            .and_then(|pair| u8::from_str_radix(pair, 16).ok());
        match (bytes[index], escape) {
            (b'%', Some(byte)) => {
                decoded.push(byte);
                index += 3;
            }
            (byte, _) => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(decoded).unwrap_or_else(|_| text.to_owned())
}

#[must_use]
pub fn ref_links(refs: &ChatRefs, text: &str, source: InlineSource) -> Vec<InlineLink> {
    match source {
        InlineSource::Text => ref_spans(text)
            .into_iter()
            .filter_map(|span| {
                let target = refs.resolve(&text[span.start + 1..span.end])?;
                Some(InlineLink {
                    url: format!("{REF_SCHEME}{}", &text[span.clone()]),
                    label: target.label,
                    range: span,
                })
            })
            .collect(),
        InlineSource::Code => {
            let code = text.trim();
            refs.resolve_code(code)
                .map(|target| InlineLink {
                    range: 0..text.len(),
                    label: target.label,
                    url: format!("{REF_SCHEME}{code}"),
                })
                .into_iter()
                .collect()
        }
    }
}

pub struct RefLinks {
    refs: Arc<ChatRefs>,
}

impl RefLinks {
    #[must_use]
    pub fn new(refs: Arc<ChatRefs>) -> Self {
        Self { refs }
    }
}

impl TextLinks for RefLinks {
    fn style(&self, url: &str, cx: &App) -> Option<HighlightStyle> {
        url.starts_with(REF_SCHEME).then(|| ref_highlight(cx))
    }

    fn click(&self, click: &LinkClick, window: &mut Window, cx: &mut App) -> bool {
        let Some(target) = self.refs.resolve_link(&click.url) else {
            return false;
        };
        (self.refs.open)(&target, window, cx);
        true
    }
}

#[must_use]
pub fn ref_text(
    id: impl Into<ala::ElementId>,
    text: &str,
    refs: &ChatRefs,
    cx: &App,
) -> Option<AnyElement> {
    let mut shown = String::with_capacity(text.len());
    let mut ranges = Vec::new();
    let mut targets = Vec::new();
    let mut last = 0;
    for span in ref_spans(text) {
        let Some(target) = refs.resolve(&text[span.start + 1..span.end]) else {
            continue;
        };
        shown.push_str(&text[last..span.start]);
        let start = shown.len();
        shown.push_str(&target.label);
        ranges.push(start..shown.len());
        targets.push(target);
        last = span.end;
    }
    if targets.is_empty() {
        return None;
    }
    shown.push_str(&text[last..]);
    let style = ref_highlight(cx);
    let highlights = ranges
        .iter()
        .map(|range| (range.clone(), style))
        .collect::<Vec<_>>();
    let open = Arc::clone(&refs.open);
    Some(
        InteractiveText::new(
            id,
            StyledText::new(SharedString::from(shown)).with_highlights(highlights),
        )
        .on_click(ranges, move |index, window, cx| {
            if let Some(target) = targets.get(index) {
                open(target, window, cx);
            }
        })
        .into_any_element(),
    )
}

const FLOAT_WIDTH: &str = "80%";
const FLOAT_HEIGHT: &str = "75%";

fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

#[must_use]
pub fn open_path(target: &RefTarget, anchor: Option<PaneId>) -> Option<CommandInvocation> {
    let RefTarget::Path { path, line } = target else {
        return None;
    };
    let trimmed = path.trim_end_matches('/');
    let (dir, file) = match trimmed.rsplit_once('/') {
        _ if path.ends_with('/') || trimmed.is_empty() || trimmed == "~" => (path.as_str(), None),
        Some((parent, name)) => (if parent.is_empty() { "/" } else { parent }, Some(name)),
        None => (".", Some(trimmed)),
    };
    let title = file.unwrap_or_else(|| trimmed.rsplit('/').next().unwrap_or(trimmed));
    let mut arguments = anchor
        .map(|anchor| vec!["-t".to_owned(), anchor.to_string()])
        .unwrap_or_default();
    arguments.extend(
        [
            "-x",
            FLOAT_WIDTH,
            "-y",
            FLOAT_HEIGHT,
            "-c",
            dir,
            "-T",
            title,
        ]
        .map(str::to_owned),
    );
    if let Some(file) = file {
        let jump = line.map_or_else(String::new, |line| format!("+{line} "));
        arguments.extend(["--existing".to_owned(), path.clone()]);
        arguments.extend(interactive_shell(&[
            "sh".to_owned(),
            "-c".to_owned(),
            format!(
                "exec \"${{VISUAL:-${{EDITOR:-vi}}}}\" {jump}{}",
                shell_quote(file)
            ),
        ]));
    }
    Some(CommandInvocation::new("new-pane", arguments))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use zz_protocol::{PaneId, SessionId, WindowId};

    use super::open_path;

    use super::{ChatRefs, ref_links, ref_spans};
    use crate::text::{InlineLink, InlineSource};
    use penna::blocks::palette::PaletteStatus;

    use crate::{
        IconName,
        command::PaletteHostId,
        target::{RefTarget, TargetHost, TargetPane, TargetSession, TargetWindow},
    };

    fn refs() -> ChatRefs {
        ChatRefs {
            hosts: vec![TargetHost {
                id: PaletteHostId(0),
                name: "macbook".to_owned(),
                status: PaletteStatus::Online,
                here: true,
                sessions: vec![TargetSession {
                    id: SessionId(0),
                    name: "zebrilus 0".to_owned(),
                    windows: vec![TargetWindow {
                        id: WindowId(1),
                        index: 1,
                        name: "bash".to_owned(),
                        panes: vec![TargetPane {
                            id: PaneId(0),
                            title: String::new(),
                            program: Some("bash".into()),
                            icon: IconName::SquareTerminal,
                            chat: None,
                        }],
                    }],
                }],
            }],
            cwd: Some("/repo".to_owned()),
            open: Arc::new(|_, _, _| {}),
            view: None,
        }
    }

    #[test]
    fn spans_skip_mail_addresses_and_trailing_punctuation() {
        let text = "see @macbook:$0:1.%0, mail me@host.com or (@zebrilus\\ 0).";
        let spans = ref_spans(text)
            .into_iter()
            .map(|span| &text[span])
            .collect::<Vec<_>>();
        assert_eq!(spans, ["@macbook:$0:1.%0", "@zebrilus\\ 0"]);
    }

    #[test]
    fn refs_in_prose_and_code_link_to_their_labels() {
        let refs = refs();
        let text = "pane @macbook:$0:1.%0 and @claude";
        assert_eq!(
            ref_links(&refs, text, InlineSource::Text),
            [InlineLink {
                range: 5..21,
                label: "bash · zebrilus 0:1".to_owned(),
                url: "zzref:@macbook:$0:1.%0".to_owned(),
            }]
        );
        assert_eq!(
            ref_links(&refs, "crates/a.rs:4", InlineSource::Code),
            [InlineLink {
                range: 0..13,
                label: "a.rs:4".to_owned(),
                url: "zzref:crates/a.rs:4".to_owned(),
            }]
        );
        assert_eq!(
            refs.resolve_code("crates/a.rs:4")
                .map(|target| target.target),
            Some(RefTarget::Path {
                path: "/repo/crates/a.rs".to_owned(),
                line: Some(4)
            })
        );
        assert!(ref_links(&refs, "main", InlineSource::Code).is_empty());
        assert_eq!(
            refs.resolve_code("@macbook:$0:1.%0")
                .map(|target| target.label),
            Some("bash · zebrilus 0:1".to_owned())
        );
    }

    #[test]
    fn markdown_link_paths_resolve_to_files_and_web_links_do_not() {
        let refs = refs();
        let path = |path: &str, line: Option<u32>| {
            Some(RefTarget::Path {
                path: path.to_owned(),
                line,
            })
        };
        let cases = [
            (
                "/Users/u/p/evidence.png",
                path("/Users/u/p/evidence.png", None),
            ),
            ("/abs/a.rs:12", path("/abs/a.rs", Some(12))),
            ("/abs/a.rs:12:5", path("/abs/a.rs", Some(12))),
            ("crates/a.rs", path("/repo/crates/a.rs", None)),
            ("./a.rs", path("/repo/a.rs", None)),
            ("crates/a.rs:12", path("/repo/crates/a.rs", Some(12))),
            ("a.rs", path("/repo/a.rs", None)),
            ("a.rs:12", path("/repo/a.rs", Some(12))),
            ("~/notes.md", path("~/notes.md", None)),
            ("file:///abs/x.png", path("/abs/x.png", None)),
            ("file:///abs/x.rs:12", path("/abs/x.rs", Some(12))),
            ("file://localhost/abs/x", path("/abs/x", None)),
            ("/a%20b.md", path("/a b.md", None)),
            ("/a b.md", path("/a b.md", None)),
            ("/abs/a.rs#L12", path("/abs/a.rs", Some(12))),
            ("C:/src/a.rs", path("C:/src/a.rs", None)),
            ("C:\\src\\a.rs:3", path("C:\\src\\a.rs", Some(3))),
            ("https://zzmux.sh/install.sh", None),
            ("http://localhost:8080/a.rs", None),
            ("mailto:me@host.com", None),
            ("#section", None),
        ];
        for (url, expected) in cases {
            assert_eq!(
                refs.resolve_link(url).map(|target| target.target),
                expected,
                "{url}"
            );
        }
    }

    #[ala::test]
    fn clicking_a_ref_link_opens_its_target(cx: &mut ala::TestAppContext) {
        use std::sync::Mutex;

        use ala::{Modifiers, MouseButton};

        use crate::text::{LinkClick, TextLinks};

        let opened = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&opened);
        let refs = ChatRefs {
            open: Arc::new(move |target, _, _| seen.lock().unwrap().push(target.label.clone())),
            ..refs()
        };
        let url = ref_links(&refs, "see @macbook:$0:1.%0", InlineSource::Text)[0]
            .url
            .clone();
        let links = super::RefLinks::new(Arc::new(refs));
        let window = cx.add_empty_window();
        let clicked = window.update(|window, cx| {
            links.click(
                &LinkClick {
                    url: url.into(),
                    button: MouseButton::Left,
                    modifiers: Modifiers::default(),
                },
                window,
                cx,
            )
        });
        assert!(clicked);
        assert_eq!(*opened.lock().unwrap(), ["bash · zebrilus 0:1"]);
    }

    #[cfg(unix)]
    #[test]
    #[allow(
        clippy::disallowed_methods,
        reason = "the test runs the command line a pane would run, in a plain shell"
    )]
    fn files_open_in_the_editor_from_an_interactive_shell() {
        let command = open_path(
            &RefTarget::Path {
                path: "~/src/it's.rs".to_owned(),
                line: Some(3),
            },
            Some(PaneId(4)),
        )
        .expect("a file opens");
        let arguments = command
            .args
            .iter()
            .map(|argument| argument.as_str().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            arguments[..12],
            [
                "-t",
                "%4",
                "-x",
                "80%",
                "-y",
                "75%",
                "-c",
                "~/src",
                "-T",
                "it's.rs",
                "--existing",
                "~/src/it's.rs"
            ]
        );
        let home = std::env::temp_dir();
        let output = std::process::Command::new(&arguments[12])
            .args(&arguments[13..])
            .env_clear()
            .env("SHELL", "/bin/sh")
            .env("EDITOR", "echo")
            .env("HOME", &home)
            .output()
            .expect("the shell runs");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "+3 it's.rs\n");
    }
}
