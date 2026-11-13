use penna::blocks::touch::StatusMark;
use zz_client::window_summary::{Attention, Tone, WindowSummary};

use super::overview::HostState;

pub fn host_mark(state: HostState, waiting: usize) -> (StatusMark, String) {
    match state {
        HostState::Connected if waiting > 0 => {
            (StatusMark::Warning, format!("{waiting} waiting for you"))
        }
        HostState::Connected => (StatusMark::Success, "Connected".to_owned()),
        HostState::Connecting => (StatusMark::Spinner, "Connecting".to_owned()),
        HostState::Offline => (StatusMark::Off, "Not connected".to_owned()),
        HostState::Failed => (StatusMark::Danger, "Failed".to_owned()),
    }
}

pub fn window_mark(summary: &WindowSummary) -> (Option<StatusMark>, Option<String>) {
    let lead = summary.detail.first();
    let says = |tone| {
        lead.filter(|part| part.tone == tone)
            .map(|part| part.text.clone())
    };
    if let Some(word) = says(Tone::Danger) {
        return (Some(StatusMark::Danger), Some(word));
    }
    if let Some(word) = says(Tone::Warning) {
        return (Some(StatusMark::Warning), Some(word));
    }
    let mark = match summary.attention {
        Attention::NeedsYou => Some(StatusMark::Warning),
        Attention::Failed => Some(StatusMark::Danger),
        Attention::Working => Some(StatusMark::Spinner),
        Attention::Done | Attention::Bell => Some(StatusMark::Ink),
        Attention::None => None,
    };
    (mark, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zz_client::window_summary::DetailPart;
    use zz_protocol::{PaneId, WindowId};

    fn summary(attention: Attention, detail: &[(&str, Tone)]) -> WindowSummary {
        WindowSummary {
            window: WindowId(1),
            index: 1,
            headline: PaneId(1),
            title: "zsh".to_owned(),
            detail: detail
                .iter()
                .map(|(text, tone)| DetailPart {
                    text: (*text).to_owned(),
                    tone: *tone,
                })
                .collect(),
            changed_at: 0,
            tiles: Vec::new(),
            attention,
        }
    }

    #[test]
    fn a_window_says_words_only_when_it_asks_or_broke() {
        assert_eq!(
            window_mark(&summary(
                Attention::NeedsYou,
                &[
                    ("Needs approval", Tone::Warning),
                    (" · rm -rf", Tone::Muted)
                ]
            )),
            (Some(StatusMark::Warning), Some("Needs approval".to_owned()))
        );
        assert_eq!(
            window_mark(&summary(Attention::None, &[("Exited 1", Tone::Danger)])),
            (Some(StatusMark::Danger), Some("Exited 1".to_owned()))
        );
        assert_eq!(
            window_mark(&summary(
                Attention::Working,
                &[("Running cargo test", Tone::Muted)]
            )),
            (Some(StatusMark::Spinner), None)
        );
        assert_eq!(
            window_mark(&summary(Attention::Done, &[("Done", Tone::Normal)])),
            (Some(StatusMark::Ink), None)
        );
        assert_eq!(
            window_mark(&summary(
                Attention::None,
                &[("zebrilus · main", Tone::Muted)]
            )),
            (None, None)
        );
    }
}
