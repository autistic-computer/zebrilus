use std::{
    collections::HashSet,
    fs, io,
    net::{Ipv4Addr, SocketAddr, TcpStream},
    process::{Command, Output, Stdio},
    thread,
    time::Duration,
};

use parking_lot::Mutex;

use crate::{
    Endpoint, SshEndpoint, configured_fleet_hosts,
    endpoint::{
        KeyEnrollment, SSH_FAILURE_STATUS, UNATTENDED_CONNECT_TIMEOUT_SECONDS,
        authorized_key_script, parse_remote_probe_output, probe_value, remote_path_fallback,
        remote_socket_probe_script, run_remote_script, shell_quote, ssh_failure_reason,
    },
    short_device_name,
    ssh_key::{is_zz_key_line, key_body},
    unmasked::SpawnUnmasked as _,
    validate_fleet_host,
};

const LOCAL_SSH_PROBE_TIMEOUT: Duration = Duration::from_millis(500);
const HOST_KEY_FILES: [&str; 3] = ["ed25519", "ecdsa", "rsa"];
const HOST_LINE_PREFIX: &str = "zz-sync-host=";

static SYNC: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncLine {
    pub name: String,
    pub outcome: Result<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustReport {
    pub lines: Vec<SyncLine>,
    pub hosts: Vec<(String, String)>,
    pub known_hosts: Vec<String>,
}

pub fn sync_fleet() -> io::Result<Vec<SyncLine>> {
    let _one_at_a_time = SYNC.lock();
    let hosts = fleet_hosts()?;
    let hub = Hub::discover(&hosts)?;
    let collected = in_parallel(&hosts, |host| {
        collect(&host.endpoint, &hub.key).map(|facts| (host, facts))
    });
    let reached = collected
        .iter()
        .filter_map(|collected| collected.as_ref().ok())
        .map(|(host, facts)| (*host, facts))
        .collect::<Vec<_>>();
    let applied = in_parallel(&reached, |(host, facts)| {
        let plan = plan_host(&hub, host, facts, &reached);
        apply(&host.endpoint, &plan).map(|applied| summary(facts, &plan, &applied))
    });

    let mut applied = applied.into_iter();
    let mut lines = hosts
        .iter()
        .zip(&collected)
        .map(|(host, collected)| SyncLine {
            name: host.name.clone(),
            outcome: match collected {
                Ok(_) => applied
                    .next()
                    .expect("every reached host was applied")
                    .and_then(|outcome| outcome),
                Err(reason) => Err(reason.clone()),
            },
        })
        .collect::<Vec<_>>();
    lines.push(SyncLine {
        name: hub.name.clone(),
        outcome: trust_locally(&local_keys(&hub, &reached)),
    });
    Ok(lines)
}

pub fn trust_fleet_key(key: &str) -> io::Result<TrustReport> {
    let key = key.trim();
    if !is_zz_key_line(key) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "that is not a zz key; pass the line zz fleet key prints on the machine to trust",
        ));
    }
    let _one_at_a_time = SYNC.lock();
    let hosts = fleet_hosts()?;
    let hub = Hub::discover(&hosts)?;
    let plan = HostPlan {
        keys: vec![key.to_owned()],
        ..HostPlan::default()
    };
    let applied = in_parallel(&hosts, |host| {
        apply(&host.endpoint, &plan).and_then(|applied| key_outcome(applied.keys))
    });
    let mut lines = hosts
        .iter()
        .zip(applied)
        .map(|(host, outcome)| SyncLine {
            name: host.name.clone(),
            outcome,
        })
        .collect::<Vec<_>>();
    lines.push(SyncLine {
        name: hub.name.clone(),
        outcome: trust_locally(&plan.keys),
    });
    let mut seen = HashSet::new();
    Ok(TrustReport {
        lines,
        hosts: hub.hosts.iter().cloned().chain(hub.entry.clone()).collect(),
        known_hosts: hub
            .known_hosts
            .iter()
            .map(|known| known.line.clone())
            .filter(|line| seen.insert(line.clone()))
            .collect(),
    })
}

struct FleetHost {
    name: String,
    endpoint: SshEndpoint,
}

fn fleet_hosts() -> io::Result<Vec<FleetHost>> {
    let (configured, _) = configured_fleet_hosts()?;
    let hosts = configured
        .into_iter()
        .filter_map(|host| match host.endpoint {
            Endpoint::Ssh(endpoint) => Some(FleetHost {
                name: host.name,
                endpoint,
            }),
            Endpoint::Local(_) => None,
        })
        .collect::<Vec<_>>();
    if hosts.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "zz/config lists no ssh hosts; add one with zz fleet add NAME DEST",
        ));
    }
    Ok(hosts)
}

