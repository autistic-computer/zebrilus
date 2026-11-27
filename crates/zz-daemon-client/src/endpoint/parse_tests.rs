use super::*;

#[test]
fn one_machine_matches_across_ports_spellings_and_transports() {
    let parse = |endpoint: &str| Endpoint::parse(endpoint).unwrap();
    let macbook = parse("tailnet://macbook.beluga-wall.ts.net");
    for same in [
        "tailnet://macbook.beluga-wall.ts.net:9922",
        "tailnet://MacBook",
        "ssh://fabrico@macbook.beluga-wall.ts.net",
    ] {
        assert!(parse(same).same_machine(&macbook), "{same}");
    }
    for other in [
        "tailnet://alienware.beluga-wall.ts.net",
        "tailnet://macbook.beluga-wall.ts.net:9923",
        "tailnet://100.121.220.88",
        "/tmp/zz.sock",
    ] {
        assert!(!parse(other).same_machine(&macbook), "{other}");
    }
    let address = "100.121.220.88".parse().unwrap();
    assert!(parse("tailnet://100.121.220.88").is_address(address));
    assert!(parse("ssh://fabrico@100.121.220.88:22").is_address(address));
    assert!(!macbook.is_address(address));
}

#[test]
fn parses_bare_path() {
    assert_eq!(
        Endpoint::parse("/tmp/zz.sock").unwrap(),
        Endpoint::Local(PathBuf::from("/tmp/zz.sock"))
    );
}

#[test]
fn parses_unix_uri() {
    assert_eq!(
        Endpoint::parse("unix:///tmp/zz.sock").unwrap(),
        Endpoint::Local(PathBuf::from("/tmp/zz.sock"))
    );
}

#[test]
fn parses_ssh_host() {
    assert_eq!(
        Endpoint::parse("ssh://desktop").unwrap(),
        Endpoint::Ssh(SshEndpoint {
            user: None,
            host: "desktop".to_owned(),
            port: None,
            remote_socket: None,
        })
    );
}

#[test]
fn parses_full_ssh_uri() {
    assert_eq!(
        Endpoint::parse("ssh://user@host:2222/run/user/1000/zz/default.sock").unwrap(),
        Endpoint::Ssh(SshEndpoint {
            user: Some("user".to_owned()),
            host: "host".to_owned(),
            port: Some(2222),
            remote_socket: Some(PathBuf::from("/run/user/1000/zz/default.sock")),
        })
    );
}

#[test]
fn parses_wsl_uris_and_matches_distros_case_insensitively() {
    let plain = Endpoint::parse("wsl://Ubuntu-24.04").unwrap();
    assert_eq!(
        plain,
        Endpoint::Wsl(WslEndpoint {
            distro: "Ubuntu-24.04".to_owned(),
            user: None,
            remote_socket: None,
        })
    );
    assert_eq!(plain.to_string(), "wsl://Ubuntu-24.04");
    let full = Endpoint::parse("wsl://me@ubuntu-24.04/tmp/zz.sock").unwrap();
    assert_eq!(
        full,
        Endpoint::Wsl(WslEndpoint {
            distro: "ubuntu-24.04".to_owned(),
            user: Some("me".to_owned()),
            remote_socket: Some(PathBuf::from("/tmp/zz.sock")),
        })
    );
    assert_eq!(full.to_string(), "wsl://me@ubuntu-24.04/tmp/zz.sock");
    assert!(plain.same_machine(&full));
    for bad in ["wsl://", "wsl://-bad", "wsl://@ubuntu", "wsl://a@b@c"] {
        assert!(Endpoint::parse(bad).is_err(), "accepted {bad}");
    }
}

#[test]
fn rejects_unknown_scheme() {
    assert!(matches!(
        Endpoint::parse("tcp://host:7777"),
        Err(EndpointError::UriParse { .. })
    ));
}

#[test]
fn rejects_removed_quic_scheme_with_an_ssh_pointer() {
    let error = Endpoint::parse("quic://gpu:7777").unwrap_err();
    assert!(matches!(error, EndpointError::UriParse { .. }));
    assert!(error.to_string().contains("use ssh://"));
}

#[test]
fn rejects_empty_ssh_host() {
    assert!(matches!(
        Endpoint::parse("ssh:///tmp/default.sock"),
        Err(EndpointError::UriParse { .. })
    ));
}
