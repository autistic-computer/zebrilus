use super::*;
use std::{
    ffi::OsString,
    io::{BufRead as _, BufReader},
    process::{Command, ExitStatus, Stdio},
    sync::mpsc,
};

use zz_daemon_client::unmasked::SpawnUnmasked as _;
use zz_protocol::{
    AgentProvider, MAX_PROVIDER_TEXT_BYTES, ManualUpdate, ProviderStatus, ProviderTask,
    ProvidersReport,
};

const PROVIDERS: [AgentProvider; 2] = [AgentProvider::ClaudeCode, AgentProvider::Codex];
const VERSION_TIMEOUT: Duration = Duration::from_secs(10);
const LATEST_TIMEOUT: Duration = Duration::from_secs(15);
const LATEST_FRESH: Duration = Duration::from_mins(10);
const TASK_TIMEOUT: Duration = Duration::from_mins(15);
const PUBLISH_GAP: Duration = Duration::from_millis(100);
const OUTPUT_GRACE: Duration = Duration::from_millis(500);
const FAILURE_LINES: usize = 3;
const CLAUDE_INSTALL: &str = "curl -fsSL https://claude.ai/install.sh | bash";
const CODEX_INSTALL: &str = "curl -fsSL https://chatgpt.com/codex/install.sh | sh";

#[derive(Default)]
pub(super) struct Providers {
    report: ProvidersReport,
    watchers: BTreeSet<ClientId>,
    latest: BTreeMap<AgentProvider, (Instant, Option<String>)>,
    #[cfg(test)]
    search: Option<OsString>,
}

impl Providers {
    fn status_mut(&mut self, provider: AgentProvider) -> &mut ProviderStatus {
        if self.report.provider(provider).is_none() {
            self.report.providers.push(ProviderStatus::new(provider));
            self.report
                .providers
                .sort_by_key(|status| PROVIDERS.iter().position(|known| *known == status.provider));
        }
        self.report
            .providers
            .iter_mut()
            .find(|status| status.provider == provider)
            .expect("provider status was just inserted")
    }

    fn running(&self, provider: AgentProvider) -> bool {
        self.report
            .provider(provider)
            .is_some_and(|status| matches!(status.task, ProviderTask::Running { .. }))
    }
}

impl Shared {
    pub(super) fn providers(
        self: &Arc<Self>,
        client: ClientId,
        args: &[RawText],
    ) -> Result<Execution, DaemonError> {
        let spec = zz_protocol::catalog_command_spec("providers").expect("registered command");
        let parsed = zz_protocol::parse_tmux_options(spec, args)?;
        spec.validate_positional_maximum(parsed.positionals.len())?;
        let update = parsed
            .options
            .iter()
            .find_map(|option| match option {
                zz_protocol::TmuxOption::Value("-u", value) => Some(value.parse::<AgentProvider>()),
                _ => None,
            })
            .transpose()
            .map_err(ServerError::CommandParse)?;
        let owner = self.server_owner();
        let start = {
            let mut providers = self.providers.lock();
            providers.watchers.insert(client);
            match update {
                Some(provider) if providers.running(provider) => false,
                Some(provider) => {
                    providers.status_mut(provider).task = ProviderTask::Running {
                        output: String::new(),
                    };
                    true
                }
                None if providers.report.checking => false,
                None => {
                    providers.report.checking = true;
                    true
                }
            }
        };
        self.publish_providers();
        if start {
            let spawned = thread::Builder::new()
                .name("zz-providers".to_owned())
                .spawn(move || match update {
                    Some(provider) => owner.run_provider_task(provider),
                    None => owner.check_providers(),
                });
            if let Err(error) = spawned {
                let mut providers = self.providers.lock();
                match update {
                    Some(provider) => {
                        providers.status_mut(provider).task = ProviderTask::Failed {
                            message: format!("Could not start the updater: {error}"),
                        };
                    }
                    None => providers.report.checking = false,
                }
                drop(providers);
                self.publish_providers();
            }
        }
        Ok(Execution::default())
    }

    fn publish_providers(&self) {
        let (report, watchers) = {
            let mut providers = self.providers.lock();
            providers
                .watchers
                .retain(|client| self.read_client(*client, |client| client.is_some()));
            (providers.report.clone(), providers.watchers.clone())
        };
        for client in watchers {
            self.publish_to_client(client, EventPayload::Providers(report.clone()));
        }
    }

    fn provider_search(&self) -> Option<OsString> {
        #[cfg(test)]
        if let Some(search) = self.providers.lock().search.clone() {
            return Some(search);
        }
        search_path()
    }

