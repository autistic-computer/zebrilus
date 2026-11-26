use super::test_support::{Stub, context, expand};
use super::*;

#[test]
fn path_length_literal_and_quote_modifiers_are_lookup_scoped() {
    assert_eq!(expand("#{b:pane_title}"), "main");
    assert_eq!(expand("#{d:pane_title}"), "/tmp/a b");
    assert_eq!(expand("#{n:pane_title}"), "13");
    assert_eq!(expand("#{l:#{session_name}}"), "#{session_name}");
    assert_eq!(expand("#{q:pane_title}"), "/tmp/a\\ b/main");
    assert_eq!(expand("#{q/s:pane_title}"), "'/tmp/a b/main'");
    assert_eq!(
        quote_shell(b"a{b} c\n\t").as_bytes(),
        b"a\\{b\\}\\ c\\\n\\\t"
    );
    assert_eq!(expand("#{b:#{l:/x/y}}"), "/x/y");

    let mut context = context();
    context.pane_title = "/".to_owned();
    assert_eq!(
        expand_status("#{b:pane_title}|#{d:pane_title}", &context, &mut Stub),
        "/|/"
    );
    context.pane_title = "#tag".to_owned();
    assert_eq!(
        expand_status("#{q/h:pane_title}|#{q/e:pane_title}", &context, &mut Stub,),
        "##tag|##tag"
    );
    context.pane_title = " ".to_owned();
    assert_eq!(
        expand_status("#{q/a:pane_title}", &context, &mut Stub),
        "\" \""
    );
    context.pane_title = "$name".to_owned();
    assert_eq!(
        expand_status("#{q/a:pane_title}", &context, &mut Stub),
        "\"\\$name\""
    );
}

#[test]
fn width_modifier_counts_display_cells_around_style_spans() {
    assert_eq!(expand("#{w:#{l:abc}}|#{w:}|#{w:#{l:a##b}}"), "3|0|3");
    assert_eq!(
        expand("#{w:#{l:a#[fg=red]b}}|#{w:#{l:a###[fg=red]b}}|#{w:#{l:a##[fg=red]b}}"),
        "2|11|2"
    );
    assert_eq!(
        expand("#{w:#{l:#[fg=red]}}|#{w:#{l:x#[]y}}|#{w:#{l:a#[fg=red}}"),
        "0|2|0"
    );
    assert_eq!(expand("#{w:#{l:日本語}}|#{w:#{l:😀e##}}"), "6|4");
}

#[test]
fn width_modifier_runs_after_the_other_value_transforms() {
    assert_eq!(
        expand("#{w;=2:#{l:abcdef}}|#{=2;w:#{l:abcdef}}|#{=3;w:#{l:日本語abc}}"),
        "2|2|2"
    );
    assert_eq!(
        expand("#{w;p6:#{l:ab}}|#{p-6;w:#{l:ab}}|#{s/a/bb/;w:#{l:aXa}}"),
        "6|6|5"
    );
    assert_eq!(expand("#{n;w:#{l:日本語}}|#{w;n:#{l:日本語}}"), "1|1");
}

#[test]
fn width_modifier_gives_control_characters_no_width() {
    let mut context = context();
    for control in ["a\tb", "a\u{1b}b", "a\u{7f}b", "a\u{1}b"] {
        context.pane_title = control.to_owned();
        assert_eq!(
            expand_status("#{n:pane_title}|#{w:pane_title}", &context, &mut Stub),
            "3|2",
            "{control:?}"
        );
    }
    context.pane_title = "e\u{301}".to_owned();
    assert_eq!(
        expand_status("#{n:pane_title}|#{w:pane_title}", &context, &mut Stub),
        "3|1"
    );
}

#[test]
fn substitutions_are_global_case_aware_and_preserve_invalid_patterns() {
    let mut context = context();
    context.window_name = "main MAIN".to_owned();
    assert_eq!(
        expand_status(
            "#{s/a/A/:window_name}|#{s/main/x/i:window_name}|#{s/[/:window_name}",
            &context,
            &mut Stub,
        ),
        "mAin MAIN|x x|main MAIN"
    );
    context.window_name = "abc123".to_owned();
    assert_eq!(
        expand_status("#{s/([a-z]+)/\\1-/:window_name}", &context, &mut Stub),
        "abc-123"
    );
    context.window_name = "ab".to_owned();
    assert_eq!(
        expand_status("#{s/x*/X/:window_name}", &context, &mut Stub),
        "aXbX"
    );
}

