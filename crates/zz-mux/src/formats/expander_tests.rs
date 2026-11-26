use super::test_support::{Stub, context, expand};
use super::*;

#[test]
fn animation_frames_follow_the_pins_format_cycle() {
    struct Clock {
        now: Option<u64>,
        animates: bool,
    }
    impl StatusHooks for Clock {
        fn strftime(&mut self, literal: &str) -> String {
            literal.to_owned()
        }

        fn shell(&mut self, _command: &str, _tag: &FormatJobTag) -> String {
            String::new()
        }

        fn cycle_clock(&mut self) -> Option<u64> {
            self.now
        }

        fn cycle_animates(&mut self) {
            self.animates = true;
        }
    }
    let frame = |format: &str, now: Option<u64>| {
        let run = |enabled: bool| {
            let mut clock = Clock {
                now,
                animates: false,
            };
            let value =
                compiled::with_enabled(enabled, || expand_status(format, &context(), &mut clock));
            (value, clock.animates)
        };
        let compiled = run(true);
        assert_eq!(compiled, run(false), "{format}");
        compiled
    };
    assert_eq!(frame("#{A:a,b,c}", None), (String::new(), false));
    assert_eq!(frame("#{A:a,b,c}", Some(0)), ("a".to_owned(), true));
    assert_eq!(frame("#{A:a,b,c}", Some(150)), ("b".to_owned(), true));
    assert_eq!(frame("#{A:a,b,c}", Some(320)), ("a".to_owned(), true));
    assert_eq!(frame("#{A/2:a,b,c}", Some(150)), ("a".to_owned(), true));
    assert_eq!(frame("#{A/2:a,b,c}", Some(250)), ("b".to_owned(), true));
    assert_eq!(frame("#{A/0:a,b}", Some(150)), ("b".to_owned(), true));
    assert_eq!(frame("#{A/x:a,b}", Some(150)), ("b".to_owned(), true));
    assert_eq!(frame("#{A:only}", Some(150)), ("only".to_owned(), false));
    assert_eq!(frame("#{A:}", Some(150)), (String::new(), false));
    assert_eq!(
        frame("#{A:#{session_name},x}", Some(0)),
        ("#{session_name}".to_owned(), true)
    );
}

#[test]
fn aliases_styles_shell_and_literal_time_keep_the_status_seam() {
    assert_eq!(
        expand("[#S] #I:#W.#P #D #F #H #h"),
        "[work] 1:main.0 %7 *- tower.local tower"
    );
    assert_eq!(expand("##S #z"), "#S #z");
    assert_eq!(
        expand("#[fg=green,bold]%H:%M#[default]"),
        "#[fg=green,bold]09:41#[default]"
    );
    assert_eq!(
        expand("#[fg=#{?client_prefix,red,green},range=#I]X"),
        "#[fg=green,range=#I]X"
    );
    assert_eq!(expand("##[fg=#{l:red}]X"), "##[fg=red]X");
    assert_eq!(expand("up #(uptime)"), "up <uptime>");
    assert_eq!(expand("#(date %H:%M)"), "<date 09:41>");
    assert_eq!(
        expand_format_values("#(date %H:%M)", &context(), &mut Stub),
        "<date %H:%M>"
    );
}

#[test]
fn comparisons_logic_truthiness_and_nested_conditionals_match_the_pin() {
    assert_eq!(
        expand("#{==:3,3}|#{!=:3,4}|#{<:10,2}|#{>:b,a}|#{<=:a,a}|#{>=:b,a}"),
        "1|1|1|1|1|1"
    );
    assert_eq!(expand("#{!:#{l:0}}|#{!!:00}|#{!!:-0}|#{!!:0.0}"), "1|1|1|1");
    assert_eq!(
        expand("#{&&:#{l:1},#{l:00},#{l:x}}|#{||:0,#{l:yes}}"),
        "1|1"
    );
    assert_eq!(expand("#{?#{==:#{window_panes},2},two,other}"), "two");
    assert_eq!(expand("#{?missing,no,#{l:default}}"), "default");
}

