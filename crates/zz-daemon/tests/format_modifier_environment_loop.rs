#![cfg(unix)]

mod clients;

use clients::Clients;
use zz_protocol::CommandInvocation;

const NAMES: &str = "#{V:<#{environ_name}>}";

fn wrap(lines: &[String]) -> String {
    lines.iter().fold(String::new(), |mut joined, line| {
        joined.push('<');
        joined.push_str(line);
        joined.push('>');
        joined
    })
}

fn stored(clients: &mut Clients, args: &[&str]) -> Vec<String> {
    clients
        .commands
        .execute(CommandInvocation::new("show-environment", args.to_vec()))
        .expect("show the environment")
        .lines()
        .map(|line| {
            line.strip_prefix('-')
                .map_or_else(
                    || line.split_once('=').expect("a live entry").0,
                    |removed| removed,
                )
                .to_owned()
        })
        .collect()
}

#[test]
fn the_default_and_the_session_flag_walk_the_sessions_own_store() {
    let mut clients = Clients::start("environ-loop-session", &["base"]);
    clients
        .commands
        .execute(CommandInvocation::new(
            "set-environment",
            ["-t", "base", "ZZ_LOOP_ONE", "one"],
        ))
        .expect("set one");
    clients
        .commands
        .execute(CommandInvocation::new(
            "set-environment",
            ["-t", "base", "ZZ_LOOP_TWO", ""],
        ))
        .expect("set an empty value");

    let names = stored(&mut clients, &["-t", "base"]);
    assert!(names.len() > 2, "the session store is seeded: {names:?}");
    assert_eq!(clients.format("base", NAMES), wrap(&names));
    assert_eq!(
        clients.format("base", "#{Vs:<#{environ_name}>}"),
        wrap(&names)
    );

    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
    assert_eq!(
        clients.format(
            "base",
            "#{V:#{?#{==:#{environ_name},ZZ_LOOP_TWO},<#{environ_value}|#{environ_removed}>,}}"
        ),
        "<|0>"
    );
}

#[test]
fn hidden_and_removed_entries_keep_their_rows() {
    let mut clients = Clients::start("environ-loop-flags", &["base"]);
    for args in [
        vec!["-t", "base", "-h", "ZZ_LOOP_HIDDEN", "secret"],
        vec!["-t", "base", "-r", "ZZ_LOOP_REMOVED"],
        vec!["-t", "base", "ZZ_LOOP_PLAIN", "kept"],
    ] {
        clients
            .commands
            .execute(CommandInvocation::new("set-environment", args))
            .expect("set the entry");
    }

    let row = "#{V:#{?#{m:ZZ_LOOP_*,#{environ_name}},\
        <#{environ_name}=#{environ_value}:#{environ_hidden}#{environ_removed}>,}}";
    assert_eq!(
        clients.format("base", row),
        "<ZZ_LOOP_HIDDEN=secret:10><ZZ_LOOP_PLAIN=kept:00><ZZ_LOOP_REMOVED=:01>"
    );
    assert!(!stored(&mut clients, &["-t", "base"]).contains(&"ZZ_LOOP_HIDDEN".to_owned()));
}

#[test]
fn each_flag_word_reads_its_own_store_when_the_names_collide() {
    let mut clients = Clients::start("environ-loop-collide", &["base"]);
    clients.attach_interactive("base");
    for args in [
        vec!["-g", "ZZ_LOOP_COLLIDE", "global"],
        vec!["-t", "base", "ZZ_LOOP_COLLIDE", "session"],
    ] {
        clients
            .commands
            .execute(CommandInvocation::new("set-environment", args))
            .expect("set the entry");
    }

    let pick = |flags: &str| {
        format!(
            "#{{V{flags}:#{{?#{{==:#{{environ_name}},ZZ_LOOP_COLLIDE}},<#{{environ_value}}>,}}}}"
        )
    };
    assert_eq!(clients.format("base", &pick("")), "<session>");
    assert_eq!(clients.format("base", &pick("s")), "<session>");
    assert_eq!(clients.format("base", &pick("g")), "<global>");
    assert_eq!(clients.format("base", &pick("c")), "\n");

    let path = std::env::var("PATH").expect("the test process has a PATH");
    assert_eq!(
        clients.format(
            "base",
            "#{Vc:#{?#{==:#{environ_name},PATH},<#{environ_value}>,}}"
        ),
        format!("<{path}>")
    );
}

