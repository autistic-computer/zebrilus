use super::*;

pub(super) const fn sent_attention(state: PeerState) -> Attention {
    match state {
        PeerState::Working | PeerState::Asking | PeerState::Answered => Attention::Working,
        PeerState::Replied => Attention::Done,
        PeerState::Failed => Attention::Failed,
        PeerState::Stopped | PeerState::Closed => Attention::None,
    }
}

pub(super) const fn note_attention(state: PeerState) -> Attention {
    match state {
        PeerState::Asking => Attention::NeedsYou,
        PeerState::Replied => Attention::Done,
        PeerState::Failed => Attention::Failed,
        PeerState::Working | PeerState::Answered | PeerState::Stopped | PeerState::Closed => {
            Attention::None
        }
    }
}

pub(super) fn peer_glyph(glyph: Icon, touch: bool, cx: &App) -> Div {
    peer_mark(
        glyph,
        penna::blocks::chat::ACTIVITY_ROW_FONT_SIZE,
        touch,
        cx,
    )
}

pub(super) fn peer_end(glyph: Icon, touch: bool, cx: &App) -> Div {
    peer_mark(glyph, ACTIVITY_DISCLOSURE_SIZE, touch, cx)
}

fn peer_mark(glyph: Icon, desktop: f32, touch: bool, cx: &App) -> Div {
    let (font, size) = if touch {
        (
            TextStyle::TouchSmall.design_px(),
            penna::blocks::chat::TOUCH_ACTIVITY_GLYPH,
        )
    } else {
        (penna::blocks::chat::ACTIVITY_ROW_FONT_SIZE, desktop)
    };
    div()
        .flex_none()
        .relative()
        .top(crate::icon::cap_center_drop(px(font), px(size), cx))
        .child(glyph.size(px(size)))
}

pub(super) fn peer_text(text: &str, shrink: f32) -> Option<Div> {
    (!text.is_empty()).then(|| {
        div()
            .min_w_0()
            .flex_shrink(shrink)
            .overflow_hidden()
            .text_ellipsis()
            .whitespace_nowrap()
            .child(text.to_owned())
    })
}

pub(super) fn note_face(note: &PeerNote, hooks: &ChatHooks<'_>) -> PeerFace {
    let command = PeerFace {
        title: "a command".to_owned(),
        program: None,
        glyph: IconName::Terminal,
        target: None,
    };
    match (&note.peer, &note.message) {
        (Some(peer), PeerMessage::Sent { .. }) if peer.agent.is_none() => PeerFace {
            glyph: IconName::Terminal,
            ..peer_face(peer, hooks)
        },
        (Some(peer), _) => peer_face(peer, hooks),
        (
            None,
            PeerMessage::Sent {
                host: Some(host), ..
            },
        ) => PeerFace {
            title: format!("a command · {host}"),
            ..command
        },
        (None, _) => command,
    }
}
