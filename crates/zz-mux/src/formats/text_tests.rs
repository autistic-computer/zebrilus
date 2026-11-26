use super::test_support::{Stub, context};
use super::*;

#[test]
fn the_client_sanitizer_spends_one_underscore_per_column_the_pin_would_have_drawn() {
    assert_eq!(utf8_sanitize(b"a\xffb"), "a_b");
    assert_eq!(utf8_sanitize("x\u{6f22}y".as_bytes()), "x__y");
    assert_eq!(utf8_sanitize("e\u{301}z".as_bytes()), "ez");
    assert_eq!(utf8_sanitize(b"a\tb"), "a_b");
    assert_eq!(utf8_sanitize(b"a\x7fb"), "a_b");
    assert_eq!(utf8_sanitize("no\u{e9}pe".as_bytes()), "no_pe");
    assert_eq!(utf8_sanitize(b"plain ASCII stays"), "plain ASCII stays");
    assert_eq!(utf8_sanitize(b"\xc3"), "_");
    assert_eq!(utf8_sanitize(b"\xc3z"), "_z");
    assert_eq!(
        sanitize_client_output(&RawText::from_bytes(b"a\xffb\na\xffb".to_vec())),
        RawText::from("a_b\na_b")
    );
}

#[test]
fn an_environment_value_no_utf8_holds_is_counted_the_way_format_draw_counts_it() {
    let value = RawText::from_bytes(b"a\xffb".to_vec());
    let mut context = context();
    context.format_universe = FormatUniverseRef {
        parts: Arc::new(FormatUniverse::with_global_environment(vec![
            FormatEnvironRow {
                name: "ZZBYTES".to_owned(),
                value: value.clone(),
                hidden: false,
                removed: false,
            },
        ])),
        engine: None,
    };
    let expand = |format: &str| expand_format_bytes(format, &context, &mut Stub).into_bytes();

    assert_eq!(expand("#{ZZBYTES}"), b"a\xffb".to_vec());
    assert_eq!(expand("#{b:#{ZZBYTES}}"), b"a\xffb".to_vec());
    assert_eq!(expand("#{d:#{ZZBYTES}}"), b"a\xffb".to_vec());
    assert_eq!(expand("#{q:#{ZZBYTES}}"), b"a\xffb".to_vec());
    assert_eq!(expand("#{E:ZZBYTES}"), b"a\xffb".to_vec());
    assert_eq!(expand("#{=1:#{ZZBYTES}}"), b"a".to_vec());
    assert_eq!(expand("#{=2:#{ZZBYTES}}"), b"ab".to_vec());
    assert_eq!(expand("#{=9:#{ZZBYTES}}"), b"ab".to_vec());
    assert_eq!(expand("#{=-1:#{ZZBYTES}}"), b"b".to_vec());
    assert_eq!(expand("#{=-2:#{ZZBYTES}}"), b"a\xffb".to_vec());
    assert_eq!(expand("#{?ZZBYTES,yes,no}"), b"yes".to_vec());
    assert_eq!(expand("#{n:ZZBYTES}|#{w:#{ZZBYTES}}"), b"3|2".to_vec());
    assert_eq!(expand("#{p5:#{ZZBYTES}}"), b"a\xffb   ".to_vec());
    assert_eq!(expand("#{p-5:#{ZZBYTES}}"), b"   a\xffb".to_vec());
    assert_eq!(
        basename(b"/tmp/a\xffb"),
        RawText::from_bytes(b"a\xffb".to_vec())
    );
    assert_eq!(format_byte_width(b"a\xffb"), 2);
}

#[test]
fn a_style_section_costs_a_trim_no_column_the_way_format_trim_left_does() {
    let trim =
        |value: &str, limit: isize| truncate_value(&RawText::from(value), limit, None).to_string();
    let marked = |value: &str, limit: isize| {
        truncate_value(&RawText::from(value), limit, Some("...")).to_string()
    };

    assert_eq!(trim("#[fg=red,bold]LEFT", 10), "#[fg=red,bold]LEFT");
    assert_eq!(trim("#[fg=red,bold]LEFT", 4), "#[fg=red,bold]LEFT");
    assert_eq!(trim("#[fg=red,bold]LEFT", 2), "#[fg=red,bold]LE");
    assert_eq!(trim("#[fg=red,bold]LEFT", 1), "#[fg=red,bold]L");
    assert_eq!(trim("#[fg=red,bold]LEFT", -10), "#[fg=red,bold]LEFT");
    assert_eq!(trim("#[fg=red,bold]LEFT", -4), "#[fg=red,bold]LEFT");
    assert_eq!(trim("#[fg=red,bold]LEFT", -2), "#[fg=red,bold]FT");
    assert_eq!(marked("#[fg=red,bold]LEFT", 2), "#[fg=red,bold]LE...");
    assert_eq!(marked("#[fg=red,bold]LEFT", -2), "...#[fg=red,bold]FT");
    assert_eq!(marked("#[fg=red,bold]LEFT", 4), "#[fg=red,bold]LEFT");

    let two = "AB#[fg=red]CD#[default]EF";
    assert_eq!(trim(two, 6), two);
    assert_eq!(trim(two, 3), "AB#[fg=red]C");
    assert_eq!(trim(two, -3), "#[fg=red]D#[default]EF");
    assert_eq!(trim(two, -2), "#[fg=red]#[default]EF");

    assert_eq!(trim("####AB", 3), "####A");
    assert_eq!(trim("####AB", 2), "####");
    assert_eq!(trim("####AB", 1), "##");
    assert_eq!(trim("####AB", -1), "B");
    assert_eq!(trim("###[fg=red]AB", 2), "###[fg=red]A");
    assert_eq!(trim("###[fg=red]AB", 1), "###[fg=red]");
    assert_eq!(trim("###[fg=red]AB", -1), "#[fg=red]B");

    assert_eq!(
        trim("#[fg=red]\u{65e5}\u{672c}\u{8a9e}", 2),
        "#[fg=red]\u{65e5}"
    );
    assert_eq!(
        trim("#[fg=red]\u{65e5}\u{672c}\u{8a9e}", 3),
        "#[fg=red]\u{65e5}"
    );
    assert_eq!(
        trim("#[fg=red]\u{65e5}\u{672c}\u{8a9e}", -2),
        "#[fg=red]\u{8a9e}"
    );

    assert_eq!(trim("#[fg=red", 2), "");
    assert_eq!(trim("#[fg=red", -2), "#[fg=red");
}
