use super::*;

#[test]
fn browser_metadata_uses_tmux_pane_titles_only() {
    let pane = PaneId(7);
    let session = ala_cef::SessionId(11);
    assert_eq!(
        browser_metadata_command(
            pane,
            &ala_cef::BrowserEvent::AddressChanged {
                session,
                url: Arc::from("https://example.com/path"),
            },
        ),
        None
    );
    assert_eq!(
        browser_metadata_command(
            pane,
            &ala_cef::BrowserEvent::TitleChanged {
                session,
                title: Arc::from("Example Domain"),
            },
        ),
        Some(CommandInvocation::new(
            "select-pane",
            ["-t", "%7", "-T", "Example Domain"],
        ))
    );
    assert_eq!(
        browser_metadata_command(
            pane,
            &ala_cef::BrowserEvent::FrameReady {
                session,
                generation: 1,
            },
        ),
        None
    );
}
