use std::net::IpAddr;

use super::*;
use crate::Endpoint;

const STATUS: &str = r#"{
  "BackendState": "Running",
  "TUN": true,
  "TailscaleIPs": ["100.121.220.88", "fd7a:115c:a1e0::2c36:dc58"],
  "Self": {"HostName": "macbook", "UserID": 5054063778888173, "Tags": ["tag:box"]},
  "CurrentTailnet": {"Name": "owner@example.com"},
  "User": {
    "5054063778888173": {"LoginName": "macbook.example.ts.net"},
    "8568435244129308": {"LoginName": "tagged-devices"},
    "3112415680124528": {"LoginName": "owner@example.com"}
  },
  "Peer": {
    "nodekey:a": {"HostName": "alienware", "DNSName": "alienware.example.ts.net.", "OS": "linux",
                  "Online": true, "UserID": 8568435244129308, "Tags": ["tag:box"],
                  "TailscaleIPs": ["100.99.198.46", "fd7a:115c:a1e0::1"]},
    "nodekey:b": {"HostName": "localhost", "DNSName": "ipad.example.ts.net.", "OS": "iOS",
                  "Online": false, "UserID": 3112415680124528,
                  "TailscaleIPs": ["100.115.130.46"]}
  }
}"#;

fn ip(text: &str) -> IpAddr {
    text.parse().unwrap()
}

fn caller(login: &str, user_id: u64, tags: &[&str]) -> Caller {
    Caller {
        host: "caller".to_owned(),
        user_id,
        login: login.to_owned(),
        tags: tags.iter().map(|tag| (*tag).to_owned()).collect(),
    }
}

#[test]
fn status_reads_self_peers_and_logins() {
    let status = parse_status(STATUS).unwrap();
    assert_eq!(status.unusable(), None);
    assert!(status.is_personal());
    assert_eq!(status.tags, ["tag:box"]);
    assert_eq!(
        status.addresses,
        [ip("100.121.220.88"), ip("fd7a:115c:a1e0::2c36:dc58")]
    );
    let alienware = status.peer("alienware").unwrap();
    assert_eq!(alienware.login, "tagged-devices");
    assert!(alienware.online);
    assert_eq!(
        status.peer("ALIENWARE.example.ts.net").unwrap().host,
        "alienware"
    );
    assert_eq!(status.peer("ipad").unwrap().login, "owner@example.com");
    assert_eq!(
        status.peer("100.115.130.46").unwrap().dns_name,
        "ipad.example.ts.net."
    );
    assert!(status.peer("xps").is_none());
}

#[test]
fn status_is_unusable_unless_running_with_an_interface() {
    let running = parse_status(STATUS).unwrap();
    for (status, reason) in [
        (
            Status {
                backend: "NeedsLogin".to_owned(),
                ..running.clone()
            },
            "Tailscale is NeedsLogin",
        ),
        (
            Status {
                tun: false,
                ..running.clone()
            },
            "Tailscale runs in userspace-networking mode, so this machine has no tailnet address to listen on",
        ),
        (
            Status {
                user_id: 0,
                ..running.clone()
            },
            "Tailscale reported no account",
        ),
    ] {
        assert_eq!(status.unusable().as_deref(), Some(reason));
    }
}

#[test]
fn whois_reads_the_user_and_tags() {
    let json = r#"{"Node": {"ComputedName": "alienware", "Tags": ["tag:box"]},
                  "UserProfile": {"ID": 8568435244129308, "LoginName": "tagged-devices"}}"#;
    assert_eq!(
        parse_whois(json).unwrap(),
        Caller {
            host: "alienware".to_owned(),
            user_id: 8_568_435_244_129_308,
            login: "tagged-devices".to_owned(),
            tags: vec!["tag:box".to_owned()],
        }
    );
}

