use super::*;

fn pointer_sample() -> Vec<u8> {
    let long_word = format!("WRAPPEDHEAD{}LONGTAIL", "x".repeat(120));
    let mut bytes = Vec::new();
    write!(bytes, "alpha beta gamma\r\n").expect("sample");
    write!(bytes, "{long_word}\r\n").expect("sample");
    write!(
        bytes,
        "\x1b]8;;https://example.com/page\x1b\\LINKTEXT\x1b]8;;\x1b\\\r\n"
    )
    .expect("sample");
    write!(bytes, "pre\there post\r\n").expect("sample");
    write!(bytes, "wide CJK\u{65e5}\u{672c}\u{8a9e} tail\r\n").expect("sample");
    write!(bytes, "\r\n").expect("sample");
    write!(bytes, "{}ENDWORD\r\n", " ".repeat(73)).expect("sample");
    write!(bytes, "dash-joined_word end\r\n").expect("sample");
    bytes
}

fn pointer_sample_context(
    column: u16,
    row: u16,
    word_separators: &WordSeparators,
) -> PointerContext {
    let mut terminal = new_terminal(80, 24, 2000).expect("terminal");
    let mut filter = EngineFilter::default();
    let mut renames = Vec::new();
    let mut bar = None;
    filter.write(
        &pointer_sample(),
        EngineKnobs::default(),
        &mut terminal,
        &mut renames,
        &mut bar,
        &mut None,
    );
    pointer_context(&terminal, None, column, row, word_separators).expect("pointer context")
}

#[test]
fn pointer_formats_read_the_word_and_the_row_under_the_cell() {
    let separators = WordSeparators::default();
    let beta = pointer_sample_context(7, 0, &separators);
    assert_eq!(beta.word, "beta");
    assert_eq!(beta.line, "alpha beta gamma");
    assert_eq!(beta.hyperlink, "");
    assert_eq!(pointer_sample_context(0, 0, &separators).word, "alpha");
    let separator_cell = pointer_sample_context(5, 0, &separators);
    assert_eq!(separator_cell.word, "beta");
    assert_eq!(separator_cell.line, "alpha beta gamma");
}

#[test]
fn pointer_formats_answer_nothing_past_a_row_and_on_a_blank_row() {
    let separators = WordSeparators::default();
    let past_the_end = pointer_sample_context(79, 0, &separators);
    assert_eq!(past_the_end.word, "");
    assert_eq!(past_the_end.line, "alpha beta gamma");
    let blank = pointer_sample_context(9, 6, &separators);
    assert_eq!(blank.word, "");
    assert_eq!(blank.line, "");
    assert_eq!(blank.hyperlink, "");
}

#[test]
fn pointer_word_crosses_a_wrap_where_the_line_does_not() {
    let separators = WordSeparators::default();
    let whole = format!("WRAPPEDHEAD{}LONGTAIL", "x".repeat(120));
    let head = pointer_sample_context(2, 1, &separators);
    assert_eq!(head.word, whole);
    assert_eq!(head.line, whole[..80]);
    let head_last_column = pointer_sample_context(79, 1, &separators);
    assert_eq!(head_last_column.word, whole);
    let tail = pointer_sample_context(4, 2, &separators);
    assert_eq!(tail.word, whole);
    assert_eq!(tail.line, whole[80..]);
}

#[test]
fn pointer_hyperlink_answers_the_osc_8_uri_of_the_cell() {
    let separators = WordSeparators::default();
    let link = pointer_sample_context(3, 3, &separators);
    assert_eq!(link.word, "LINKTEXT");
    assert_eq!(link.line, "LINKTEXT");
    assert_eq!(link.hyperlink, "https://example.com/page");
    let past_the_link = pointer_sample_context(19, 3, &separators);
    assert_eq!(past_the_link.word, "");
    assert_eq!(past_the_link.line, "LINKTEXT");
    assert_eq!(past_the_link.hyperlink, "");
}