    fn check_providers(&self) {
        let search = self.provider_search();
        for provider in PROVIDERS {
            let found = detect(provider, search.as_ref());
            let latest = self.latest_release(provider, found.as_ref(), search.as_ref());
            let mut providers = self.providers.lock();
            let status = providers.status_mut(provider);
            apply(found.as_ref(), status);
            status.latest = latest;
        }
        self.providers.lock().report.checking = false;
        self.publish_providers();
    }

    fn latest_release(
        &self,
        provider: AgentProvider,
        found: Option<&Found>,
        search: Option<&OsString>,
    ) -> Option<String> {
        if let Some((checked, latest)) = self.providers.lock().latest.get(&provider)
            && checked.elapsed() < LATEST_FRESH
            && latest.is_some()
        {
            return latest.clone();
        }
        let latest = fetch_latest(provider, found, search);
        self.providers
            .lock()
            .latest
            .insert(provider, (Instant::now(), latest.clone()));
        latest
    }

    fn run_provider_task(&self, provider: AgentProvider) {
        let search = self.provider_search();
        let found = detect(provider, search.as_ref());
        let before = found.as_ref().and_then(|found| found.version.clone());
        let outcome = match task_command(provider, found.as_ref(), search.as_ref()) {
            Ok(command) => {
                let mut published = Instant::now();
                run(command, TASK_TIMEOUT, |line| {
                    self.providers.lock().status_mut(provider).task = ProviderTask::Running {
                        output: bounded(line),
                    };
                    if published.elapsed() >= PUBLISH_GAP {
                        published = Instant::now();
                        self.publish_providers();
                    }
                })
            }
            Err(message) => Err(message),
        };
        let found = detect(provider, search.as_ref());
        let latest = self.latest_release(provider, found.as_ref(), search.as_ref());
        let mut providers = self.providers.lock();
        let status = providers.status_mut(provider);
        apply(found.as_ref(), status);
        status.latest = latest;
        status.task = match outcome {
            Ok(_) if !status.installed() => ProviderTask::Failed {
                message: format!(
                    "The installer finished, but {} is not on the login PATH. Open a new shell \
                     and check that ~/.local/bin is on PATH.",
                    program(provider)
                ),
            },
            Ok(_) if status.outdated() && status.version == before => ProviderTask::Failed {
                message: format!(
                    "The updater finished, but {} still reports {}.",
                    program(provider),
                    status.version.as_deref().unwrap_or("its old version")
                ),
            },
            Ok(_) => ProviderTask::Idle,
            Err(message) => ProviderTask::Failed {
                message: bounded(&message),
            },
        };
        log::info!(
            target: "zz::providers",
            "{} {} {:?} -> {:?}",
            provider.as_str(),
            if before.is_some() { "update" } else { "install" },
            before,
            status.version,
        );
        drop(providers);
        self.publish_providers();
    }
}

struct Found {
    path: PathBuf,
    version: Option<String>,
    owner: Owner,
}

#[derive(Debug, PartialEq, Eq)]
enum Owner {
    Updater,
    Npm {
        bin: Option<PathBuf>,
    },
    Homebrew {
        brew: PathBuf,
        cask: bool,
        token: String,
    },
    Manual(ManualUpdate),
}

impl Owner {
    fn first_on_path(&self) -> Option<&Path> {
        match self {
            Self::Npm { bin } => bin.as_deref(),
            Self::Updater | Self::Homebrew { .. } | Self::Manual(_) => None,
        }
    }
}

fn apply(found: Option<&Found>, status: &mut ProviderStatus) {
    status.path = found.map(|found| found.path.to_string_lossy().into_owned());
    status.version = found.and_then(|found| found.version.clone());
    status.manual = found.and_then(|found| match &found.owner {
        Owner::Manual(manual) => Some(manual.clone()),
        Owner::Updater | Owner::Npm { .. } | Owner::Homebrew { .. } => None,
    });
}

fn manual_message(manual: &ManualUpdate) -> String {
    match &manual.command {
        Some(command) => format!("Installed by {}. Update it with: {command}", manual.owner),
        None => format!("Installed by {}. Update it there.", manual.owner),
    }
}

const fn program(provider: AgentProvider) -> &'static str {
    match provider {
        AgentProvider::ClaudeCode => "claude",
        AgentProvider::Codex => "codex",
    }
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|home| home.is_dir())
}

fn search_path() -> Option<OsString> {
    #[cfg(feature = "agent")]
    let base = crate::agent::environment::agent_path().map(OsString::from);
    #[cfg(not(feature = "agent"))]
    let base: Option<OsString> = None;
    let base = base.or_else(|| std::env::var_os("PATH"));
    let mut directories = base
        .as_deref()
        .map(|path| std::env::split_paths(path).collect::<Vec<_>>())
        .unwrap_or_default();
    if let Some(home) = home() {
        for extra in [home.join(".local/bin"), home.join(".claude/local")] {
            if !directories.contains(&extra) {
                directories.push(extra);
            }
        }
    }
    std::env::join_paths(directories).ok()
}

