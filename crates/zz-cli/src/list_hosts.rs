use std::{process::ExitCode, thread};

use zz_daemon_client::{
    Endpoint, HostEntry, HostProbe, HostStatus, RejectedHost, configured_fleet_hosts, probe_host,
};
use zz_mux::{FormatJobTag, StatusContext, StatusHooks, expand_format_values, format_references};

const DEFAULT_FORMAT: &str = "#{host_name}: #{host_url} #{host_state}\
                              #{?host_version, #{host_version},}#{?host_error, (#{host_error}),}";
const PROBE_VARIABLES: [&str; 4] = ["host_state", "host_version", "host_protocol", "host_error"];
const USAGE: &str = "usage: zz list-hosts [-F format] [--json]";
const JSON_WITH_FORMAT: &str = "command list-hosts: --json cannot be combined with -F";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Listing<'a> {
    Format(&'a str),
    Json,
}

pub(crate) fn run(arguments: &[String]) -> ExitCode {
    match parse(arguments) {
        Ok(listing) => {
            let output = output(listing.unwrap_or(Listing::Format(DEFAULT_FORMAT)));
            match output {
                Ok(rows) => {
                    if !rows.is_empty() {
                        println!("{rows}");
                    }
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("zz: {error}");
                    crate::exit_code_for(crate::CliFailure::Runtime)
                }
            }
        }
        Err(usage) => {
            eprintln!("zz: {usage}");
            crate::exit_code_for(crate::CliFailure::Usage)
        }
    }
}

pub(crate) fn parse(arguments: &[String]) -> Result<Option<Listing<'_>>, String> {
    let mut format = None;
    let mut json = false;
    let mut arguments = arguments.iter();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--json" => json = true,
            "-F" => format = Some(arguments.next().ok_or_else(|| USAGE.to_owned())?.as_str()),
            flag if flag.len() > 2 && flag.starts_with("-F") => format = Some(&flag[2..]),
            _ => return Err(USAGE.to_owned()),
        }
    }
    match (format, json) {
        (Some(_), true) => Err(JSON_WITH_FORMAT.to_owned()),
        (Some(format), false) => Ok(Some(Listing::Format(format))),
        (None, true) => Ok(Some(Listing::Json)),
        (None, false) => Ok(None),
    }
}

pub(crate) fn output(listing: Listing<'_>) -> Result<String, String> {
    let (hosts, rejected) =
        configured_fleet_hosts().map_err(|error| format!("could not read zz/config: {error}"))?;
    for line in rejected_lines(&rejected) {
        eprintln!("{line}");
    }
    Ok(rows(&hosts, listing, probe_endpoint))
}

fn rejected_lines(rejected: &[RejectedHost]) -> Vec<String> {
    rejected
        .iter()
        .map(|host| {
            format!(
                "zz: host-{}: {}; zz fleet remove {} deletes it",
                host.name, host.reason, host.name
            )
        })
        .collect()
}

fn probe_endpoint(endpoint: &Endpoint) -> HostProbe {
    match endpoint {
        Endpoint::Ssh(endpoint) => probe_host(endpoint),
        Endpoint::Local(path) if path.exists() => HostProbe {
            status: HostStatus::Ready,
            version: String::new(),
            protocol: String::new(),
            error: String::new(),
        },
        Endpoint::Local(path) => HostProbe::failed(
            HostStatus::Unreachable,
            format!("no daemon socket at {}", path.display()),
        ),
    }
}

fn needs_probe(listing: Listing<'_>) -> bool {
    match listing {
        Listing::Json => true,
        Listing::Format(format) => format_references(format)
            .iter()
            .any(|name| PROBE_VARIABLES.contains(&name.as_str())),
    }
}

struct Row<'a> {
    host: &'a HostEntry,
    url: String,
    probe: Option<HostProbe>,
}

impl Row<'_> {
    fn value(&self, name: &str) -> Option<&str> {
        let probe = self.probe.as_ref();
        Some(match name {
            "host_name" => &self.host.name,
            "host_url" | "host_endpoint" => &self.url,
            "host_state" => probe.map_or("", |probe| probe.status.as_str()),
            "host_version" => probe.map_or("", |probe| &probe.version),
            "host_protocol" => probe.map_or("", |probe| &probe.protocol),
            "host_error" => probe.map_or("", |probe| &probe.error),
            _ => return None,
        })
    }
}