#[derive(Debug, Default)]
struct Hub {
    name: String,
    key: String,
    hosts: Vec<(String, String)>,
    entry: Option<(String, String)>,
    known_hosts: Vec<KnownHost>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct KnownHost {
    name: String,
    lookup: String,
    line: String,
    body: String,
}

impl Hub {
    fn discover(hosts: &[FleetHost]) -> io::Result<Self> {
        let key = crate::ssh_public_key()?;
        let name = short_device_name().unwrap_or_else(|| "localhost".to_owned());
        let user = std::env::var("USER")
            .or_else(|_| std::env::var("LOGNAME"))
            .ok();
        let entry = user
            .filter(|_| accepts_ssh_locally())
            .map(|user| (name.clone(), format!("ssh://{user}@{name}")))
            .filter(|(name, url)| validate_fleet_host(name, url).is_ok());
        let mut known_hosts = hosts
            .iter()
            .flat_map(|host| local_known_hosts(&host.name, &host.endpoint))
            .collect::<Vec<_>>();
        if entry.is_some() {
            known_hosts.extend(own_host_keys(&name));
        }
        Ok(Self {
            name,
            key,
            hosts: hosts
                .iter()
                .map(|host| (host.name.clone(), host.endpoint.to_string()))
                .collect(),
            entry,
            known_hosts,
        })
    }
}

fn accepts_ssh_locally() -> bool {
    TcpStream::connect_timeout(
        &SocketAddr::from((Ipv4Addr::LOCALHOST, 22)),
        LOCAL_SSH_PROBE_TIMEOUT,
    )
    .is_ok()
}

fn local_known_hosts(name: &str, endpoint: &SshEndpoint) -> Vec<KnownHost> {
    let lookup = endpoint.port.map_or_else(
        || endpoint.host.clone(),
        |port| format!("[{}]:{port}", endpoint.host),
    );
    let Ok(output) = Command::new("ssh-keygen")
        .arg("-F")
        .arg(&lookup)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output_unmasked()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| known_host(name, &lookup, line))
        .collect()
}

fn own_host_keys(name: &str) -> Vec<KnownHost> {
    HOST_KEY_FILES
        .iter()
        .filter_map(|kind| fs::read_to_string(format!("/etc/ssh/ssh_host_{kind}_key.pub")).ok())
        .filter_map(|public| {
            let mut fields = public.split_whitespace();
            let (kind, key) = (fields.next()?, fields.next()?);
            known_host(name, name, &format!("{name} {kind} {key}"))
        })
        .collect()
}

fn tcsh_safe(text: &str) -> bool {
    !text.contains('!')
}

fn known_host(name: &str, lookup: &str, line: &str) -> Option<KnownHost> {
    let mut fields = line.split_whitespace();
    let (hosts, kind, key) = (fields.next()?, fields.next()?, fields.next()?);
    let printable = |text: &str| text.bytes().all(|byte| byte.is_ascii_graphic());
    let key_type = kind
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'@'))
        && ["ssh-", "ecdsa-", "sk-"]
            .iter()
            .any(|prefix| kind.starts_with(prefix));
    let base64 = key
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'='));
    let plain = !hosts.starts_with(['@', '#']) && !hosts.contains(['*', '?', '!']);
    (plain && printable(hosts) && tcsh_safe(lookup) && key_type && base64).then(|| KnownHost {
        name: name.to_owned(),
        lookup: lookup.to_owned(),
        line: format!("{hosts} {kind} {key}"),
        body: format!("{kind} {key}"),
    })
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct HostFacts {
    key: Option<String>,
    hub_key: Option<KeyEnrollment>,
    zz: bool,
    hosts: Vec<(String, String)>,
}

fn collect_script(hub_key: &str) -> String {
    format!(
        "{}; if command -v \"$zz_cli\" >/dev/null 2>&1; then \
         \"$zz_cli\" fleet list -F '{HOST_LINE_PREFIX}#{{host_name}} #{{host_endpoint}}' \
         2>/dev/null; fi",
        remote_socket_probe_script(Some(hub_key)),
    )
}

fn collect(endpoint: &SshEndpoint, hub_key: &str) -> Result<HostFacts, String> {
    let output = run_remote_script(
        endpoint,
        UNATTENDED_CONNECT_TIMEOUT_SECONDS,
        &collect_script(hub_key),
    )
    .map_err(|error| format!("could not run ssh: {error}"))?;
    collected_facts(
        output.status.code(),
        &output.stdout,
        &output.stderr,
        &output.status.to_string(),
    )
}

