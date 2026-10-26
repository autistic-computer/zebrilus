use super::*;
use crate::target::tests::hosts;
use zz_protocol::{PaneId, SessionId, WindowId};

fn link(address: &str) -> TargetLink {
    resolve_link(address, &hosts()).unwrap_or_else(|| panic!("{address} is not a link"))
}

#[test]
fn a_link_names_what_it_points_at_on_its_host() {
    let host = link("@ubuntu");
    assert_eq!((host.kind, host.state), (TargetKind::Host, LinkState::Live));
    assert_eq!(host.detail, "2 sessions");

    let session = link("@ubuntu:work");
    assert_eq!(session.target, Some(PaletteTarget::Session(SessionId(3))));
    let window = link("@ubuntu:work:2");
    assert_eq!(window.target, Some(PaletteTarget::Window(WindowId(8))));
    assert_eq!(window.detail, "agents · 2 panes");
    assert_eq!(
        link("@ubuntu:work:agents").target,
        Some(PaletteTarget::Window(WindowId(8)))
    );
    assert_eq!(
        link("@ubuntu:work:2.%13").target,
        Some(PaletteTarget::Pane(PaneId(13)))
    );
    assert_eq!(
        link("@ubuntu:work:2.1").target,
        Some(PaletteTarget::Pane(PaneId(13)))
    );
    let pane = link("@ubuntu:%12");
    assert_eq!(pane.target, Some(PaletteTarget::Pane(PaneId(12))));
    assert_eq!(pane.detail, "work:2 · Claude Code · fix tests");
    assert_eq!(
        link(r"@ubuntu:my\ notes").target,
        Some(PaletteTarget::Session(SessionId(4)))
    );
}

#[test]
fn a_link_without_a_host_means_the_host_here() {
    let window = link("@main:1");
    assert_eq!(window.host, "macbook");
    assert_eq!(window.target, Some(PaletteTarget::Window(WindowId(2))));
    assert_eq!(link("@%5").target, Some(PaletteTarget::Pane(PaneId(5))));
    let file = link("@~/dev/zz/src/main.rs:42:7");
    assert_eq!(
        (file.host.as_str(), file.kind),
        ("macbook", TargetKind::File)
    );
    assert_eq!(file.path.as_deref(), Some("~/dev/zz/src/main.rs"));
    assert_eq!((file.line, file.name.as_str()), (Some(42), "main.rs:42"));
    assert_eq!(link("@ubuntu:/srv/logs/").kind, TargetKind::Dir);
    assert_eq!(resolve_link("ubuntu", &hosts()), None);
}

#[test]
fn a_link_mutes_offline_and_strikes_through_once_gone() {
    assert_eq!(link("@xps").state, LinkState::Offline);
    assert_eq!(link("@xps:work:1").state, LinkState::Offline);
    let gone = link("@ubuntu:work:9");
    assert_eq!(
        (gone.kind, gone.state),
        (TargetKind::Window, LinkState::Gone)
    );
    assert_eq!(gone.detail, "Not on ubuntu any more");
    assert_eq!(link("@ubuntu:%99").state, LinkState::Gone);
    assert_eq!(link("@nope:1").state, LinkState::Gone);
}

#[test]
fn a_file_link_pastes_its_path_with_the_host_when_it_is_elsewhere() {
    let file = link(r"@ubuntu:~/My\ Files/a.rs:3");
    assert_eq!(
        file.paste_text(false).as_deref(),
        Some(r"ubuntu:~/My\ Files/a.rs ")
    );
    assert_eq!(file.paste_text(true).as_deref(), Some(r"~/My\ Files/a.rs "));
    assert_eq!(link("@ubuntu:work").paste_text(true), None);
}

#[test]
fn bare_mentions_in_prose_become_links_and_code_stays_code() {
    let linked = autolink(
        "Build broke in @ubuntu:work:2. See `@ubuntu:work:1` and [vim](@ubuntu:work:1), \
         mail fab@ubuntu.dev, ask @here or @main.",
        &hosts(),
    );
    assert_eq!(
        linked.as_deref(),
        Some(
            "Build broke in [\\@ubuntu\\:work\\:2](<@ubuntu:work:2>). See `@ubuntu:work:1` and \
             [vim](@ubuntu:work:1), mail fab@ubuntu.dev, ask @here or [\\@main](<@main>)."
        )
    );
    assert_eq!(autolink("no mentions here", &hosts()), None);
    assert_eq!(autolink("```\n@ubuntu\n```", &hosts()), None);
    assert_eq!(
        autolink("**@xps** is down", &hosts()).as_deref(),
        Some("**[\\@xps](<@xps>)** is down")
    );
}