fn locate(provider: AgentProvider, search: Option<&OsString>) -> Option<PathBuf> {
    let names: &[String] = if cfg!(windows) {
        &[
            format!("{}.exe", program(provider)),
            format!("{}.cmd", program(provider)),
        ]
    } else {
        &[program(provider).to_owned()]
    };
    std::env::split_paths(search?).find_map(|directory| {
        names
            .iter()
            .map(|name| directory.join(name))
            .find(|candidate| executable(candidate))
    })
}

#[cfg(unix)]
fn executable(candidate: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    fs::metadata(candidate)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn executable(candidate: &Path) -> bool {
    candidate.is_file()
}

fn detect(provider: AgentProvider, search: Option<&OsString>) -> Option<Found> {
    let path = locate(provider, search)?;
    let owner = owner(provider, &path, search);
    let mut command = Command::new(&path);
    command.arg("--version");
    prepare(&mut command, search, owner.first_on_path());
    let mut output = String::new();
    let version = run(command, VERSION_TIMEOUT, |line| {
        if output.is_empty() {
            line.clone_into(&mut output);
        }
    })
    .ok()
    .and_then(|_| parse_version(&output));
    Some(Found {
        path,
        version,
        owner,
    })
}

fn parse_version(output: &str) -> Option<String> {
    output.split_whitespace().find_map(|token| {
        let token = token
            .trim_matches(|character: char| matches!(character, '(' | ')' | ','))
            .trim_start_matches('v');
        let mut parts = token.split('.');
        let numeric = parts.next().is_some_and(|first| {
            !first.is_empty() && first.bytes().all(|byte| byte.is_ascii_digit())
        });
        (numeric && parts.next().is_some()).then(|| token.to_owned())
    })
}

fn owner(provider: AgentProvider, path: &Path, search: Option<&OsString>) -> Owner {
    let mut real = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if path.parent().and_then(Path::file_name) == Some("shims".as_ref())
        && real.file_name() == Some("mise".as_ref())
    {
        match mise_which(&real, provider, search) {
            Some(target) => real = target,
            None => {
                return Owner::Manual(ManualUpdate {
                    owner: "mise".to_owned(),
                    command: None,
                });
            }
        }
    }
    if let Some(prefix) = npm_prefix(&real) {
        return npm_owner(provider, &prefix);
    }
    if let Some(owner) = homebrew(&real) {
        return owner;
    }
    if let Some(manual) = mise_install(&real) {
        return Owner::Manual(manual);
    }
    system_owner(provider, &real, search).unwrap_or(Owner::Updater)
}

fn npm_package(provider: AgentProvider) -> &'static str {
    match provider {
        AgentProvider::ClaudeCode => "@anthropic-ai/claude-code",
        AgentProvider::Codex => "@openai/codex",
    }
}

fn npm_prefix(real: &Path) -> Option<PathBuf> {
    let text = real.to_str()?;
    let (prefix, _) = text.split_once("/lib/node_modules/")?;
    Some(PathBuf::from(prefix))
}

fn npm_owner(provider: AgentProvider, prefix: &Path) -> Owner {
    let bin = prefix.join("bin");
    let npm = bin.join("npm");
    let npm = executable(&npm).then_some(npm);
    if writable(&prefix.join("lib/node_modules")) {
        return Owner::Npm {
            bin: npm.map(|_| bin),
        };
    }
    let npm = npm.map_or_else(|| "npm".to_owned(), |npm| npm.display().to_string());
    Owner::Manual(ManualUpdate {
        owner: format!("npm under {}", prefix.display()),
        command: Some(format!(
            "sudo {npm} install -g {}@latest",
            npm_package(provider)
        )),
    })
}

#[cfg(unix)]
fn writable(directory: &Path) -> bool {
    rustix::fs::access(directory, rustix::fs::Access::WRITE_OK).is_ok()
}

#[cfg(not(unix))]
fn writable(directory: &Path) -> bool {
    fs::metadata(directory).is_ok_and(|metadata| !metadata.permissions().readonly())
}

fn homebrew(real: &Path) -> Option<Owner> {
    let components = real.components().collect::<Vec<_>>();
    let index = components.iter().position(|component| {
        matches!(component.as_os_str().to_str(), Some("Caskroom" | "Cellar"))
    })?;
    let cask = components[index].as_os_str() == "Caskroom";
    let token = components.get(index + 1)?.as_os_str().to_str()?.to_owned();
    let prefix = components[..index].iter().collect::<PathBuf>();
    let brew = prefix.join("bin/brew");
    executable(&brew).then_some(Owner::Homebrew { brew, cask, token })
}

