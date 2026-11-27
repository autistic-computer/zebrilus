use std::fs;

use super::*;

#[test]
fn a_found_machine_matches_a_configured_tailnet_host_by_its_first_label() {
    let tailnet = |peer: &str| {
        Endpoint::Tailnet(zz_daemon_client::TailnetEndpoint {
            peer: peer.to_owned(),
            port: None,
        })
    };
    assert!(same_machine(
        &tailnet("Alienware"),
        &tailnet("alienware.example.ts.net")
    ));
    assert!(!same_machine(
        &tailnet("xps"),
        &tailnet("alienware.example.ts.net")
    ));
    assert!(!same_machine(
        &tailnet("100.64.0.1"),
        &tailnet("100.64.0.3")
    ));
    assert!(!same_machine(
        &Endpoint::parse("ssh://alienware").unwrap(),
        &tailnet("alienware.example.ts.net")
    ));
}

#[test]
fn fleet_hosts_parse_in_config_order_without_becoming_daemon_entries() {
    let parsed = parse_config(
        "\
         host-desktop = ssh://fabrico@desktop:2222\n\
         background = #101010\n\
         host-scratch = unix:///tmp/zz-scratch.sock\n\
         host-legacy = /tmp/zz-legacy.sock\n",
    );
    let expected = vec![
        HostEntry {
            name: "desktop".to_owned(),
            endpoint: Endpoint::parse("ssh://fabrico@desktop:2222").expect("desktop endpoint"),
        },
        HostEntry {
            name: "scratch".to_owned(),
            endpoint: Endpoint::parse("unix:///tmp/zz-scratch.sock").expect("scratch endpoint"),
        },
        HostEntry {
            name: "legacy".to_owned(),
            endpoint: Endpoint::parse("/tmp/zz-legacy.sock").expect("legacy endpoint"),
        },
    ];

    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    assert_eq!(parsed.hosts, expected);
    assert_eq!(
        parsed.daemon_entries,
        [("background".to_owned(), "#101010".to_owned())]
    );
}

#[test]
fn valid_fleet_endpoint_is_retained_while_an_invalid_one_is_dropped() {
    let parsed = parse_config(
        "\
         host-gpu = ssh://gpu:7777\n\
         host-broken = quic://gpu:7777\n",
    );

    assert_eq!(
        parsed.hosts,
        [HostEntry {
            name: "gpu".to_owned(),
            endpoint: Endpoint::parse("ssh://gpu:7777").expect("ssh endpoint"),
        }]
    );
    assert!(parsed.daemon_entries.is_empty());
    assert_eq!(
        parsed.diagnostics,
        [ConfigDiagnostic {
            line: 2,
            message: "invalid `host-broken`: invalid endpoint URI `quic://gpu:7777`: quic endpoints were removed; use ssh://"
                .to_owned(),
        }]
    );
    assert_eq!(
        parsed.rejected_hosts,
        [RejectedHost {
            name: "broken".to_owned(),
            value: "quic://gpu:7777".to_owned(),
            reason:
                "invalid endpoint URI `quic://gpu:7777`: quic endpoints were removed; use ssh://"
                    .to_owned(),
        }]
    );
}

#[test]
fn duplicate_fleet_host_warns_and_moves_the_winner_to_its_config_position() {
    let parsed = parse_config(
        "\
         host-desktop = ssh://old-desktop\n\
         host-server = ssh://server\n\
         host-desktop = ssh://new-desktop\n",
    );

    assert_eq!(
        parsed.hosts,
        [
            HostEntry {
                name: "server".to_owned(),
                endpoint: Endpoint::parse("ssh://server").expect("server endpoint"),
            },
            HostEntry {
                name: "desktop".to_owned(),
                endpoint: Endpoint::parse("ssh://new-desktop")
                    .expect("replacement desktop endpoint"),
            },
        ]
    );
    assert!(parsed.daemon_entries.is_empty());
    assert_eq!(
        parsed.diagnostics,
        [ConfigDiagnostic {
            line: 3,
            message: "duplicate host `desktop`; last entry wins".to_owned(),
        }]
    );
}