#[test]
fn your_messages_keep_their_text_as_typed_around_the_links() {
    let linked = autolink_plain("look at @ubuntu:work:2\n  *now* <b>\n\nthanks", &hosts());
    assert_eq!(
        linked.as_deref(),
        Some(
            "look at [\\@ubuntu\\:work\\:2](<@ubuntu:work:2>)\\\n&#32;&#32;\\*now\\* \\<b\\>\n\nthanks"
        )
    );
    assert_eq!(autolink_plain("plain *text*", &hosts()), None);
}

fn parsed_links(markdown: &str) -> Vec<(String, String)> {
    fn walk(node: &markdown::mdast::Node, out: &mut Vec<(String, String)>) {
        if let markdown::mdast::Node::Link(link) = node {
            out.push((node.to_string(), link.url.clone()));
        }
        for child in node.children().into_iter().flatten() {
            walk(child, out);
        }
    }
    let mut out = Vec::new();
    walk(
        &markdown::to_mdast(markdown, &markdown::ParseOptions::gfm()).unwrap(),
        &mut out,
    );
    out
}

#[test]
fn autolinked_text_parses_back_to_links_to_the_mentions() {
    let mention = |text: &str| (text.to_owned(), text.to_owned());
    assert_eq!(
        parsed_links(&autolink("on @ubuntu:work:2 and @~/dev/a_b.rs:3", &hosts()).unwrap()),
        [mention("@ubuntu:work:2"), mention("@~/dev/a_b.rs:3")]
    );
    assert_eq!(
        parsed_links(&autolink_plain("(@ubuntu) *x*\n@xps:/tmp/", &hosts()).unwrap()),
        [mention("@ubuntu"), mention("@xps:/tmp/")]
    );
}

mod clicks {
    use std::{cell::RefCell, rc::Rc};

    use zz_gpui::{
        Context, InteractiveElement as _, IntoElement, Modifiers, MouseButton, Render,
        TestAppContext, VisualTestContext, Window, div, point,
    };

    use super::*;
    use crate::text::TextView;

    struct Thread {
        text: &'static str,
        seen: Rc<RefCell<Vec<String>>>,
    }

    impl Render for Thread {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let ink = LinkInk {
                live: cx.theme().accent,
                muted: cx.theme().foreground,
            };
            let (open, mention) = (self.seen.clone(), self.seen.clone());
            div()
                .w(px(300.0))
                .on_action(move |link: &OpenLink, _, _| {
                    open.borrow_mut().push(format!("open {}", link.address));
                })
                .on_action(move |link: &MentionLink, _, _| {
                    mention
                        .borrow_mut()
                        .push(format!("mention {}", link.address));
                })
                .child(
                    TextView::markdown("thread", self.text)
                        .links(TargetLinks::new(hosts().into(), ink)),
                )
        }
    }

    fn thread<'a>(
        text: &'static str,
        cx: &'a mut TestAppContext,
    ) -> (Rc<RefCell<Vec<String>>>, &'a mut VisualTestContext) {
        cx.update(crate::init);
        let seen = Rc::new(RefCell::new(Vec::new()));
        let (_, cx) = cx.add_window_view({
            let seen = seen.clone();
            move |window, cx| {
                let thread = cx.new(|_| Thread { text, seen });
                crate::Root::new(thread, window, cx)
            }
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        (seen, cx)
    }

    #[zz_gpui::test]
    fn a_click_goes_there_and_the_platform_key_mentions_it(cx: &mut TestAppContext) {
        let (seen, cx) = thread("[the agents](@ubuntu:work:2)", cx);
        let at = point(px(8.0), px(8.0));
        cx.simulate_click(at, Modifiers::default());
        cx.run_until_parked();
        cx.simulate_click(at, Modifiers::secondary_key());
        cx.run_until_parked();
        assert_eq!(
            *seen.borrow(),
            ["open @ubuntu:work:2", "mention @ubuntu:work:2"]
        );
        assert_eq!(cx.opened_url(), None);
    }

    #[zz_gpui::test]
    fn a_right_click_copies_the_link(cx: &mut TestAppContext) {
        let (seen, cx) = thread("[the agents](@ubuntu:work:2)", cx);
        let at = point(px(8.0), px(8.0));
        cx.simulate_mouse_down(at, MouseButton::Right, Modifiers::default());
        cx.simulate_mouse_up(at, MouseButton::Right, Modifiers::default());
        cx.run_until_parked();
        assert!(seen.borrow().is_empty());
        assert_eq!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref(),
            Some("@ubuntu:work:2")
        );
    }

    #[zz_gpui::test]
    fn web_links_still_open_in_the_browser(cx: &mut TestAppContext) {
        let (seen, cx) = thread("[docs](https://zzmux.sh)", cx);
        cx.simulate_click(point(px(8.0), px(8.0)), Modifiers::default());
        cx.run_until_parked();
        assert!(seen.borrow().is_empty());
        assert_eq!(cx.opened_url().as_deref(), Some("https://zzmux.sh"));
    }
}