fn collected_facts(
    code: Option<i32>,
    stdout: &[u8],
    stderr: &[u8],
    status: &str,
) -> Result<HostFacts, String> {
    let stderr = String::from_utf8_lossy(stderr);
    let text = String::from_utf8_lossy(stdout);
    if code == Some(SSH_FAILURE_STATUS) || probe_value(&text, "zz-probe-socket=").is_none() {
        return Err(ssh_failure_reason(&stderr, status));
    }
    let probe = parse_remote_probe_output("", stdout);
    Ok(HostFacts {
        key: probe.pubkey,
        hub_key: probe.key,
        zz: probe_value(&text, "zz-probe-protocol=") != Some("missing"),
        hosts: text
            .lines()
            .filter_map(|line| line.strip_prefix(HOST_LINE_PREFIX))
            .filter_map(|row| row.trim().split_once(' '))
            .map(|(name, url)| (name.to_owned(), url.trim().to_owned()))
            .collect(),
    })
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct HostPlan {
    keys: Vec<String>,
    host_lines: Vec<(String, String)>,
    known_hosts: Vec<KnownHost>,
    unwritable_hosts: usize,
    unsafe_hosts: usize,
}

fn plan_host(
    hub: &Hub,
    host: &FleetHost,
    facts: &HostFacts,
    fleet: &[(&FleetHost, &HostFacts)],
) -> HostPlan {
    let keys = distinct_keys(
        fleet
            .iter()
            .filter(|(other, _)| other.name != host.name)
            .filter_map(|(_, other)| other.key.as_deref()),
        [hub.key.as_str()].into_iter().chain(facts.key.as_deref()),
    );
    let names = facts
        .hosts
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<HashSet<_>>();
    let destinations = facts
        .hosts
        .iter()
        .filter_map(|(_, url)| destination_host(url))
        .chain([host.endpoint.host.to_ascii_lowercase()])
        .collect::<HashSet<_>>();
    let (candidates, unsafe_hosts) = hub
        .hosts
        .iter()
        .filter(|(name, _)| *name != host.name)
        .cloned()
        .chain(hub.entry.clone())
        .filter(|(name, url)| {
            !names.contains(name.as_str())
                && destination_host(url).is_some_and(|host| !destinations.contains(&host))
        })
        .map(|(name, url)| {
            let destination = url.strip_prefix("ssh://").unwrap_or(&url).to_owned();
            (name, destination)
        })
        .partition::<Vec<_>, _>(|(name, destination)| tcsh_safe(name) && tcsh_safe(destination));
    let (host_lines, unwritable_hosts) = if facts.zz {
        (candidates, 0)
    } else {
        (Vec::new(), candidates.len())
    };
    HostPlan {
        keys,
        host_lines,
        known_hosts: hub
            .known_hosts
            .iter()
            .filter(|known| known.name != host.name)
            .cloned()
            .collect(),
        unwritable_hosts,
        unsafe_hosts: unsafe_hosts.len(),
    }
}

fn destination_host(url: &str) -> Option<String> {
    match Endpoint::parse(url) {
        Ok(Endpoint::Ssh(endpoint)) => Some(endpoint.host.to_ascii_lowercase()),
        _ => None,
    }
}

fn distinct_keys<'a>(
    keys: impl IntoIterator<Item = &'a str>,
    except: impl IntoIterator<Item = &'a str>,
) -> Vec<String> {
    let mut seen = except.into_iter().map(key_body).collect::<HashSet<_>>();
    keys.into_iter()
        .filter(|key| is_zz_key_line(key) && seen.insert(key_body(key)))
        .map(str::to_owned)
        .collect()
}

fn apply_script(plan: &HostPlan) -> String {
    let keys = plan.keys.iter().map(|key| {
        format!(
            "( {} ) || printf 'zz-probe-key=failed\\n'; ",
            authorized_key_script(key_body(key), key),
        )
    });
    let names = (!plan.host_lines.is_empty())
        .then(|| "zz_names=$(\"$zz_cli\" fleet list -F '#{host_name}' 2>/dev/null); ".to_owned());
    let hosts = plan.host_lines.iter().map(|(name, destination)| {
        format!(
            "if printf '%s\\n' \"$zz_names\" | grep -qxF {name}; \
             then printf '{HOST_LINE_PREFIX}present\\n'; \
             elif \"$zz_cli\" fleet add {name} {destination} >/dev/null 2>&1; \
             then printf '{HOST_LINE_PREFIX}added\\n'; else printf '{HOST_LINE_PREFIX}failed\\n'; fi; ",
            name = shell_quote(name),
            destination = shell_quote(destination),
        )
    });
    let known_hosts = plan.known_hosts.iter().map(|known| {
        format!(
            "( {} ) || printf 'zz-sync-known=failed\\n'; ",
            known_host_script(known),
        )
    });
    std::iter::once(remote_path_fallback())
        .chain(keys)
        .chain(names)
        .chain(hosts)
        .chain(known_hosts)
        .chain(std::iter::once("true".to_owned()))
        .collect()
}