#[test]
fn match_supports_fnmatch_regex_casefold_and_fuzzy_results() {
    assert_eq!(expand("#{m:m*,#{window_name}}"), "1");
    assert_eq!(expand("#{m/ri:MAIN,#{window_name}}"), "1");
    assert_eq!(expand("#{m/r:[,#{window_name}}"), "0");
    assert_eq!(expand("#{m/z:mn,#{window_name}}"), "1");
    assert_eq!(expand("#{m/p:mn,#{window_name}}"), "0,3");

    let mut context = context();
    context.window_name = "axab".to_owned();
    assert_eq!(
        expand_status("#{m/p:ab,#{window_name}}", &context, &mut Stub),
        "2,3"
    );
    context.window_name = "MAIN".to_owned();
    assert_eq!(
        expand_status("#{m/z:mn,#{window_name}}", &context, &mut Stub),
        "1"
    );
    context.window_name = "main worker".to_owned();
    assert_eq!(
        expand_status(
            "#{m/z:^main,#{window_name}}|#{m/z:main$,#{window_name}}|#{m/z:!other main,#{window_name}}|#{m/z:nope|main,#{window_name}}",
            &context,
            &mut Stub,
        ),
        "1|0|1|1"
    );
    context.window_name = "main".to_owned();
    assert_eq!(
        expand_status("#{m/z:MN,#{window_name}}", &context, &mut Stub),
        "0"
    );
    context.window_name = "界a".to_owned();
    assert_eq!(
        expand_status("#{m/p:界,#{window_name}}", &context, &mut Stub),
        "0,1"
    );
}

#[test]
fn truncation_uses_display_cells_markers_and_byte_length() {
    let mut context = context();
    context.pane_title = "ab界cd".to_owned();
    assert_eq!(
        expand_status(
            "#{=/4/...:pane_title}|#{=-3:pane_title}|#{n;=/4/...:pane_title}|#{=/2/x;=3:pane_title}",
            &context,
            &mut Stub,
        ),
        "ab界...|cd|8|abx"
    );
}

#[test]
fn padding_arithmetic_ascii_and_colour_match_the_pin() {
    assert_eq!(expand("#{p12:window_name}"), "main        ");
    assert_eq!(expand("#{p-6:window_name}"), "  main");
    assert_eq!(expand("#{p/x:window_name}"), "main");
    assert_eq!(expand("#{p6:#{l:界a}}"), "界a   ");
    assert_eq!(expand("#{p10001:window_name}"), "main");
    assert_eq!(expand("#{e|+|:2,3}"), "5");
    assert_eq!(expand("#{e|-|:2,5}"), "-3");
    assert_eq!(expand("#{e|m|:7,3}"), "1");
    assert_eq!(expand("#{e|%|:7,3}"), "1");
    assert_eq!(expand("#{e|%%|:7,3}"), "");
    assert_eq!(expand("#{e|==|:5,5}"), "1");
    assert_eq!(expand("#{e|/|f|3:1,3}"), "0.333");
    assert_eq!(expand("#{e|*|f:2.5,2}"), "5.00");
    assert_eq!(expand("#{e|+|:bad,2}"), "");
    assert_eq!(expand("#{e|+|f|x:1,2}"), "");
    assert_eq!(expand("#{e|+|f|2|extra:1,2}"), "");
    assert_eq!(expand("#{a:98}|#{a:65}|#{a:200}|#{a:nope}"), "b|A||");
    assert_eq!(expand("#{c:red}"), "800000");
    assert_eq!(expand("#{c:colour4}"), "000080");
    assert_eq!(expand("#{c:#7f7f7f}"), "7f7f7f");
    assert_eq!(expand("#{c/f:red}"), "\u{1b}[31m");
    assert_eq!(expand("#{c/b:colour4}"), "\u{1b}[48;5;4m");
    assert_eq!(expand("#{c/fb:red}"), "\u{1b}[41m");
    assert_eq!(expand("#{c/f:none}"), "\u{1b}[0m");
    assert_eq!(expand("#{c:notacolour}"), "");
    assert_eq!(expand("#{c/f:notacolour}"), "");
    assert_eq!(expand("#{C:needle}|#{C/ri:needle}"), "0|0");
}

#[test]
fn repeat_modifier_expands_operands_and_runs_before_post_transforms() {
    assert_eq!(expand("#{R:x,3}"), "xxx");
    assert_eq!(expand("#{R:#{session_name},2}"), "workwork");
    assert_eq!(expand("#{R:a#,,2}"), "a,a,");
    assert_eq!(expand("#{R:x,#{e|+|:1,2}}"), "xxx");
    assert_eq!(expand("#{R:x,+2}|#{R:x, 2}"), "xx|xx");
    assert_eq!(expand("#{n:#{R:x,10000}}"), "10000");
    assert_eq!(expand("#{n;R:x,3}|#{R;=/2:x,3}"), "3|xx");
}

#[test]
fn repeat_modifier_rejects_missing_or_out_of_range_counts() {
    for format in [
        "#{R:x,0}",
        "#{R:x,-1}",
        "#{R:x,10001}",
        "#{R:x,bad}",
        "#{R:x,2 }",
    ] {
        assert_eq!(expand(format), "", "format: {format}");
    }
    assert_eq!(expand("before#{R:x}after"), "before");
    assert_eq!(expand("before#{R:#{R:x,5000},10000}after"), "before");
}
