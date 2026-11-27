use super::*;

#[test]
fn the_infocmp_reader_finds_the_extended_section() {
    let Some(entries) = terminfo_entries("xterm-256color") else {
        return;
    };
    assert!(entries.iter().any(|entry| entry.starts_with("AX=")));
    assert!(entries.iter().any(|entry| entry.starts_with("colors=256")));
    assert!(
        entries
            .iter()
            .any(|entry| entry.starts_with("clear=\u{1b}["))
    );
    let term = client_terminal_facts(
        "xterm-256color",
        Some("truecolor"),
        "",
        &["xterm*:clipboard:ccolour:cstyle:focus:title".to_owned()],
        &[],
    )
    .expect("term");
    assert!(term.has_capability("smcup"));
    assert!(term.has_feature("RGB"));
    assert!(term.has_feature("bpaste"));
}

#[test]
fn the_stock_arrays_move_the_other_terms_the_way_the_pin_does() {
    let engine = zz_mux::MuxEngine::default();
    let features = engine.terminal_features_option();
    let overrides = engine.terminal_overrides_option();
    let facts = |term: &str| client_terminal_facts(term, None, "", &features, &overrides);
    if let Some(screen) = facts("screen-256color") {
        assert!(screen.has_feature("bpaste"));
        assert!(screen.has_feature("focus"));
        assert!(screen.has_capability("Enfcs"));
        assert!(!screen.has_capability("XT"));
    }
    if let Some(tmux) = facts("tmux-256color") {
        assert!(tmux.has_feature("bpaste"));
        assert!(tmux.has_feature("focus"));
        assert!(tmux.has_capability("Enfcs"));
    }
    if let Some(linux) = facts("linux") {
        assert!(linux.has_feature("title"));
        assert!(!linux.has_capability("AX"));
    }
    if let Some(rxvt) = facts("rxvt") {
        assert!(!rxvt.has_capability("kf1"));
        assert!(!rxvt.has_capability("kf63"));
    }
}

#[test]
fn the_infocmp_reader_speaks_both_ncurses_generations() {
    let entries = parse_infocmp_entries(
        "#\tReconstructed via infocmp\nxterm-256color|xterm with 256 colors,\n\tam,\n\tcolors#0x100,\n\tpairs#65536,\n\tclear=\\E[H\\E[2J,\n\tkf1=\\EOP,\n\tbel=^G,\n\tacsc=``aaffggiijjkk\\,\\:\\^\\\\,\n\tsmcup@,\n\tU8#1,\n\tSs=\\E[%p1%d q$<5>,\n",
    );
    assert_eq!(
        entries,
        [
            "xterm-256color|xterm with 256 colors=1",
            "am=1",
            "colors=256",
            "pairs=65536",
            "clear=\u{1b}[H\u{1b}[2J",
            "kf1=\u{1b}OP",
            "bel=\u{7}",
            "acsc=``aaffggiijjkk,:^\\",
            "U8=1",
            "Ss=\u{1b}[%p1%d q$<5>",
        ]
    );
    let term = TtyTerm::create(
        "xterm-256color",
        &entries,
        None,
        "",
        &[],
        &["linux*:AX@".to_owned()],
    );
    assert!(term.has_capability("colors"));
    assert!(term.has_feature("bpaste"));
    assert!(!term.has_capability("smcup"));
}