#[test]
fn pointer_word_reads_the_same_text_from_both_halves_of_a_wide_cell() {
    let separators = WordSeparators::default();
    let expected = "CJK\u{65e5}\u{672c}\u{8a9e}";
    assert_eq!(pointer_sample_context(10, 5, &separators).word, expected);
    assert_eq!(pointer_sample_context(11, 5, &separators).word, expected);
    assert_eq!(
        pointer_sample_context(10, 5, &separators).line,
        "wide CJK\u{65e5}\u{672c}\u{8a9e} tail"
    );
}

#[test]
fn pointer_line_keeps_leading_blanks_and_trims_trailing_ones() {
    let separators = WordSeparators::default();
    let end = pointer_sample_context(77, 7, &separators);
    assert_eq!(end.word, "ENDWORD");
    assert_eq!(end.line, format!("{}ENDWORD", " ".repeat(73)));
}

#[test]
fn pointer_word_honours_the_word_separators_option() {
    let separators = WordSeparators::default();
    assert_eq!(pointer_sample_context(2, 8, &separators).word, "dash");
    assert_eq!(
        pointer_sample_context(11, 8, &separators).word,
        "joined_word"
    );
    let rebound = WordSeparators::new("_x");
    assert_eq!(pointer_sample_context(2, 8, &rebound).word, "dash-joined");
    assert_eq!(pointer_sample_context(11, 8, &rebound).word, "word");
    assert_eq!(pointer_sample_context(2, 1, &rebound).word, "WRAPPEDHEAD");
    assert_eq!(pointer_sample_context(4, 2, &rebound).word, "");
}

fn pointer_scrollback_sample(word: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    for index in 0..30u16 {
        write!(bytes, "FILLER{index:03}\r\n").expect("sample");
    }
    write!(bytes, "{word}\r\n").expect("sample");
    for index in 0..22u16 {
        write!(bytes, "TAIL{index:03}\r\n").expect("sample");
    }
    bytes
}

fn pointer_scrollback_terminal<'alloc>(word: &str) -> Terminal<'alloc, 'static> {
    let mut terminal = new_terminal(80, 24, 2000).expect("terminal");
    terminal.vt_write(&pointer_scrollback_sample(word));
    terminal
}

#[test]
fn pointer_word_crosses_a_wrap_back_into_the_scrollback() {
    let separators = WordSeparators::default();
    let whole = format!("WRAPPEDHEAD{}LONGTAIL", "x".repeat(120));
    let terminal = pointer_scrollback_terminal(&whole);
    let grid = LiveGrid::new(&terminal).expect("live grid");
    assert_eq!(grid.active_base().expect("active base"), 31);
    let tail = pointer_context(&terminal, None, 4, 0, &separators).expect("pointer context");
    assert_eq!(tail.word, whole);
    assert_eq!(tail.line, whole[80..]);
    let below = pointer_context(&terminal, None, 0, 1, &separators).expect("pointer context");
    assert_eq!(below.word, "TAIL000");
    assert_eq!(below.line, "TAIL000");
}

#[test]
fn pointer_formats_read_the_frozen_mode_at_its_own_viewport_offset() {
    let separators = WordSeparators::default();
    let whole = format!("WRAPPEDHEAD{}LONGTAIL", "x".repeat(120));
    let mut terminal = pointer_scrollback_terminal(&whole);
    let mut selection = None;
    let mut copy_mode = None;
    enter_copy_mode(
        &mut terminal,
        &mut selection,
        &mut copy_mode,
        false,
        false,
        None,
        true,
    )
    .expect("copy mode");
    let mut unseen_output = 0;
    for _ in 0..3 {
        apply_copy_mode_action(
            &mut terminal,
            &mut selection,
            &mut copy_mode,
            &mut unseen_output,
            CopyModeAction::ScrollUp,
            &separators,
            false,
        )
        .expect("scroll up");
    }
    let mode = copy_mode.as_deref().expect("mode survives the scroll");
    assert_eq!(mode.viewport_offset, 28);
    let frozen =
        pointer_context(&terminal, Some(mode), 2, 1, &separators).expect("pointer context");
    assert_eq!(frozen.word, "FILLER029");
    assert_eq!(frozen.line, "FILLER029");
    let live = pointer_context(&terminal, None, 2, 1, &separators).expect("pointer context");
    assert_eq!(live.word, "TAIL000");
    assert_eq!(live.line, "TAIL000");
}