#[test]
fn expansion_output_keeps_every_byte_past_the_status_wire_bound() {
    let padded = expand("#{p-9000:#{l:tail}}");
    assert_eq!(padded.len(), 9000);
    assert!(padded.ends_with("tail"));
    assert_eq!(expand("#{n:#{p-9000:#{l:tail}}}"), "9000");
    assert_eq!(expand("#{R:x,10000}").len(), 10_000);
    assert_eq!(
        expand_format_values("#{p-9000:#{l:tail}}", &context(), &mut Stub).len(),
        9000
    );
}

#[test]
fn the_loop_limit_counts_recursion_depth_and_not_a_running_total() {
    let siblings = "#{l:x}".repeat(200);
    assert_eq!(expand(&siblings).len(), 200);

    let mut nested = "#{l:deep}".to_owned();
    for _ in 0..FORMAT_LOOP_LIMIT - 1 {
        nested = format!("#{{s/x/x/:{nested}}}");
    }
    assert_eq!(expand(&nested), "deep");
    assert_eq!(expand(&format!("#{{s/x/x/:{nested}}}")), "");
}

#[test]
fn time_expand_again_and_expand_again_with_time_are_distinct() {
    assert_eq!(
        expand_format_values("#{t/f/%Y:start_time}", &context(), &mut Stub),
        "2000"
    );
    assert_eq!(
        expand_format_values("#{t/f/%Y;t/f:start_time}", &context(), &mut Stub),
        "2000"
    );
    assert_eq!(expand("#{t/f/%Y:start_time}"), "2026");
    assert!(!expand("#{t:start_time}").is_empty());
    assert_eq!(expand("#{t:session_created}"), "");
    assert_eq!(expand("#{pane_dead_time}"), "");
    let mut context = context();
    context.pane_dead_time = Some(946_728_000);
    assert_eq!(
        expand_format_values(
            "#{pane_dead_time}|#{t/f/%Y:pane_dead_time}",
            &context,
            &mut Stub,
        ),
        "946728000|2000"
    );
    context.pane_title = "#{session_name}".to_owned();
    assert_eq!(
        expand_status("#{E:pane_title}", &context, &mut Stub),
        "work"
    );
    context.pane_title = "%Y #{session_name}".to_owned();
    assert_eq!(
        expand_status("#{T:pane_title}", &context, &mut Stub),
        "2026 work"
    );

    context.format_now = Some(946_731_661);
    context.start_time = Some(946_728_000);
    assert_eq!(
        expand_status("#{t/r:start_time}", &context, &mut Stub),
        "1h1m"
    );
    assert_eq!(
        expand_status("#{t/p;t/r:start_time}", &context, &mut Stub),
        "1h1m"
    );
    assert!(expand_status("#{t/p:start_time}", &context, &mut Stub).contains(':'));
    let difference = expand_status("#{t/d:start_time}", &context, &mut Stub)
        .parse::<u64>()
        .unwrap();
    assert_eq!(difference, 3_661);
}

#[test]
fn unknown_and_malformed_forms_follow_source_fallbacks() {
    assert_eq!(expand("before#{not_a_tmux_variable}after"), "beforeafter");
    assert_eq!(expand("before#{z:pane_title}after"), "beforeafter");
    assert_eq!(expand("#{z:#{l:foo}}"), "z:foo");
    assert_eq!(expand("#{p:pane_title}"), "/tmp/a b/main");
    assert_eq!(expand("before#{==:one}after"), "before");
}

#[test]
fn recursive_expansion_stops_at_the_pinned_depth() {
    let nested = (0..10).fold("#{l:x}".to_owned(), |value, _| format!("#{{E:{value}}}"));
    assert_eq!(expand(&nested), "x");
    let hostile = (0..110).fold("#{l:x}".to_owned(), |value, _| format!("#{{E:{value}}}"));
    assert_eq!(expand(&hostile), "");
}