fn mise_which(mise: &Path, provider: AgentProvider, search: Option<&OsString>) -> Option<PathBuf> {
    let mut command = Command::new(mise);
    command.args(["which", program(provider)]);
    prepare(&mut command, search, None);
    let mut target = None;
    run(command, VERSION_TIMEOUT, |line| {
        target.get_or_insert_with(|| PathBuf::from(line));
    })
    .ok()?;
    fs::canonicalize(target?).ok()
}

fn mise_install(real: &Path) -> Option<ManualUpdate> {
    let tool = real.ancestors().find(|ancestor| {
        ancestor.parent().and_then(Path::file_name) == Some("installs".as_ref())
            && ancestor.join(".mise.backend.toml").is_file()
    })?;
    let backend = fs::read_to_string(tool.join(".mise.backend.toml")).ok()?;
    let short = backend.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key.trim() == "short").then(|| value.trim().trim_matches('"').to_owned())
    })?;
    Some(ManualUpdate {
        owner: "mise".to_owned(),
        command: Some(format!("mise use -g {short}@latest")),
    })
}

fn system_owner(provider: AgentProvider, real: &Path, search: Option<&OsString>) -> Option<Owner> {
    let text = real.to_string_lossy().replace('\\', "/");
    let manual = |owner: &str, command: Option<String>| {
        Some(Owner::Manual(ManualUpdate {
            owner: owner.to_owned(),
            command,
        }))
    };
    if text.to_ascii_lowercase().contains("/winget/") {
        return manual(
            "winget",
            (provider == AgentProvider::ClaudeCode)
                .then(|| "winget upgrade Anthropic.ClaudeCode".to_owned()),
        );
    }
    if text.starts_with("/nix/store/") {
        return manual("Nix", None);
    }
    if let Some(rest) = text.strip_prefix("/snap/") {
        let name = rest.split('/').next().unwrap_or_default();
        return manual("Snap", Some(format!("sudo snap refresh {name}")));
    }
    let system = ["/usr/", "/bin/", "/sbin/", "/opt/"]
        .iter()
        .any(|prefix| text.starts_with(prefix))
        && !text.starts_with("/usr/local/")
        && !text.starts_with("/opt/homebrew/");
    if !system {
        return None;
    }
    if let Some(manual) = system_package(real, search) {
        return Some(Owner::Manual(manual));
    }
    let directory = real.parent()?;
    (!writable(directory)).then(|| {
        Owner::Manual(ManualUpdate {
            owner: format!("the system ({})", directory.display()),
            command: None,
        })
    })
}

fn system_package(real: &Path, search: Option<&OsString>) -> Option<ManualUpdate> {
    let path = real.to_str()?;
    let query = |program: &str, args: &[&str]| {
        let mut command = Command::new(program);
        command.args(args).arg(path);
        prepare(&mut command, search, None);
        let mut first = None;
        run(command, VERSION_TIMEOUT, |line| {
            first.get_or_insert_with(|| line.to_owned());
        })
        .ok()
        .and(first)
    };
    let installed = |program: &str| {
        search.is_some_and(|search| {
            std::env::split_paths(search).any(|directory| executable(&directory.join(program)))
        })
    };
    if let Some(line) = query("dpkg-query", &["-S"]) {
        let name = line.split(':').next()?.trim().to_owned();
        return Some(ManualUpdate {
            owner: format!("apt ({name})"),
            command: Some(format!("sudo apt install --only-upgrade {name}")),
        });
    }
    if let Some(name) = query("pacman", &["-Qqo"]) {
        let command = ["paru", "yay"]
            .into_iter()
            .find(|helper| installed(helper))
            .map_or_else(
                || format!("sudo pacman -S {name}"),
                |helper| format!("{helper} -S {name}"),
            );
        return Some(ManualUpdate {
            owner: format!("pacman ({name})"),
            command: Some(command),
        });
    }
    let name = query("rpm", &["-qf", "--qf", "%{NAME}"])?;
    Some(ManualUpdate {
        owner: format!("rpm ({name})"),
        command: Some(format!("sudo dnf upgrade {name}")),
    })
}

fn claude_channel(found: Option<&Found>) -> &'static str {
    if let Some(Found {
        owner: Owner::Homebrew {
            cask: true, token, ..
        },
        ..
    }) = found
    {
        return if token.ends_with("@latest") {
            "latest"
        } else {
            "stable"
        };
    }
    let configured = home()
        .and_then(|home| fs::read(home.join(".claude/settings.json")).ok())
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|settings| {
            settings
                .get("autoUpdatesChannel")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        });
    if configured.as_deref() == Some("stable") {
        "stable"
    } else {
        "latest"
    }
}