fn known_host_script(known: &KnownHost) -> String {
    let file = "\"$HOME/.ssh/known_hosts\"";
    format!(
        "umask 077; mkdir -p \"$HOME/.ssh\" && touch {file} || exit 1; \
         if grep -qxF {line} {file} || {{ command -v ssh-keygen >/dev/null 2>&1 && \
         ssh-keygen -F {lookup} -f {file} 2>/dev/null | grep -qF {body}; }}; \
         then printf 'zz-sync-known=present\\n'; exit 0; fi; \
         if [ -s {file} ] && [ -n \"$(tail -c 1 {file})\" ]; then echo >> {file}; fi; \
         printf '%s\\n' {line} >> {file} || exit 1; printf 'zz-sync-known=added\\n'",
        line = shell_quote(&known.line),
        lookup = shell_quote(&known.lookup),
        body = shell_quote(&known.body),
    )
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Tally {
    added: usize,
    present: usize,
    failed: usize,
}

impl Tally {
    fn of(stdout: &str, prefix: &str, expected: usize) -> Self {
        let mut tally = Self::default();
        for value in stdout
            .lines()
            .filter_map(|line| line.trim().strip_prefix(prefix))
        {
            match value {
                "added" => tally.added += 1,
                "present" => tally.present += 1,
                _ => tally.failed += 1,
            }
        }
        tally.failed += expected.saturating_sub(tally.added + tally.present + tally.failed);
        tally
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Applied {
    keys: Tally,
    hosts: Tally,
    known_hosts: Tally,
}

impl Applied {
    fn of(stdout: &[u8], plan: &HostPlan) -> Self {
        let stdout = String::from_utf8_lossy(stdout);
        Self {
            keys: Tally::of(&stdout, "zz-probe-key=", plan.keys.len()),
            hosts: Tally::of(&stdout, HOST_LINE_PREFIX, plan.host_lines.len()),
            known_hosts: Tally::of(&stdout, "zz-sync-known=", plan.known_hosts.len()),
        }
    }
}

fn apply(endpoint: &SshEndpoint, plan: &HostPlan) -> Result<Applied, String> {
    if plan.keys.is_empty() && plan.host_lines.is_empty() && plan.known_hosts.is_empty() {
        return Ok(Applied::default());
    }
    let output = run_remote_script(
        endpoint,
        UNATTENDED_CONNECT_TIMEOUT_SECONDS,
        &apply_script(plan),
    )
    .map_err(|error| format!("could not run ssh: {error}"))?;
    if output.status.code() == Some(SSH_FAILURE_STATUS) {
        return Err(ssh_failure_reason(
            &String::from_utf8_lossy(&output.stderr),
            &output.status.to_string(),
        ));
    }
    applied(&output, plan)
}

fn applied(output: &Output, plan: &HostPlan) -> Result<Applied, String> {
    if output.status.success() {
        return Ok(Applied::of(&output.stdout, plan));
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = stderr
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map_or_else(|| output.status.to_string(), ToOwned::to_owned);
    Err(format!("the sync script stopped early: {detail}"))
}

fn summary(facts: &HostFacts, plan: &HostPlan, applied: &Applied) -> Result<String, String> {
    let mut keys = applied.keys;
    match facts.hub_key {
        Some(KeyEnrollment::Added) => keys.added += 1,
        Some(KeyEnrollment::Present) => keys.present += 1,
        Some(KeyEnrollment::Failed) | None => keys.failed += 1,
    }
    let known = facts.hosts.len() + applied.hosts.added;
    let text = format!(
        "{}, knows {known} hosts (+{})",
        trust_text(keys),
        applied.hosts.added
    );
    let mut problems = Vec::new();
    if keys.failed > 0 {
        problems.push(format!(
            "could not add {} to ~/.ssh/authorized_keys",
            count(keys.failed, "zz key")
        ));
    }
    if applied.hosts.failed > 0 {
        problems.push(format!(
            "zz fleet add failed for {}",
            count(applied.hosts.failed, "host")
        ));
    }
    if applied.known_hosts.failed > 0 {
        problems.push(format!(
            "could not add {} to ~/.ssh/known_hosts",
            count(applied.known_hosts.failed, "host key")
        ));
    }
    if plan.unwritable_hosts > 0 {
        problems.push(format!(
            "zz is not installed there to learn {}",
            count(plan.unwritable_hosts, "host")
        ));
    }
    if plan.unsafe_hosts > 0 {
        problems.push(format!(
            "skipped {} whose name or destination has a !",
            count(plan.unsafe_hosts, "host")
        ));
    }
    if problems.is_empty() {
        Ok(text)
    } else {
        Err(format!("{text}; {}", problems.join("; ")))
    }
}

fn trust_text(keys: Tally) -> String {
    format!(
        "trusts {} (+{})",
        count(keys.added + keys.present, "zz key"),
        keys.added
    )
}

fn count(number: usize, noun: &str) -> String {
    if number == 1 {
        format!("1 {noun}")
    } else {
        format!("{number} {noun}s")
    }
}

fn local_keys(hub: &Hub, fleet: &[(&FleetHost, &HostFacts)]) -> Vec<String> {
    distinct_keys(
        fleet.iter().filter_map(|(_, facts)| facts.key.as_deref()),
        [hub.key.as_str()],
    )
}

#[cfg(unix)]
fn trust_locally(keys: &[String]) -> Result<String, String> {
    let plan = HostPlan {
        keys: keys.to_vec(),
        ..HostPlan::default()
    };
    let output = Command::new("/bin/sh")
        .arg("-c")
        .arg(apply_script(&plan))
        .stdin(Stdio::null())
        .output_unmasked()
        .map_err(|error| format!("could not run sh: {error}"))?;
    key_outcome(applied(&output, &plan)?.keys)
}

fn key_outcome(keys: Tally) -> Result<String, String> {
    let text = trust_text(keys);
    if keys.failed > 0 {
        Err(format!(
            "{text}; could not add {} to ~/.ssh/authorized_keys",
            count(keys.failed, "zz key")
        ))
    } else {
        Ok(text)
    }
}

#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)]
fn trust_locally(_: &[String]) -> Result<String, String> {
    Ok("skipped; zz does not manage this machine's authorized_keys on Windows".to_owned())
}

fn in_parallel<'a, T: Sync, R: Send>(items: &'a [T], work: impl Fn(&'a T) -> R + Sync) -> Vec<R> {
    thread::scope(|scope| {
        let work = &work;
        items
            .iter()
            .map(|item| scope.spawn(move || work(item)))
            .collect::<Vec<_>>()
            .into_iter()
            .map(|handle| handle.join().expect("a fleet sync thread panicked"))
            .collect()
    })
}

#[cfg(all(test, unix))]
mod tests {
    use std::{
        os::unix::{fs::PermissionsExt as _, process::ExitStatusExt as _},
        path::Path,
        process::ExitStatus,
    };

    use super::*;

    const HUB_KEY: &str = "ssh-ed25519 AAAAhub zz@macbook";
    const XPS_KEY: &str = "ssh-ed25519 AAAAxps zz@xps";
    const UBUNTU_KEY: &str = "ssh-ed25519 AAAAubuntu zz@ubuntu";
    const ALIENWARE_KEY: &str = "ssh-ed25519 AAAAalienware zz@alienware";

    fn fleet_host(name: &str, url: &str) -> FleetHost {
        let Endpoint::Ssh(endpoint) = Endpoint::parse(url).unwrap() else {
            panic!("{url} is not an ssh endpoint");
        };
        FleetHost {
            name: name.to_owned(),
            endpoint,
        }
    }

    fn facts(key: Option<&str>, hosts: &[(&str, &str)]) -> HostFacts {
        HostFacts {
            key: key.map(str::to_owned),
            hub_key: Some(KeyEnrollment::Added),
            zz: true,
            hosts: hosts
                .iter()
                .map(|(name, url)| ((*name).to_owned(), (*url).to_owned()))
                .collect(),
        }
    }

    fn known(name: &str) -> KnownHost {
        known_host(name, name, &format!("{name} ssh-ed25519 AAAA{name}")).unwrap()
    }

    fn hub(hosts: &[&FleetHost]) -> Hub {
        Hub {
            name: "macbook".to_owned(),
            key: HUB_KEY.to_owned(),
            hosts: hosts
                .iter()
                .map(|host| (host.name.clone(), host.endpoint.to_string()))
                .collect(),
            entry: Some(("macbook".to_owned(), "ssh://demfabris@macbook".to_owned())),
            known_hosts: ["xps", "ubuntu", "alienware", "macbook"]
                .into_iter()
                .map(known)
                .collect(),
        }
    }

    fn names(plan: &HostPlan) -> Vec<(&str, &str)> {
        plan.host_lines
            .iter()
            .map(|(name, destination)| (name.as_str(), destination.as_str()))
            .collect()
    }

    #[test]
    fn each_host_learns_the_other_keys_hosts_and_host_keys_it_lacks() {
        let xps = fleet_host("xps", "ssh://demfabris@xps");
        let ubuntu = fleet_host("ubuntu", "ssh://ubuntu");
        let alienware = fleet_host("alienware", "ssh://alienware:2222");
        let gpu = fleet_host("gpu", "ssh://gpu");
        let hub = hub(&[&xps, &ubuntu, &alienware, &gpu]);
        let xps_facts = facts(Some(XPS_KEY), &[("macbook", "ssh://macbook")]);
        let ubuntu_facts = facts(Some(UBUNTU_KEY), &[]);
        let alienware_facts = facts(
            Some(ALIENWARE_KEY),
            &[
                ("mac", "ssh://demfabris@MacBook"),
                ("box", "ssh://alienware"),
            ],
        );
        let fleet = [
            (&xps, &xps_facts),
            (&ubuntu, &ubuntu_facts),
            (&alienware, &alienware_facts),
        ];

        let plan = plan_host(&hub, &xps, &xps_facts, &fleet);
        assert_eq!(plan.keys, [UBUNTU_KEY, ALIENWARE_KEY]);
        assert_eq!(
            names(&plan),
            [
                ("ubuntu", "ubuntu"),
                ("alienware", "alienware:2222"),
                ("gpu", "gpu")
            ],
            "a name xps already has stays as xps wrote it, and gpu is listed though it was not reached"
        );
        assert_eq!(
            plan.known_hosts
                .iter()
                .map(|known| known.name.as_str())
                .collect::<Vec<_>>(),
            ["ubuntu", "alienware", "macbook"]
        );
        assert_eq!(plan.unwritable_hosts, 0);

        let plan = plan_host(&hub, &ubuntu, &ubuntu_facts, &fleet);
        assert_eq!(plan.keys, [XPS_KEY, ALIENWARE_KEY]);
        assert_eq!(
            names(&plan),
            [
                ("xps", "demfabris@xps"),
                ("alienware", "alienware:2222"),
                ("gpu", "gpu"),
                ("macbook", "demfabris@macbook"),
            ]
        );

        let plan = plan_host(&hub, &alienware, &alienware_facts, &fleet);
        assert_eq!(
            names(&plan),
            [
                ("xps", "demfabris@xps"),
                ("ubuntu", "ubuntu"),
                ("gpu", "gpu")
            ],
            "this machine is already there as mac, and box is alienware itself"
        );
    }

    #[test]
    fn a_host_without_zz_still_gets_keys_and_says_what_it_could_not_learn() {
        let xps = fleet_host("xps", "ssh://xps");
        let ubuntu = fleet_host("ubuntu", "ssh://ubuntu");
        let mut hub = hub(&[&xps, &ubuntu]);
        hub.entry = None;
        let mut xps_facts = facts(Some(XPS_KEY), &[]);
        xps_facts.zz = false;
        let ubuntu_facts = facts(Some(UBUNTU_KEY), &[]);
        let fleet = [(&xps, &xps_facts), (&ubuntu, &ubuntu_facts)];
        let plan = plan_host(&hub, &xps, &xps_facts, &fleet);
        assert_eq!(plan.keys, [UBUNTU_KEY]);
        assert!(plan.host_lines.is_empty());
        assert_eq!(plan.unwritable_hosts, 1);
        assert_eq!(
            summary(&xps_facts, &plan, &Applied::default()),
            Err(
                "trusts 1 zz key (+1), knows 0 hosts (+0); zz is not installed there to learn 1 host"
                    .to_owned()
            )
        );
    }

    #[test]
    fn keys_are_trusted_once_each_and_never_in_a_shape_zz_would_not_write() {
        assert_eq!(
            distinct_keys(
                [
                    XPS_KEY,
                    "ssh-ed25519 AAAAxps zz@xps-renamed",
                    "command=\"id\" ssh-ed25519 AAAAevil zz@evil",
                    "ssh-rsa AAAArsa zz@rsa",
                    UBUNTU_KEY,
                    HUB_KEY,
                ],
                [HUB_KEY],
            ),
            [XPS_KEY, UBUNTU_KEY]
        );
        let xps = fleet_host("xps", "ssh://xps");
        let twin = fleet_host("twin", "ssh://twin");
        let hub = hub(&[&xps, &twin]);
        let xps_facts = facts(Some(XPS_KEY), &[]);
        let shared = facts(Some(HUB_KEY), &[]);
        assert_eq!(
            local_keys(&hub, &[(&xps, &xps_facts), (&twin, &shared)]),
            [XPS_KEY]
        );
    }

    #[test]
    fn the_collect_pass_reads_the_key_the_hosts_and_whether_zz_is_there() {
        let stdout = format!(
            "Welcome!\nzz-probe-key=added\nzz-probe-pubkey={XPS_KEY}\n\
             zz-probe-socket=/run/user/1000/zz/default.sock\nzz-probe-protocol=110\n\
             {HOST_LINE_PREFIX}macbook ssh://macbook\n{HOST_LINE_PREFIX}gpu ssh://me@gpu:2222\n"
        );
        assert_eq!(
            collected_facts(Some(0), stdout.as_bytes(), b"", "exit status: 0"),
            Ok(HostFacts {
                key: Some(XPS_KEY.to_owned()),
                hub_key: Some(KeyEnrollment::Added),
                zz: true,
                hosts: vec![
                    ("macbook".to_owned(), "ssh://macbook".to_owned()),
                    ("gpu".to_owned(), "ssh://me@gpu:2222".to_owned()),
                ],
            })
        );
        let missing = collected_facts(
            Some(0),
            b"zz-probe-key=present\nzz-probe-socket=/tmp/zz-a/default.sock\nzz-probe-protocol=missing\n",
            b"",
            "exit status: 0",
        )
        .unwrap();
        assert!(!missing.zz);
        assert_eq!(missing.key, None);
        assert_eq!(
            collected_facts(
                Some(255),
                b"",
                b"ssh: connect to host xps port 22: Connection refused\n",
                "exit status: 255"
            )
            .unwrap_err(),
            ssh_failure_reason("Connection refused", "")
        );
    }

    #[test]
    fn known_hosts_lines_are_copied_bare_or_not_at_all() {
        let hashed = "|1|abc=|def= ssh-ed25519 AAAAC3Nza";
        assert_eq!(
            known_host("xps", "xps", hashed).map(|known| known.line),
            Some(hashed.to_owned())
        );
        assert_eq!(
            known_host(
                "xps",
                "xps",
                "xps,100.64.0.2 ecdsa-sha2-nistp256 AAAAE2 comment"
            )
            .map(|known| (known.line, known.body)),
            Some((
                "xps,100.64.0.2 ecdsa-sha2-nistp256 AAAAE2".to_owned(),
                "ecdsa-sha2-nistp256 AAAAE2".to_owned()
            ))
        );
        for rejected in [
            "# Host xps found: line 41",
            "@cert-authority *.lan ssh-ed25519 AAAA",
            "xps ssh-ed25519",
            "xps rsa AAAA",
            "xps ssh-ed25519 AAAA;rm",
            "* ssh-ed25519 AAAA",
            "*.ts.net,xps ssh-ed25519 AAAA",
            "xps,!xps-old ssh-ed25519 AAAA",
            "xps? ssh-ed25519 AAAA",
        ] {
            assert_eq!(known_host("xps", "xps", rejected), None, "{rejected}");
        }
        assert_eq!(known_host("x!y", "x!y", "x!y ssh-ed25519 AAAA"), None);
    }

    #[test]
    fn a_host_whose_name_or_destination_has_a_bang_is_skipped_and_reported() {
        let xps = fleet_host("xps", "ssh://xps");
        let bang = fleet_host("bang!", "ssh://bang");
        let odd = fleet_host("odd", "ssh://me!@odd");
        let mut hub = hub(&[&xps, &bang, &odd]);
        hub.entry = None;
        let xps_facts = facts(Some(XPS_KEY), &[]);
        let plan = plan_host(&hub, &xps, &xps_facts, &[(&xps, &xps_facts)]);
        assert!(plan.host_lines.is_empty());
        assert_eq!(plan.unsafe_hosts, 2);
        assert_eq!(
            summary(&xps_facts, &plan, &Applied::default()),
            Err(
                "trusts 1 zz key (+1), knows 0 hosts (+0); skipped 2 hosts whose name or \
                 destination has a !"
                    .to_owned()
            )
        );
    }

    #[test]
    fn an_apply_that_stops_early_or_skips_a_step_is_a_failure() {
        let plan = HostPlan {
            keys: vec![UBUNTU_KEY.to_owned()],
            host_lines: vec![("ubuntu".to_owned(), "ubuntu".to_owned())],
            known_hosts: vec![known("ubuntu")],
            ..HostPlan::default()
        };
        let output = |code: i32, stdout: &[u8], stderr: &[u8]| Output {
            status: ExitStatus::from_raw(code << 8),
            stdout: stdout.to_vec(),
            stderr: stderr.to_vec(),
        };
        assert_eq!(
            applied(&output(1, b"", b"xps: Event not found.\n"), &plan),
            Err("the sync script stopped early: xps: Event not found.".to_owned())
        );
        assert_eq!(
            applied(&output(2, b"", b""), &plan),
            Err("the sync script stopped early: exit status: 2".to_owned())
        );
        let partial = applied(&output(0, b"zz-probe-key=added\n", b""), &plan).unwrap();
        assert_eq!(
            (
                partial.keys.added,
                partial.hosts.failed,
                partial.known_hosts.failed
            ),
            (1, 1, 1)
        );
        let facts = facts(Some(XPS_KEY), &[]);
        assert!(summary(&facts, &plan, &partial).is_err());
        let mut silent = facts.clone();
        silent.hub_key = None;
        assert_eq!(
            summary(&silent, &HostPlan::default(), &Applied::default()),
            Err(
                "trusts 0 zz keys (+0), knows 0 hosts (+0); could not add 1 zz key to \
                 ~/.ssh/authorized_keys"
                    .to_owned()
            )
        );
    }

    #[test]
    fn the_summary_counts_what_each_host_trusts_and_knows() {
        let facts = facts(Some(XPS_KEY), &[("macbook", "ssh://macbook")]);
        let applied = Applied::of(
            b"zz-probe-key=added\nzz-probe-key=added\nzz-sync-host=added\nzz-sync-host=added\n\
              zz-sync-known=present\n",
            &HostPlan::default(),
        );
        assert_eq!(
            summary(&facts, &HostPlan::default(), &applied),
            Ok("trusts 3 zz keys (+3), knows 3 hosts (+2)".to_owned())
        );
        let applied = Applied::of(
            b"zz-probe-key=failed\nzz-sync-host=failed\nzz-sync-known=failed\n",
            &HostPlan::default(),
        );
        assert_eq!(
            summary(&facts, &HostPlan::default(), &applied),
            Err(
                "trusts 1 zz key (+1), knows 1 hosts (+0); could not add 1 zz key to \
                 ~/.ssh/authorized_keys; zz fleet add failed for 1 host; could not add 1 host key \
                 to ~/.ssh/known_hosts"
                    .to_owned()
            )
        );
    }

    fn fake_zz(home: &Path) -> String {
        let bin = home.join(".local/bin");
        fs::create_dir_all(&bin).unwrap();
        let zz = bin.join(zz_protocol::app_identity::DIRECTORY);
        fs::write(
            &zz,
            "#!/bin/sh\nf=\"$HOME/hosts\"; touch \"$f\"\ncase \"$1 $2\" in\n\
             \"fleet list\") while read -r n d; do printf '%s\\n' \"$4\" | \
             sed \"s|#{host_name}|$n|;s|#{host_endpoint}|ssh://$d|\"; done < \"$f\" ;;\n\
             \"fleet add\") grep -v \"^$3 \" \"$f\" > \"$f.new\"; printf '%s %s\\n' \"$3\" \"$4\" \
             >> \"$f.new\"; mv \"$f.new\" \"$f\" ;;\n*) exit 1 ;;\nesac\n",
        )
        .unwrap();
        fs::set_permissions(&zz, fs::Permissions::from_mode(0o755)).unwrap();
        format!("{}:/usr/bin:/bin", bin.display())
    }

    #[test]
    fn the_apply_script_run_twice_appends_everything_exactly_once() {
        let home = tempfile::tempdir().unwrap();
        let path = fake_zz(home.path());
        let plan = HostPlan {
            keys: vec![XPS_KEY.to_owned(), UBUNTU_KEY.to_owned()],
            host_lines: vec![
                ("ubuntu".to_owned(), "ubuntu".to_owned()),
                ("macbook".to_owned(), "demfabris@macbook".to_owned()),
            ],
            known_hosts: vec![known("ubuntu"), known("macbook")],
            ..HostPlan::default()
        };
        let ssh = home.path().join(".ssh");
        fs::create_dir_all(&ssh).unwrap();
        fs::write(ssh.join("known_hosts"), "macbook ssh-ed25519 AAAAmacbook").unwrap();
        let run = |login_shell: &str| {
            let output = Command::new(login_shell)
                .args([
                    "-c",
                    &format!("sh -c {}", shell_quote(&apply_script(&plan))),
                ])
                .env("HOME", home.path())
                .env("PATH", &path)
                .output()
                .unwrap();
            assert!(output.status.success(), "{login_shell}: {output:?}");
            applied(&output, &plan).unwrap()
        };

        let first = run("/bin/sh");
        assert_eq!(
            (first.keys.added, first.hosts.added, first.known_hosts),
            (
                2,
                2,
                Tally {
                    added: 1,
                    present: 1,
                    failed: 0
                }
            )
        );
        let tcsh = Path::new("/bin/tcsh");
        let second = run(if tcsh.exists() {
            "/bin/tcsh"
        } else {
            "/bin/sh"
        });
        assert_eq!(
            (
                second.keys.present,
                second.hosts.present,
                second.known_hosts.present
            ),
            (2, 2, 2)
        );
        assert_eq!(
            second.keys.added + second.hosts.added + second.known_hosts.added,
            0
        );

        assert_eq!(
            fs::read_to_string(ssh.join("authorized_keys")).unwrap(),
            format!("{XPS_KEY}\n{UBUNTU_KEY}\n")
        );
        assert_eq!(
            fs::read_to_string(ssh.join("known_hosts")).unwrap(),
            "macbook ssh-ed25519 AAAAmacbook\nubuntu ssh-ed25519 AAAAubuntu\n"
        );
        assert_eq!(
            fs::read_to_string(home.path().join("hosts")).unwrap(),
            "ubuntu ubuntu\nmacbook demfabris@macbook\n"
        );
    }
}