#[test]
fn removing_a_fleet_host_drops_every_duplicate_and_preserves_other_lines() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(CONFIG_FILE_NAME);
    fs::write(
        &path,
        "# keep\nhost-desktop = ssh://old\ntray = false\nhost-desktop=ssh://new:9922 # effective\nhost-server = ssh://server:9922\n",
    )
    .unwrap();

    assert!(remove_fleet_host_at(&path, "desktop").unwrap());
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "# keep\ntray = false\nhost-server = ssh://server:9922\n"
    );
    assert!(!remove_fleet_host_at(&path, "desktop").unwrap());
}

#[test]
fn invalid_fleet_host_names_warn_and_are_dropped() {
    let parsed = parse_config(
        "\
         host-local = ssh://local-alias\n\
         host- = ssh://unnamed\n\
         host-bad name = ssh://bad-name\n",
    );

    assert!(parsed.hosts.is_empty());
    assert!(parsed.daemon_entries.is_empty());
    assert_eq!(
        parsed.diagnostics,
        [
            ConfigDiagnostic {
                line: 1,
                message: "invalid `host-local`: host name `local` is reserved".to_owned(),
            },
            ConfigDiagnostic {
                line: 2,
                message: "invalid `host-`: host name must not be empty".to_owned(),
            },
            ConfigDiagnostic {
                line: 3,
                message: "invalid `host-bad name`: host name must not contain whitespace"
                    .to_owned(),
            },
        ]
    );
    assert_eq!(
        parsed
            .rejected_hosts
            .iter()
            .map(|host| host.name.as_str())
            .collect::<Vec<_>>(),
        ["local", "", "bad name"]
    );
}

#[test]
fn fleet_host_writer_replaces_in_place_and_preserves_every_other_byte() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("zz/config");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let source = "# keep this comment\r\n\
                  host-desktop  = ssh://old-desktop  # keep this too\r\n\
                  tray = false\r\n";
    fs::write(&path, source).unwrap();

    write_fleet_host_at(&path, "desktop", "ssh://desktop:9922").unwrap();
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "# keep this comment\r\n\
         host-desktop  = ssh://desktop:9922  # keep this too\r\n\
         tray = false\r\n"
    );

    write_fleet_host_at(&path, "desktop", "ssh://desktop:7444").unwrap();
    let edited = fs::read_to_string(&path).unwrap();
    assert!(edited.contains("host-desktop  = ssh://desktop:7444  # keep this too\r\n"));
    assert_eq!(edited.matches("host-desktop").count(), 1);
    assert!(zz_daemon_client::validate_fleet_host("local", "ssh://desktop:7444").is_err());
}

#[test]
fn an_added_ssh_host_round_trips_through_the_config_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(CONFIG_FILE_NAME);
    fs::write(&path, "tray = false\n").unwrap();

    write_fleet_host_at(&path, "arch-desktop", "ssh://fabrico@arch-desktop:2222").unwrap();

    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "tray = false\nhost-arch-desktop = ssh://fabrico@arch-desktop:2222\n"
    );
    let parsed = load_config(&path).unwrap();
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    assert_eq!(
        parsed.hosts,
        [HostEntry {
            name: "arch-desktop".to_owned(),
            endpoint: Endpoint::Ssh(zz_daemon_client::SshEndpoint {
                user: Some("fabrico".to_owned()),
                host: "arch-desktop".to_owned(),
                port: Some(2222),
                remote_socket: None,
            }),
        }]
    );
}

#[ala::test]
fn install_config_publishes_and_clears_fleet_hosts(cx: &mut ala::TestAppContext) {
    let parsed = parse_config(
        "\
         host-desktop = ssh://desktop\n\
         host-scratch = unix:///tmp/zz-scratch.sock\n",
    );
    let expected = parsed.hosts.clone();

    cx.update(|cx| {
        install_config(Some(Path::new("/tmp/zz/config")), Some(Ok(parsed)), cx);
    });
    assert_eq!(cx.update(|cx| fleet_hosts(cx)), expected);

    cx.update(|cx| install_config(None, None, cx));
    assert!(cx.update(|cx| fleet_hosts(cx)).is_empty());
}