#[test]
fn the_client_store_is_the_invoking_clients_own_environment() {
    let mut clients = Clients::start("environ-loop-client", &["base"]);
    let detached = clients.format("base", "#{Vc:<#{environ_name}>}");
    assert!(
        !detached.is_empty(),
        "the command client has an environment"
    );

    let attached = clients.attach_interactive("base");
    assert_eq!(clients.format("base", "#{Vc:<#{environ_name}>}"), detached);
    clients.detach(attached);
    assert_eq!(clients.format("base", "#{Vc:<#{environ_name}>}"), detached);

    let path = std::env::var("PATH").expect("the test process has a PATH");
    assert_eq!(
        clients.format(
            "base",
            "#{Vc:#{?#{==:#{environ_name},PATH},<#{environ_value}>,}}"
        ),
        format!("<{path}>")
    );
    let names = detached
        .trim_matches(['<', '>'])
        .split("><")
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
    assert_eq!(
        clients.format("base", "#{Vc:#{environ_hidden}#{environ_removed}}"),
        "00".repeat(names.len())
    );
}

#[test]
fn an_unspelled_flag_word_leaves_no_store_and_no_rows() {
    let mut clients = Clients::start("environ-loop-flagword", &["base"]);
    clients.attach_interactive("base");
    clients
        .commands
        .execute(CommandInvocation::new(
            "set-environment",
            ["-g", "ZZ_LOOP_GLOBAL", "g"],
        ))
        .expect("set a global entry");

    for flags in ["z", "gs", "sg", "sc", "gc", "S", "G", "C", "gg", "cs", " s"] {
        assert_eq!(
            clients.format("base", &format!("[#{{V{flags}:x}}]")),
            "[]",
            "V{flags}"
        );
    }
    for flags in ["", "s", "g", "c"] {
        assert_ne!(
            clients.format("base", &format!("[#{{V{flags}:x}}]")),
            "[]",
            "V{flags}"
        );
    }
}

#[test]
fn an_emptied_session_store_produces_no_rows_while_the_global_one_keeps_its_own() {
    let mut clients = Clients::start("environ-loop-context", &["base"]);
    for name in stored(&mut clients, &["-t", "base"]) {
        clients
            .commands
            .execute(CommandInvocation::new(
                "set-environment",
                ["-t", "base", "-u", name.as_str()],
            ))
            .expect("unset the entry");
    }

    assert_eq!(clients.format("base", "[#{V:x}]"), "[]");
    assert_eq!(clients.format("base", "[#{Vs:x}]"), "[]");
    assert_ne!(clients.format("base", "[#{Vg:x}]"), "[]");
}

#[test]
fn nested_and_malformed_environment_loops_keep_the_pinned_fallback() {
    let mut clients = Clients::start("environ-loop-nesting", &["base"]);
    clients
        .commands
        .execute(CommandInvocation::new(
            "set-environment",
            ["-t", "base", "ZZ_LOOP_NEST", "n"],
        ))
        .expect("set the entry");
    let rows = clients.format("base", "#{n:#{V:x}}");

    assert_eq!(
        clients.format("base", "#{n:#{V:#{W:x}}}"),
        rows,
        "one window per row"
    );
    assert_eq!(
        clients.format("base", "#{n:#{V:#{V:x}}}"),
        (rows.parse::<usize>().expect("a row count").pow(2)).to_string()
    );
    assert_eq!(
        clients.format(
            "base",
            "#{V:#{?#{==:#{environ_name},ZZ_LOOP_NEST},\
             [#{V:#{?#{==:#{environ_name},ZZ_LOOP_NEST},in,}}#{environ_name}],}}"
        ),
        "[inZZ_LOOP_NEST]"
    );
    assert_eq!(clients.format("base", "#{V:}"), "\n");
}

#[test]
fn the_row_formats_stay_empty_outside_a_loop() {
    let mut clients = Clients::start("environ-loop-leak", &["base"]);
    clients.attach_interactive("base");

    assert_eq!(
        clients.format(
            "base",
            "[#{environ_name}|#{environ_value}|#{environ_hidden}|#{environ_removed}]"
        ),
        "[|||]"
    );
    assert_eq!(
        clients.format("base", "#{V:x}[#{environ_name}#{environ_hidden}]"),
        format!(
            "{}[]",
            "x".repeat(
                clients
                    .format("base", "#{n:#{V:x}}")
                    .parse::<usize>()
                    .expect("a row count")
            )
        )
    );
}