fn rows(
    hosts: &[HostEntry],
    listing: Listing<'_>,
    probe: impl Fn(&Endpoint) -> HostProbe + Sync,
) -> String {
    let probes = if needs_probe(listing) {
        thread::scope(|scope| {
            let probe = &probe;
            hosts
                .iter()
                .map(|host| {
                    scope.spawn(move || Some(with_advice(&host.name, probe(&host.endpoint))))
                })
                .collect::<Vec<_>>()
                .into_iter()
                .map(|handle| handle.join().expect("a host probe thread panicked"))
                .collect::<Vec<_>>()
        })
    } else {
        vec![None; hosts.len()]
    };
    hosts
        .iter()
        .zip(probes)
        .map(|(host, probe)| {
            let row = Row {
                host,
                url: host.endpoint.to_string(),
                probe,
            };
            match listing {
                Listing::Json => json_row(&row),
                Listing::Format(format) => {
                    expand_format_values(format, &StatusContext::default(), &mut RowHooks(&row))
                }
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn with_advice(name: &str, mut probe: HostProbe) -> HostProbe {
    let fix = match probe.status {
        HostStatus::Missing => format!("run: zz fleet install {name}"),
        HostStatus::Incompatible => format!("run: zz fleet install --restart-daemon {name}"),
        HostStatus::Ready | HostStatus::Auth | HostStatus::Unreachable => return probe,
    };
    probe.error = format!("{}; {fix}", probe.error);
    probe
}

fn json_row(row: &Row<'_>) -> String {
    let fields = [
        "host_name",
        "host_url",
        "host_state",
        "host_version",
        "host_protocol",
        "host_error",
    ]
    .map(|name| {
        format!(
            "{}:{}",
            serde_json::Value::from(name),
            serde_json::Value::from(row.value(name).unwrap_or_default())
        )
    });
    format!("{{{}}}", fields.join(","))
}

struct RowHooks<'a, 'b>(&'a Row<'b>);

impl StatusHooks for RowHooks<'_, '_> {
    fn strftime(&mut self, literal: &str) -> String {
        literal.to_owned()
    }

    fn shell(&mut self, _command: &str, _tag: &FormatJobTag) -> String {
        String::new()
    }

    fn variable(&mut self, name: &str, _context: &StatusContext) -> Option<String> {
        self.0.value(name).map(str::to_owned)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    fn hosts() -> Vec<HostEntry> {
        [
            ("xps", "ssh://demfabris@xps"),
            ("alienware", "ssh://alienware"),
        ]
        .into_iter()
        .map(|(name, url)| HostEntry {
            name: name.to_owned(),
            endpoint: Endpoint::parse(url).unwrap(),
        })
        .collect()
    }

    fn fake_probe(endpoint: &Endpoint) -> HostProbe {
        let Endpoint::Ssh(endpoint) = endpoint else {
            panic!("only ssh hosts here");
        };
        if endpoint.host == "xps" {
            HostProbe {
                status: HostStatus::Ready,
                version: "0.19.0".to_owned(),
                protocol: "110".to_owned(),
                error: String::new(),
            }
        } else {
            HostProbe::failed(
                HostStatus::Auth,
                "ssh rejected the login: add your key to the host or start an ssh agent",
            )
        }
    }

    fn arguments(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn the_default_listing_probes_and_names_the_state_version_and_trouble() {
        assert_eq!(
            rows(&hosts(), Listing::Format(DEFAULT_FORMAT), fake_probe),
            "xps: ssh://demfabris@xps ready 0.19.0\nalienware: ssh://alienware auth (ssh \
             rejected the login: add your key to the host or start an ssh agent)"
        );
    }

    #[test]
    fn formats_expand_like_any_list_command() {
        assert_eq!(
            rows(
                &hosts(),
                Listing::Format("#{host_name}=#{host_endpoint}"),
                fake_probe
            ),
            "xps=ssh://demfabris@xps\nalienware=ssh://alienware"
        );
        assert_eq!(
            rows(
                &hosts(),
                Listing::Format("#{?#{==:#{host_state},ready},up,down:#{host_state}}"),
                fake_probe
            ),
            "up\ndown:auth"
        );
        assert_eq!(
            rows(
                &hosts(),
                Listing::Format("[#{p10:host_name}] #{=3:host_url}"),
                fake_probe
            ),
            "[xps       ] ssh\n[alienware ] ssh"
        );
        assert_eq!(
            rows(
                &hosts(),
                Listing::Format("#{host_name}#{unknown}"),
                fake_probe
            ),
            "xps\nalienware"
        );
    }

    #[test]
    fn json_rows_carry_every_variable_in_a_fixed_order() {
        assert_eq!(
            rows(&hosts(), Listing::Json, fake_probe),
            "{\"host_name\":\"xps\",\"host_url\":\"ssh://demfabris@xps\",\"host_state\":\"ready\",\
             \"host_version\":\"0.19.0\",\"host_protocol\":\"110\",\"host_error\":\"\"}\n\
             {\"host_name\":\"alienware\",\"host_url\":\"ssh://alienware\",\"host_state\":\"auth\",\
             \"host_version\":\"\",\"host_protocol\":\"\",\"host_error\":\"ssh rejected the login: \
             add your key to the host or start an ssh agent\"}"
        );
    }

    #[test]
    fn only_a_listing_that_shows_probe_results_pays_for_ssh() {
        let probes = AtomicUsize::new(0);
        let counting = |endpoint: &Endpoint| {
            probes.fetch_add(1, Ordering::SeqCst);
            fake_probe(endpoint)
        };
        rows(
            &hosts(),
            Listing::Format("#{host_name} #{host_url}"),
            counting,
        );
        rows(
            &hosts(),
            Listing::Format("#{host_name} #{host_endpoint}"),
            counting,
        );
        assert_eq!(probes.load(Ordering::SeqCst), 0);
        rows(&hosts(), Listing::Format("#{host_version}"), counting);
        assert_eq!(probes.load(Ordering::SeqCst), 2);
        rows(&hosts(), Listing::Json, counting);
        assert_eq!(probes.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn hosts_that_need_zz_installed_say_how() {
        assert_eq!(
            with_advice(
                "gpu",
                HostProbe::failed(HostStatus::Missing, "zz is not installed")
            )
            .error,
            "zz is not installed; run: zz fleet install gpu"
        );
        assert_eq!(
            with_advice(
                "gpu",
                HostProbe::failed(HostStatus::Incompatible, "speaks protocol v107")
            )
            .error,
            "speaks protocol v107; run: zz fleet install --restart-daemon gpu"
        );
    }

    #[test]
    fn arguments_take_a_format_or_json_but_not_both() {
        assert_eq!(parse(&[]), Ok(None));
        assert_eq!(
            parse(&arguments(&["-F", "#{host_name}"])),
            Ok(Some(Listing::Format("#{host_name}")))
        );
        assert_eq!(
            parse(&arguments(&["-F#{host_name}"])),
            Ok(Some(Listing::Format("#{host_name}")))
        );
        assert_eq!(parse(&arguments(&["--json"])), Ok(Some(Listing::Json)));
        assert_eq!(
            parse(&arguments(&["--json", "-F", "x"])),
            Err(JSON_WITH_FORMAT.to_owned())
        );
        for rejected in [&["-F"][..], &["-a"], &["xps"]] {
            assert_eq!(parse(&arguments(rejected)), Err(USAGE.to_owned()));
        }
    }

    #[test]
    fn rejected_host_lines_name_the_fix() {
        assert_eq!(
            rejected_lines(&[RejectedHost {
                name: "gpu".to_owned(),
                value: "quic://gpu:9922".to_owned(),
                reason: "quic endpoints were removed; use ssh://".to_owned(),
            }]),
            [
                "zz: host-gpu: quic endpoints were removed; use ssh://; zz fleet remove gpu deletes it"
            ]
        );
    }
}
