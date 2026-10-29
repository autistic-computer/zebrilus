#![cfg(unix)]

mod clients;

use clients::Clients;

const POSITION: &str = "#{L:<#{loop_index}:#{loop_last_flag}>}";

#[test]
fn an_empty_roster_produces_no_positions_at_all() {
    let mut clients = Clients::start("client-context-empty", &["base"]);
    assert_eq!(clients.format("base", POSITION), "\n");
}

#[test]
fn one_client_is_index_zero_and_the_last_row() {
    let mut clients = Clients::start("client-context-one", &["base"]);
    clients.attach_interactive("base");
    assert_eq!(clients.format("base", POSITION), "<0:1>");
}

#[test]
fn two_clients_count_from_zero_and_only_the_final_row_carries_the_flag() {
    let mut clients = Clients::start("client-context-two", &["base"]);
    let first = clients.attach_interactive("base");
    clients.attach_control("base");
    assert_eq!(clients.format("base", POSITION), "<0:0><1:1>");

    for form in [
        "#{Li:", "#{Ln:", "#{Lr:", "#{Lnr:", "#{Lt:", "#{Ltr:", "#{Lz:",
    ] {
        assert_eq!(
            clients.format(
                "base",
                &format!("{form}<#{{loop_index}}:#{{loop_last_flag}}>}}")
            ),
            "<0:0><1:1>",
            "{form}"
        );
    }

    let ascending = clients.format("base", "#{L:<#{loop_index}#{client_name}>}");
    assert_ne!(
        ascending,
        clients.format("base", "#{Lr:<#{loop_index}#{client_name}>}")
    );
    let before = clients.format("base", "#{Lt:<#{loop_index}#{client_name}>}");
    clients.note_activity(first, "base");
    assert_ne!(
        before,
        clients.format("base", "#{Lt:<#{loop_index}#{client_name}>}"),
        "input moved a different client to index 0"
    );
    assert_eq!(
        ascending,
        clients.format("base", "#{L:<#{loop_index}#{client_name}>}"),
        "the name order and its positions did not move"
    );
    assert_eq!(
        clients.format("base", "#{Lt:<#{loop_index}:#{loop_last_flag}>}"),
        "<0:0><1:1>"
    );

    clients.detach(first);
    assert_eq!(clients.format("base", POSITION), "<0:1>");
}

#[test]
fn a_nested_loop_owns_the_position_only_while_its_rows_run() {
    let mut clients = Clients::start("client-context-nested", &["base"]);
    clients.attach_interactive("base");
    clients.attach_control("base");

    assert_eq!(
        clients.format(
            "base",
            "#{L:[#{loop_index}#{L:(#{loop_index}#{loop_last_flag})}#{loop_index}]}"
        ),
        "[0(00)(11)0][1(00)(11)1]"
    );
    assert_eq!(
        clients.format(
            "base",
            "#{L:[#{loop_index}#{W:<#{loop_index}#{loop_last_flag}>}]}"
        ),
        "[0<01>][1<01>]"
    );
    assert_eq!(
        clients.format("base", "#{W:[#{loop_index}#{L:<#{loop_index}>}]}"),
        "[0<0><1>]"
    );
}

#[test]
fn the_position_formats_stay_empty_outside_every_loop() {
    let mut clients = Clients::start("client-context-leak", &["base"]);
    clients.attach_interactive("base");

    assert_eq!(
        clients.format("base", "[#{loop_index}|#{loop_last_flag}]"),
        "[|]"
    );
    assert_eq!(
        clients.format("base", "#{L:x}[#{loop_index}#{loop_last_flag}]"),
        "x[]"
    );
    assert_eq!(
        clients.format(
            "base",
            "#{?loop_last_flag,set,unset}#{L:#{?loop_last_flag,set,unset}}"
        ),
        "unsetset"
    );
}