fn fetch_latest(
    provider: AgentProvider,
    found: Option<&Found>,
    search: Option<&OsString>,
) -> Option<String> {
    let url = match provider {
        AgentProvider::ClaudeCode => format!(
            "https://downloads.claude.ai/claude-code-releases/{}",
            claude_channel(found)
        ),
        AgentProvider::Codex => "https://registry.npmjs.org/@openai%2fcodex/latest".to_owned(),
    };
    let mut command = Command::new("curl");
    command.args(["-fsSL", "--max-time", "10", &url]);
    prepare(&mut command, search, None);
    let mut body = String::new();
    run(command, LATEST_TIMEOUT, |line| body.push_str(line)).ok()?;
    let version = match provider {
        AgentProvider::ClaudeCode => body.trim().to_owned(),
        AgentProvider::Codex => serde_json::from_str::<serde_json::Value>(&body)
            .ok()?
            .get("version")?
            .as_str()?
            .to_owned(),
    };
    parse_version(&version)
}

fn task_command(
    provider: AgentProvider,
    found: Option<&Found>,
    search: Option<&OsString>,
) -> Result<Command, String> {
    let mut command = match found {
        None if cfg!(windows) => {
            return Err(format!(
                "Install {} from its website on Windows.",
                provider.label()
            ));
        }
        None => {
            let mut command = Command::new("sh");
            command.arg("-c").arg(match provider {
                AgentProvider::ClaudeCode => CLAUDE_INSTALL,
                AgentProvider::Codex => CODEX_INSTALL,
            });
            command
        }
        Some(Found {
            owner: Owner::Manual(manual),
            ..
        }) => return Err(manual_message(manual)),
        Some(Found {
            owner: Owner::Homebrew { brew, cask, token },
            ..
        }) => {
            let mut command = Command::new(brew);
            command.arg("upgrade");
            if *cask {
                command.arg("--cask");
            }
            command.arg(token);
            command
        }
        Some(found) => {
            let mut command = Command::new(&found.path);
            command.arg("update");
            command
        }
    };
    prepare(
        &mut command,
        search,
        found.and_then(|found| found.owner.first_on_path()),
    );
    Ok(command)
}

fn prepare(command: &mut Command, search: Option<&OsString>, first: Option<&Path>) {
    let mut directories = first.map(Path::to_path_buf).into_iter().collect::<Vec<_>>();
    if let Some(search) = search {
        directories.extend(std::env::split_paths(search));
    }
    if !directories.is_empty()
        && let Ok(path) = std::env::join_paths(directories)
    {
        command.env("PATH", path);
    }
    command
        .env("NONINTERACTIVE", "1")
        .env("HOMEBREW_NO_ENV_HINTS", "1")
        .env("NO_COLOR", "1")
        .env("TERM", "dumb")
        .stdin(Stdio::null());
    if let Some(home) = home() {
        command.current_dir(home);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        command.process_group(0);
    }
}

