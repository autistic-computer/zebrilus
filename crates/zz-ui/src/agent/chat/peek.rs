use super::*;

pub(super) fn child_peek(
    card: &ChildCard,
    chat: Option<&ChatView>,
    host: &str,
    now: u64,
    phase: f32,
    ground: penna::Ground,
    cx: &App,
) -> Option<AnyElement> {
    let lines = card.peek(chat);
    if lines.is_empty() {
        return None;
    }
    let live = chat
        .and_then(|chat| chat.turns.last())
        .map(|turn| chat_rows::duration(now.saturating_sub(turn.started_at)));
    let full = lines.len() == PEEK_LINES;
    let rows = v_flex().ml(px(PEEK_INDENT)).mb(px(8.0)).children(
        lines
            .iter()
            .map(|line| peek_line(line, host, live.as_deref(), phase, ground, cx)),
    );
    Some(
        FoldFade {
            child: rows.into_any_element(),
            fade: full.then_some(px(PEEK_FADE)),
            from_top: true,
        }
        .into_any_element(),
    )
}

fn peek_line(
    line: &PeekLine,
    host: &str,
    elapsed: Option<&str>,
    phase: f32,
    ground: penna::Ground,
    cx: &App,
) -> Div {
    let theme = cx.theme();
    let glyph = match &line.mark {
        PeekMark::Step(kind) => Some(Icon::new(kind_icon(*kind))),
        PeekMark::Running => Some(penna::blocks::chat::spinner(phase)),
        PeekMark::Failed(kind) => Some(Icon::new(kind_icon(*kind)).text_color(theme.danger)),
        PeekMark::Declined(kind) | PeekMark::Asking(kind) => {
            Some(Icon::new(kind_icon(*kind)).text_color(theme.warning))
        }
        PeekMark::Error => Some(Icon::new(IconName::TriangleAlert).text_color(theme.danger)),
        PeekMark::Live => None,
    };
    let small = px(TextStyle::Small.design_px());
    let glyph = glyph.map(|glyph| {
        div()
            .flex_none()
            .relative()
            .top(crate::icon::cap_center_drop(
                small,
                px(ACTIVITY_DISCLOSURE_SIZE),
                cx,
            ))
            .child(glyph.size(px(ACTIVITY_DISCLOSURE_SIZE)))
    });
    let is_live = line.mark == PeekMark::Live;
    let text = div()
        .min_w_0()
        .overflow_hidden()
        .text_ellipsis()
        .text_color(if is_live {
            theme.foreground.muted()
        } else {
            theme.foreground
        })
        .when(line.code, |text| text.type_style_mono(TextStyle::Small, cx))
        .child(line.label.clone());
    let verdict = line.verdict.as_ref().map(|verdict| {
        let (said, tone) = match verdict {
            Verdict::Allowed => ("allowed".to_owned(), Tone::Success),
            Verdict::Denied => ("denied".to_owned(), Tone::Warning),
            Verdict::Picked(label) => (format!("{label} picked"), Tone::Neutral),
            Verdict::Answered => ("answered".to_owned(), Tone::Neutral),
        };
        Badge::new(format!("{said} by {host}"))
            .tone(tone)
            .small()
            .on(ground)
    });
    h_flex()
        .flex_none()
        .w_full()
        .h(px(PEEK_LINE))
        .gap_2()
        .items_center()
        .min_w_0()
        .whitespace_nowrap()
        .type_style(TextStyle::Small, cx)
        .line_height(px(CHILD_LINE))
        .text_color(theme.foreground.muted())
        .children(glyph)
        .child(text)
        .children(line.detail.clone().map(|detail| {
            div()
                .min_w_0()
                .flex_shrink(1000.0)
                .overflow_hidden()
                .text_ellipsis()
                .child(detail)
        }))
        .when_some(elapsed.filter(|_| is_live), |row, time| {
            row.child(
                div()
                    .flex_none()
                    .text_color(theme.foreground.faint())
                    .child(time.to_owned()),
            )
        })
        .when(matches!(line.mark, PeekMark::Asking(_)), |row| {
            row.child(
                h_flex()
                    .flex_none()
                    .ml_auto()
                    .gap(px(6.0))
                    .items_center()
                    .child(penna::blocks::chat::spinner(phase).size(px(ACTIVITY_DISCLOSURE_SIZE)))
                    .child(format!("waiting for {host}")),
            )
        })
        .children(verdict.map(|badge| div().flex_none().ml_auto().child(badge)))
}
