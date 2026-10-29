use std::collections::BTreeMap;

use zz_mux::{FormatClientRow, FormatJobTag, StatusContext, StatusHooks, expand_status};

struct Roster {
    rows: Vec<FormatClientRow>,
}

impl Roster {
    fn new(rows: &[(&str, u64, &str)]) -> Self {
        Self {
            rows: rows
                .iter()
                .map(|(name, activity, session)| FormatClientRow {
                    name: (*name).to_owned(),
                    activity: *activity,
                    variables: BTreeMap::from([
                        ("client_name".to_owned(), (*name).to_owned()),
                        ("client_session".to_owned(), (*session).to_owned()),
                        ("client_activity".to_owned(), activity.to_string()),
                    ]),
                    environment: Vec::new(),
                })
                .collect(),
        }
    }
}

impl StatusHooks for Roster {
    fn strftime(&mut self, literal: &str) -> String {
        literal.to_owned()
    }

    fn shell(&mut self, _command: &str, _tag: &FormatJobTag) -> String {
        String::new()
    }

    fn variable(&mut self, name: &str, _context: &StatusContext) -> Option<String> {
        (name == "client_name").then(|| "outer".to_owned())
    }

    fn client_loop_rows(&mut self, _context: &StatusContext) -> Vec<FormatClientRow> {
        self.rows.clone()
    }
}

fn expand(rows: &[(&str, u64, &str)], format: &str) -> String {
    let mut hooks = Roster::new(rows);
    expand_status(format, &StatusContext::default(), &mut hooks)
}

const THREE: &[(&str, u64, &str)] = &[("beta", 2, "s2"), ("alpha", 9, "s1"), ("gamma", 5, "s3")];

#[test]
fn the_default_and_the_index_flag_keep_the_connect_order() {
    for form in ["#{L:", "#{Li:"] {
        assert_eq!(
            expand(THREE, &format!("{form}<#{{client_name}}>}}")),
            "<beta><alpha><gamma>",
            "{form}"
        );
    }
}

#[test]
fn the_name_flag_orders_by_client_name() {
    assert_eq!(
        expand(THREE, "#{Ln:<#{client_name}>}"),
        "<alpha><beta><gamma>"
    );
}

#[test]
fn the_time_flag_orders_by_activity_newest_first() {
    assert_eq!(
        expand(THREE, "#{Lt:<#{client_name}>}"),
        "<alpha><gamma><beta>"
    );
}

#[test]
fn reversal_flips_the_connect_order() {
    for form in ["#{Lr:", "#{Lir:"] {
        assert_eq!(
            expand(THREE, &format!("{form}<#{{client_name}}>}}")),
            "<gamma><alpha><beta>",
            "{form}"
        );
    }
}

#[test]
fn reversal_negates_the_finished_comparison_including_the_name_tie_break() {
    assert_eq!(
        expand(THREE, "#{Lnr:<#{client_name}>}"),
        "<gamma><beta><alpha>"
    );
    assert_eq!(
        expand(THREE, "#{Ltr:<#{client_name}>}"),
        "<beta><gamma><alpha>"
    );

    let tied: &[(&str, u64, &str)] = &[("b", 4, "s"), ("a", 4, "s"), ("c", 4, "s")];
    assert_eq!(expand(tied, "#{Lt:#{client_name}}"), "abc");
    assert_eq!(expand(tied, "#{Ltr:#{client_name}}"), "cba");
}

#[test]
fn an_unknown_order_letter_falls_back_to_the_connect_order_without_reversing() {
    for form in ["#{Lz:", "#{L:", "#{Lqq:"] {
        assert_eq!(
            expand(THREE, &format!("{form}#{{client_name}}}}")),
            "betaalphagamma",
            "{form}"
        );
    }
    assert_eq!(expand(THREE, "#{Lzr:#{client_name}}"), "gammaalphabeta");
}

#[test]
fn a_row_answers_only_for_its_own_client_and_leaks_nothing_outside_the_loop() {
    assert_eq!(
        expand(THREE, "#{L:<#{client_name}:#{client_session}>}"),
        "<beta:s2><alpha:s1><gamma:s3>"
    );
    assert_eq!(expand(THREE, "#{L:<#{client_tty}>}"), "<><><>");
    assert_eq!(
        expand(THREE, "[#{client_name}]#{L:x}[#{client_name}]"),
        "[outer]xxx[outer]"
    );
}

#[test]
fn an_empty_roster_expands_the_loop_to_nothing() {
    assert_eq!(expand(&[], "[#{L:<#{client_name}>}]"), "[]");
    assert_eq!(expand(&[], "#{L:}"), "");
}

#[test]
fn client_loops_nest_and_each_level_keeps_its_own_row() {
    assert_eq!(
        expand(THREE, "#{L:[#{client_name}#{L:(#{client_name})}]}"),
        concat!(
            "[beta(beta)(alpha)(gamma)]",
            "[alpha(beta)(alpha)(gamma)]",
            "[gamma(beta)(alpha)(gamma)]"
        )
    );
}