fn run(
    mut command: Command,
    timeout: Duration,
    mut on_line: impl FnMut(&str),
) -> Result<ExitStatus, String> {
    let (reader, writer) = std::io::pipe().map_err(|error| error.to_string())?;
    let writer_err = writer.try_clone().map_err(|error| error.to_string())?;
    command.stdout(writer).stderr(writer_err);
    let mut child = command
        .spawn_unmasked()
        .map_err(|error| format!("Could not run {}: {error}", command.get_program().display()))?;
    drop(command);
    let (lines, received) = mpsc::channel();
    let pump = thread::Builder::new()
        .name("zz-providers-output".to_owned())
        .spawn(move || {
            let mut reader = BufReader::new(reader);
            let mut buffer = Vec::new();
            loop {
                buffer.clear();
                match reader.read_until(b'\n', &mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        for line in String::from_utf8_lossy(&buffer).split('\r') {
                            let line = plain(line);
                            if !line.is_empty() && lines.send(line).is_err() {
                                return;
                            }
                        }
                    }
                }
            }
        })
        .map_err(|error| error.to_string())?;
    let deadline = Instant::now() + timeout;
    let mut tail = VecDeque::with_capacity(FAILURE_LINES);
    let mut exited: Option<(ExitStatus, Instant)> = None;
    let status = loop {
        match received.recv_timeout(Duration::from_millis(50)) {
            Ok(line) => {
                on_line(&line);
                if tail.len() == FAILURE_LINES {
                    tail.pop_front();
                }
                tail.push_back(line);
                continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let status = match exited {
                    Some((status, _)) => status,
                    None => child.wait().map_err(|error| error.to_string())?,
                };
                let _ = pump.join();
                break status;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        match exited {
            Some((status, at)) if at.elapsed() >= OUTPUT_GRACE => break status,
            Some(_) => {}
            None => {
                if let Ok(Some(status)) = child.try_wait() {
                    exited = Some((status, Instant::now()));
                }
            }
        }
        if exited.is_none() && Instant::now() >= deadline {
            kill_group(&mut child);
            return Err(format!(
                "Timed out after {} minutes.",
                timeout.as_secs().div_ceil(60)
            ));
        }
    };
    if status.success() {
        Ok(status)
    } else {
        let mut message = Vec::from(tail).join("\n");
        if message.is_empty() {
            message = format!("Exited with {status}.");
        }
        Err(message)
    }
}

fn kill_group(child: &mut std::process::Child) {
    #[cfg(unix)]
    if let Some(pid) = i32::try_from(child.id())
        .ok()
        .and_then(rustix::process::Pid::from_raw)
    {
        let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn plain(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut characters = line.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\u{1b}' {
            match characters.next() {
                Some('[') => {
                    for next in characters.by_ref() {
                        if ('@'..='~').contains(&next) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    while let Some(next) = characters.next() {
                        if next == '\u{7}' {
                            break;
                        }
                        if next == '\u{1b}' && characters.peek() == Some(&'\\') {
                            characters.next();
                            break;
                        }
                    }
                }
                _ => {}
            }
        } else if !character.is_control() {
            output.push(character);
        }
    }
    output.trim().to_owned()
}

fn bounded(text: &str) -> String {
    if text.len() <= MAX_PROVIDER_TEXT_BYTES {
        return text.to_owned();
    }
    let mut end = MAX_PROVIDER_TEXT_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_come_from_the_first_numeric_token() {
        assert_eq!(
            parse_version("2.1.309 (Claude Code)").as_deref(),
            Some("2.1.309")
        );
        assert_eq!(
            parse_version("codex-cli 0.163.0").as_deref(),
            Some("0.163.0")
        );
        assert_eq!(parse_version("v1.2.3,").as_deref(), Some("1.2.3"));
        assert_eq!(parse_version("command not found"), None);
        assert_eq!(parse_version("12 errors"), None);
    }

    #[cfg(unix)]
    fn root() -> bool {
        rustix::process::geteuid().is_root()
    }

    #[cfg(unix)]
    #[test]
    fn homebrew_installs_upgrade_through_their_own_brew() {
        let dir = tempfile::tempdir().unwrap();
        let brew = dir.path().join("bin/brew");
        fs::create_dir_all(brew.parent().unwrap()).unwrap();
        script(&brew, "");
        let cask = dir
            .path()
            .join("Caskroom/claude-code@latest/2.1.309/claude");
        assert_eq!(
            homebrew(&cask),
            Some(Owner::Homebrew {
                brew: brew.clone(),
                cask: true,
                token: "claude-code@latest".to_owned()
            })
        );
        let formula = dir.path().join("Cellar/codex/0.163.0/bin/codex");
        assert_eq!(
            homebrew(&formula),
            Some(Owner::Homebrew {
                brew,
                cask: false,
                token: "codex".to_owned()
            })
        );
        assert_eq!(
            homebrew(Path::new("/home/me/.local/share/claude/versions/2.1.309")),
            None
        );
    }

    #[cfg(unix)]
    #[test]
    fn npm_installs_update_with_their_own_npm_unless_the_prefix_is_read_only() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        let prefix = fs::canonicalize(dir.path())
            .unwrap()
            .join("Cellar/node/26.12.1");
        let npm = prefix.join("bin/npm");
        fs::create_dir_all(npm.parent().unwrap()).unwrap();
        script(&npm, "");
        let codex = prefix.join("lib/node_modules/@openai/codex/bin/codex.js");
        fs::create_dir_all(codex.parent().unwrap()).unwrap();
        script(&codex, "");
        assert_eq!(
            owner(AgentProvider::Codex, &codex, None),
            Owner::Npm {
                bin: Some(prefix.join("bin"))
            }
        );
        if root() {
            return;
        }
        let modules = prefix.join("lib/node_modules");
        fs::set_permissions(&modules, fs::Permissions::from_mode(0o555)).unwrap();
        let owned = owner(AgentProvider::Codex, &codex, None);
        fs::set_permissions(&modules, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            owned,
            Owner::Manual(ManualUpdate {
                owner: format!("npm under {}", prefix.display()),
                command: Some(format!(
                    "sudo {} install -g @openai/codex@latest",
                    npm.display()
                )),
            })
        );
    }

    #[cfg(unix)]
    #[test]
    fn mise_tools_and_their_shims_stay_with_mise() {
        let dir = tempfile::tempdir().unwrap();
        let tool = dir.path().join("mise/installs/npm-openai-codex");
        let codex = tool.join("0.163.0/bin/codex");
        fs::create_dir_all(codex.parent().unwrap()).unwrap();
        script(&codex, "");
        fs::write(
            tool.join(".mise.backend.toml"),
            "short = \"npm:@openai/codex\"\nfull = \"npm:@openai/codex\"\n",
        )
        .unwrap();
        let expected = Owner::Manual(ManualUpdate {
            owner: "mise".to_owned(),
            command: Some("mise use -g npm:@openai/codex@latest".to_owned()),
        });
        assert_eq!(owner(AgentProvider::Codex, &codex, None), expected);
        let mise = dir.path().join("mise-bin/mise");
        fs::create_dir_all(mise.parent().unwrap()).unwrap();
        script(&mise, &format!("echo {}", codex.display()));
        let shim = dir.path().join("mise/shims/codex");
        fs::create_dir_all(shim.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&mise, &shim).unwrap();
        assert_eq!(owner(AgentProvider::Codex, &shim, None), expected);
    }

    #[cfg(unix)]
    #[test]
    fn system_installs_name_their_package_manager() {
        let manual = |owner: &str, command: Option<&str>| {
            Some(Owner::Manual(ManualUpdate {
                owner: owner.to_owned(),
                command: command.map(str::to_owned),
            }))
        };
        assert_eq!(
            system_owner(
                AgentProvider::Codex,
                Path::new("/nix/store/abc-codex-0.163.0/bin/codex"),
                None
            ),
            manual("Nix", None)
        );
        assert_eq!(
            system_owner(
                AgentProvider::Codex,
                Path::new("/snap/codex/12/bin/codex"),
                None
            ),
            manual("Snap", Some("sudo snap refresh codex"))
        );
        assert_eq!(
            system_owner(
                AgentProvider::ClaudeCode,
                Path::new("C:/Users/me/AppData/Local/Microsoft/WinGet/Packages/claude.exe"),
                None
            ),
            manual("winget", Some("winget upgrade Anthropic.ClaudeCode"))
        );
        assert_eq!(
            system_owner(
                AgentProvider::ClaudeCode,
                Path::new("/home/me/.local/share/claude/versions/2.1.309"),
                None
            ),
            None
        );
        let dir = tempfile::tempdir().unwrap();
        script(&dir.path().join("pacman"), "echo claude-code");
        script(&dir.path().join("paru"), "");
        let search = std::env::join_paths([dir.path()]).unwrap();
        assert_eq!(
            system_owner(
                AgentProvider::ClaudeCode,
                Path::new("/usr/bin/claude"),
                Some(&search)
            ),
            manual("pacman (claude-code)", Some("paru -S claude-code"))
        );
        if !root() {
            assert_eq!(
                system_owner(
                    AgentProvider::ClaudeCode,
                    Path::new("/usr/bin/claude"),
                    None
                ),
                manual("the system (/usr/bin)", None)
            );
        }
    }

    #[test]
    fn a_manual_install_is_never_updated() {
        let found = Found {
            path: PathBuf::from("/usr/bin/codex"),
            version: Some("0.159.0".to_owned()),
            owner: Owner::Manual(ManualUpdate {
                owner: "npm under /usr".to_owned(),
                command: Some("sudo /usr/bin/npm install -g @openai/codex@latest".to_owned()),
            }),
        };
        assert_eq!(
            task_command(AgentProvider::Codex, Some(&found), None).err(),
            Some(
                "Installed by npm under /usr. Update it with: sudo /usr/bin/npm install -g \
                 @openai/codex@latest"
                    .to_owned()
            )
        );
    }

    #[test]
    fn output_lines_lose_their_escapes_and_progress_returns() {
        assert_eq!(plain("\u{1b}[32m✔\u{1b}[0m Updated"), "✔ Updated");
        assert_eq!(plain("\u{1b}]0;title\u{7}done  "), "done");
        assert_eq!(plain("tab\there"), "tabhere");
    }

    #[cfg(unix)]
    #[test]
    fn run_streams_lines_and_reports_the_last_ones_on_failure() {
        let mut command = Command::new("sh");
        command.args([
            "-c",
            "echo one; printf 'two\\rthree\\n' >&2; echo four; exit 3",
        ]);
        let mut seen = Vec::new();
        let error = run(command, Duration::from_secs(10), |line| {
            seen.push(line.to_owned());
        })
        .unwrap_err();
        assert_eq!(seen, ["one", "two", "three", "four"]);
        assert_eq!(error, "two\nthree\nfour");
    }

    #[cfg(unix)]
    fn script(path: &Path, body: &str) {
        use std::os::unix::fs::PermissionsExt as _;
        fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[cfg(unix)]
    fn next_report(
        mailbox: &OutboundMailbox,
        done: impl Fn(&ProvidersReport) -> bool,
    ) -> (ProvidersReport, Vec<ProvidersReport>) {
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut seen = Vec::new();
        loop {
            for message in super::super::tests::take_reliable_messages(mailbox) {
                if let ProtocolMessage::Event(Event {
                    payload: EventPayload::Providers(report),
                    ..
                }) = message
                {
                    if done(&report) {
                        return (report, seen);
                    }
                    seen.push(report);
                }
            }
            assert!(Instant::now() < deadline, "no matching report in {seen:#?}");
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[cfg(unix)]
    #[test]
    fn providers_checks_updates_and_installs_with_the_vendor_tools() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        fs::create_dir_all(&bin).unwrap();
        let state = dir.path().join("claude-version");
        fs::write(&state, "2.1.308").unwrap();
        script(
            &bin.join("claude"),
            &format!(
                "case \"$1\" in\n--version) echo \"$(cat {state}) (Claude Code)\" ;;\n\
                 update) [ -f {fail} ] && {{ echo 'npm error code EACCES' >&2; exit 1; }}\n\
                 echo 'Checking for updates'; printf 'Downloading\\r50%%\\r'; echo 2.1.309 > {state}; echo 'Updated to 2.1.309' ;;\nesac",
                state = state.display(),
                fail = dir.path().join("fail").display(),
            ),
        );
        let installer = dir.path().join("install-codex.sh");
        fs::write(
            &installer,
            format!(
                "echo 'Installing Codex'\nprintf '#!/bin/sh\\necho codex-cli 0.164.0\\n' > {codex}\nchmod +x {codex}\n",
                codex = bin.join("codex").display(),
            ),
        )
        .unwrap();
        script(
            &bin.join("curl"),
            &format!(
                "for url; do :; done\ncase \"$url\" in\n\
                 *claude-code-releases/latest) echo 2.1.309 ;;\n\
                 *registry.npmjs.org*) echo '{{\"name\":\"@openai/codex\",\"version\":\"0.164.0\"}}' ;;\n\
                 *codex/install.sh) cat {installer} ;;\n*) exit 22 ;;\nesac",
                installer = installer.display(),
            ),
        );
        let shared = Arc::new(Shared::new(1));
        let mailbox = OutboundMailbox::new();
        let (client, _) =
            shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
        shared.providers.lock().search = Some(
            std::env::join_paths([bin.as_path(), Path::new("/usr/bin"), Path::new("/bin")])
                .unwrap(),
        );
        let run = |args: &[&str]| {
            shared
                .execute(
                    client,
                    ClientKind::Interactive,
                    &mut ExecutionContext::default(),
                    &CommandInvocation::new("providers", args.iter().copied()),
                )
                .unwrap();
        };

        run(&[]);
        let (report, _) = next_report(&mailbox, |report| !report.checking);
        let claude = report.provider(AgentProvider::ClaudeCode).unwrap();
        assert_eq!(claude.path.as_deref(), bin.join("claude").to_str());
        assert_eq!(claude.version.as_deref(), Some("2.1.308"));
        assert_eq!(claude.latest.as_deref(), Some("2.1.309"));
        assert!(claude.outdated());
        let codex = report.provider(AgentProvider::Codex).unwrap();
        assert!(!codex.installed());
        assert_eq!(codex.latest.as_deref(), Some("0.164.0"));

        run(&["-u", "claude-code"]);
        let (report, seen) = next_report(&mailbox, |report| {
            report
                .provider(AgentProvider::ClaudeCode)
                .is_some_and(|status| status.task == ProviderTask::Idle)
        });
        assert!(seen.iter().any(|report| matches!(
            &report.provider(AgentProvider::ClaudeCode).unwrap().task,
            ProviderTask::Running { .. }
        )));
        let claude = report.provider(AgentProvider::ClaudeCode).unwrap();
        assert_eq!(claude.version.as_deref(), Some("2.1.309"));
        assert!(!claude.outdated());

        run(&["-u", "codex"]);
        let (report, _) = next_report(&mailbox, |report| {
            report
                .provider(AgentProvider::Codex)
                .is_some_and(|status| status.installed() && status.task == ProviderTask::Idle)
        });
        let codex = report.provider(AgentProvider::Codex).unwrap();
        assert_eq!(codex.version.as_deref(), Some("0.164.0"));

        fs::write(dir.path().join("fail"), "").unwrap();
        run(&["-u", "claude"]);
        let (report, _) = next_report(&mailbox, |report| {
            report
                .provider(AgentProvider::ClaudeCode)
                .is_some_and(|status| matches!(status.task, ProviderTask::Failed { .. }))
        });
        assert_eq!(
            report.provider(AgentProvider::ClaudeCode).unwrap().task,
            ProviderTask::Failed {
                message: "npm error code EACCES".into()
            }
        );
        assert!(
            shared
                .execute(
                    client,
                    ClientKind::Interactive,
                    &mut ExecutionContext::default(),
                    &CommandInvocation::new("providers", ["-u", "cursor"]),
                )
                .is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn run_kills_a_command_past_its_deadline() {
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 30"]);
        prepare(&mut command, None, None);
        let started = Instant::now();
        let error = run(command, Duration::from_millis(200), |_| {}).unwrap_err();
        assert!(error.starts_with("Timed out"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