#[test]
fn admission_follows_the_policy_table() {
    let personal = parse_status(STATUS).unwrap();
    let organization = Status {
        tailnet: "example.com".to_owned(),
        tags: Vec::new(),
        user_id: 7,
        ..personal.clone()
    };
    let tagged_organization = Status {
        tags: vec!["tag:server".to_owned()],
        ..organization.clone()
    };
    let remote = ip("100.99.198.46");
    let cases: [(&str, Allow, &Status, Caller, IpAddr, Result<(), &str>); 11] = [
        (
            "tagged box, personal",
            Allow::Owner,
            &personal,
            caller("tagged-devices", 85, &["tag:box"]),
            remote,
            Ok(()),
        ),
        (
            "owner's phone, personal",
            Allow::Owner,
            &personal,
            caller("owner@example.com", 31, &[]),
            remote,
            Ok(()),
        ),
        (
            "invited user, personal",
            Allow::Owner,
            &personal,
            caller("guest@example.org", 42, &[]),
            remote,
            Err("guest@example.org is not the owner of the tailnet owner@example.com"),
        ),
        (
            "own address",
            Allow::Members,
            &personal,
            caller("tagged-devices", 85, &["tag:box"]),
            ip("fd7a:115c:a1e0::2c36:dc58"),
            Err("it comes from this machine's own tailnet address"),
        ),
        (
            "no identity",
            Allow::Members,
            &personal,
            caller("", 0, &[]),
            remote,
            Err("Tailscale did not say who it is"),
        ),
        (
            "same user, organization",
            Allow::Owner,
            &organization,
            caller("me@example.com", 7, &[]),
            remote,
            Ok(()),
        ),
        (
            "coworker, organization",
            Allow::Owner,
            &organization,
            caller("you@example.com", 8, &[]),
            remote,
            Err("it belongs to you@example.com"),
        ),
        (
            "tagged caller, organization",
            Allow::Owner,
            &organization,
            caller("tagged-devices", 7, &["tag:ci"]),
            remote,
            Err("it is a tagged device"),
        ),
        (
            "tagged self, organization",
            Allow::Owner,
            &tagged_organization,
            caller("me@example.com", 7, &[]),
            remote,
            Err("this machine is tagged, so it has no owner to match; set tailnet-allow = members"),
        ),
        (
            "coworker, members",
            Allow::Members,
            &organization,
            caller("you@example.com", 8, &[]),
            remote,
            Ok(()),
        ),
        (
            "tagged self, members",
            Allow::Members,
            &tagged_organization,
            caller("you@example.com", 8, &[]),
            remote,
            Ok(()),
        ),
    ];
    for (name, allow, me, caller, from, expected) in cases {
        assert_eq!(
            admit(allow, me, &caller, from),
            expected.map_err(str::to_owned),
            "{name}"
        );
    }
}

#[test]
fn settings_take_the_last_valid_line() {
    let settings = Settings::from_config(
        "tailnet = off\ntailnet-allow = members # coworkers too\ntailnet = sideways\nhost-arch = ssh://arch\n",
    );
    assert_eq!(
        settings,
        Settings {
            mode: Mode::Off,
            allow: Allow::Members,
        }
    );
    assert_eq!(Settings::from_config(""), Settings::default());
    assert_eq!(
        Settings::default().apply("tailnet", "sideways"),
        Err("expected auto, on or off".to_owned())
    );
}

#[test]
fn tailnet_endpoints_parse_and_print() {
    for (input, peer, port, shown) in [
        (
            "tailnet://alienware",
            "alienware",
            None,
            "tailnet://alienware",
        ),
        (
            "tailnet://xps.example.ts.net:9923/",
            "xps.example.ts.net",
            Some(9923),
            "tailnet://xps.example.ts.net:9923",
        ),
        (
            "tailnet://100.99.198.46",
            "100.99.198.46",
            None,
            "tailnet://100.99.198.46",
        ),
        (
            "tailnet://[fd7a:115c:a1e0::1]:9922",
            "fd7a:115c:a1e0::1",
            Some(9922),
            "tailnet://[fd7a:115c:a1e0::1]:9922",
        ),
    ] {
        let Endpoint::Tailnet(endpoint) = Endpoint::parse(input).unwrap() else {
            panic!("{input} is not a tailnet endpoint");
        };
        assert_eq!(
            (endpoint.peer.as_str(), endpoint.port),
            (peer, port),
            "{input}"
        );
        assert_eq!(endpoint.to_string(), shown);
    }
    for input in [
        "tailnet://",
        "tailnet://box:0",
        "tailnet://box:http",
        "tailnet://b ox",
        "tailnet://[box]",
    ] {
        assert!(Endpoint::parse(input).is_err(), "accepted {input}");
    }
}
