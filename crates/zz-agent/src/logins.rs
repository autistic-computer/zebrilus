use std::{
    collections::BTreeMap,
    fmt, fs, io,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};

use serde::{Deserialize, Serialize};
use zz_protocol::{AgentAccount, AgentProvider};

const RECORD_FILE: &str = "logins.json";
#[cfg(not(target_os = "macos"))]
const PARKED_DIRECTORY: &str = "parked";

static LOCK: Mutex<()> = Mutex::new(());

#[must_use]
pub const fn swaps(provider: AgentProvider) -> bool {
    matches!(provider, AgentProvider::ClaudeCode | AgentProvider::Codex)
}

#[derive(Clone, PartialEq, Eq)]
pub struct Login(Vec<u8>);

impl Login {
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }

    #[must_use]
    pub fn owner(&self) -> Option<Owner> {
        let login = serde_json::from_slice::<serde_json::Value>(&self.0).ok()?;
        let id = |key: &str| {
            login["claudeAiOauth"][key]
                .as_str()
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
        };
        Some(Owner {
            account: id("accountUuid")?,
            organization: id("organizationUuid")?,
        })
    }
}

impl fmt::Debug for Login {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Login(..)")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Owner {
    pub account: String,
    pub organization: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Probe {
    pub account: AgentAccount,
    pub organization: Option<String>,
}

impl Probe {
    fn names(&self, owner: Option<&Owner>) -> bool {
        match (owner, self.organization.as_deref()) {
            (Some(owner), Some(named)) => owner.organization == named,
            _ => true,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slot {
    pub id: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub organization: Option<String>,
    #[serde(default)]
    pub plan: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<Owner>,
}

impl Slot {
    fn named(id: &str, account: &AgentAccount) -> Self {
        Self {
            id: id.to_owned(),
            email: account.email.clone(),
            organization: account.organization.clone(),
            plan: account.plan.clone(),
            owner: None,
        }
    }

    fn settled(id: &str, probe: &Probe, owner: Option<Owner>, before: Option<&Self>) -> Self {
        if probe.names(owner.as_ref()) {
            return Self {
                owner,
                ..Self::named(id, &probe.account)
            };
        }
        let kept = before.filter(|before| match &before.owner {
            Some(recorded) => Some(recorded) == owner.as_ref(),
            None => !before.holds(&probe.account),
        });
        Self {
            id: id.to_owned(),
            email: kept.and_then(|slot| slot.email.clone()),
            organization: kept.and_then(|slot| slot.organization.clone()),
            plan: kept.and_then(|slot| slot.plan.clone()),
            owner,
        }
    }

    fn holds(&self, account: &AgentAccount) -> bool {
        account.email.is_some()
            && self.email == account.email
            && self.organization == account.organization
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Logins {
    #[serde(default)]
    pub active: Option<String>,
    #[serde(default)]
    pub slots: Vec<Slot>,
}

impl Logins {
    #[must_use]
    pub fn active(&self) -> Option<&str> {
        self.active.as_deref()
    }

    #[must_use]
    pub fn slot(&self, id: &str) -> Option<&Slot> {
        self.slots.iter().find(|slot| slot.id == id)
    }

    #[must_use]
    pub fn holding(&self, account: &AgentAccount) -> Option<&str> {
        self.slots
            .iter()
            .find(|slot| slot.holds(account))
            .map(|slot| slot.id.as_str())
    }

    #[must_use]
    pub fn owned_by(&self, owner: &Owner) -> Option<&str> {
        self.slots
            .iter()
            .find(|slot| slot.owner.as_ref() == Some(owner))
            .map(|slot| slot.id.as_str())
    }

    fn named_by(&self, probe: &Probe, owner: Option<&Owner>) -> Option<&str> {
        if !probe.names(owner) {
            return None;
        }
        self.slots
            .iter()
            .find(|slot| slot.holds(&probe.account) && (slot.owner.is_none() || owner.is_none()))
            .map(|slot| slot.id.as_str())
    }

    fn learn(&mut self, id: &str, owner: Option<Owner>) {
        if let Some(slot) = self.slots.iter_mut().find(|slot| slot.id == id)
            && slot.owner.is_none()
        {
            slot.owner = owner;
        }
    }

    #[must_use]
    pub fn next_id(&self) -> String {
        (1..=self.slots.len() + 1)
            .map(|index| format!("a{index}"))
            .find(|id| self.slot(id).is_none())
            .expect("one more number than slots leaves an id free")
    }

    pub fn upsert(&mut self, slot: Slot) {
        if let Some(known) = self.slots.iter_mut().find(|known| known.id == slot.id) {
            *known = slot;
        } else {
            self.slots.push(slot);
            self.slots
                .sort_by(|left, right| order(&left.id).cmp(&order(&right.id)));
        }
    }
}

fn order(id: &str) -> (usize, &str) {
    (id.len(), id)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place<'a> {
    Native,
    Parked(&'a str),
}

pub trait Vault {
    fn read(&self, provider: AgentProvider, place: Place<'_>) -> io::Result<Option<Login>>;
    fn write(&self, provider: AgentProvider, place: Place<'_>, login: &Login) -> io::Result<()>;
    fn remove(&self, provider: AgentProvider, place: Place<'_>) -> io::Result<()>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Landing {
    Active,
    Parked,
}

pub fn swap(
    vault: &dyn Vault,
    provider: AgentProvider,
    logins: &mut Logins,
    target: &str,
) -> Result<(), String> {
    let label = provider.label();
    if logins.active() == Some(target) {
        return Ok(());
    }
    if logins.slot(target).is_none() {
        return Err(format!("{label} has no account {target}"));
    }
    let incoming = vault
        .read(provider, Place::Parked(target))
        .map_err(|error| format!("could not read the {label} account {target}: {error}"))?
        .ok_or_else(|| format!("the {label} account {target} is signed out"))?;
    let outgoing = logins.active.clone();
    if let Some(active) = outgoing.as_deref()
        && let Some(current) = vault
            .read(provider, Place::Native)
            .map_err(|error| format!("could not read the {label} login: {error}"))?
    {
        vault
            .write(provider, Place::Parked(active), &current)
            .map_err(|error| {
                format!("could not set the {label} account {active} aside: {error}")
            })?;
        logins.learn(active, owner_of(provider, &current));
    }
    if let Err(error) = place_native(vault, provider, &incoming) {
        if let Some(active) = outgoing.as_deref()
            && let Ok(Some(previous)) = vault.read(provider, Place::Parked(active))
            && place_native(vault, provider, &previous).is_ok()
        {
            let _ = vault.remove(provider, Place::Parked(active));
        }
        return Err(format!("could not sign {label} in as {target}: {error}"));
    }
    if let Err(error) = vault.remove(provider, Place::Parked(target)) {
        log::warn!(target: "zz::agent", "could not drop the parked {label} account {target}: {error}");
    }
    logins.active = Some(target.to_owned());
    logins.learn(target, owner_of(provider, &incoming));
    Ok(())
}

fn owner_of(provider: AgentProvider, login: &Login) -> Option<Owner> {
    (provider == AgentProvider::ClaudeCode)
        .then(|| login.owner())
        .flatten()
}

fn owner_at(vault: &dyn Vault, provider: AgentProvider, place: Place<'_>) -> Option<Owner> {
    if provider != AgentProvider::ClaudeCode {
        return None;
    }
    owner_of(provider, &vault.read(provider, place).ok().flatten()?)
}

fn learn_parked(vault: &dyn Vault, provider: AgentProvider, logins: &mut Logins) -> bool {
    let unknown = logins
        .slots
        .iter()
        .filter(|slot| slot.owner.is_none() && logins.active() != Some(slot.id.as_str()))
        .map(|slot| slot.id.clone())
        .collect::<Vec<_>>();
    let mut learned = false;
    for id in unknown {
        let owner = owner_at(vault, provider, Place::Parked(&id));
        learned |= owner.is_some();
        logins.learn(&id, owner);
    }
    learned
}

fn place_native(vault: &dyn Vault, provider: AgentProvider, login: &Login) -> io::Result<()> {
    vault.write(provider, Place::Native, login)?;
    match vault.read(provider, Place::Native)? {
        Some(written) if written == *login => Ok(()),
        _ => Err(io::Error::other("the login read back differs")),
    }
}

pub fn begin_sign_in(
    vault: &dyn Vault,
    provider: AgentProvider,
    logins: &Logins,
) -> io::Result<Option<String>> {
    let Some(active) = logins.active() else {
        return Ok(None);
    };
    let Some(current) = vault.read(provider, Place::Native)? else {
        return Ok(None);
    };
    vault.write(provider, Place::Parked(active), &current)?;
    Ok(Some(active.to_owned()))
}

pub fn abandon_sign_in(vault: &dyn Vault, provider: AgentProvider, copied: Option<&str>) {
    let Some(copied) = copied else {
        return;
    };
    let label = provider.label();
    if matches!(vault.read(provider, Place::Native), Ok(None))
        && let Ok(Some(previous)) = vault.read(provider, Place::Parked(copied))
        && let Err(error) = place_native(vault, provider, &previous)
    {
        log::warn!(target: "zz::agent", "could not put the {label} account {copied} back: {error}");
        return;
    }
    if let Err(error) = vault.remove(provider, Place::Parked(copied)) {
        log::warn!(target: "zz::agent", "could not drop the {label} sign-in's copy of {copied}: {error}");
    }
}

pub fn finish_sign_in(
    vault: &dyn Vault,
    provider: AgentProvider,
    logins: &mut Logins,
    copied: Option<&str>,
    probe: &Probe,
    asked: Option<&str>,
    busy: bool,
) -> Result<(String, Landing), String> {
    let label = provider.label();
    let owner = owner_at(vault, provider, Place::Native);
    let target = owner
        .as_ref()
        .and_then(|owner| logins.owned_by(owner))
        .or_else(|| asked.filter(|id| logins.slot(id).is_some()))
        .or_else(|| logins.named_by(probe, owner.as_ref()))
        .map_or_else(|| logins.next_id(), str::to_owned);
    let before = logins.slot(&target).filter(|slot| slot.owner.is_some());
    let slot = Slot::settled(&target, probe, owner, before);
    logins.upsert(slot);
    if busy && copied.is_some_and(|copied| copied != target) {
        let fresh = vault
            .read(provider, Place::Native)
            .map_err(|error| format!("could not read the new {label} login: {error}"))?
            .ok_or_else(|| format!("the new {label} login is gone"))?;
        vault
            .write(provider, Place::Parked(&target), &fresh)
            .map_err(|error| format!("could not set the new {label} login aside: {error}"))?;
        if let Some(copied) = copied
            && let Some(previous) = vault
                .read(provider, Place::Parked(copied))
                .map_err(|error| format!("could not read the {label} account {copied}: {error}"))?
        {
            place_native(vault, provider, &previous).map_err(|error| {
                format!("could not put the {label} account {copied} back: {error}")
            })?;
            let _ = vault.remove(provider, Place::Parked(copied));
            logins.active = Some(copied.to_owned());
        }
        return Ok((target, Landing::Parked));
    }
    if copied == Some(target.as_str()) {
        abandon_sign_in(vault, provider, copied);
    }
    if let Err(error) = vault.remove(provider, Place::Parked(&target)) {
        log::warn!(target: "zz::agent", "could not drop the old {label} login of {target}: {error}");
    }
    logins.active = Some(target.clone());
    Ok((target, Landing::Active))
}

pub fn adopt(vault: &dyn Vault, logins: &mut Logins, probe: &Probe) -> String {
    let provider = probe.account.provider;
    let owner = owner_at(vault, provider, Place::Native);
    let known = owner
        .as_ref()
        .and_then(|owner| logins.owned_by(owner))
        .or_else(|| {
            (provider == AgentProvider::Codex)
                .then(|| logins.holding(&probe.account))
                .flatten()
        });
    let id = known.map_or_else(|| logins.next_id(), str::to_owned);
    let slot = Slot::settled(&id, probe, owner, logins.slot(&id));
    logins.upsert(slot);
    logins.active = Some(id.clone());
    if let Err(error) = vault.remove(provider, Place::Parked(&id)) {
        log::warn!(target: "zz::agent", "could not drop the parked {} account {id}: {error}", provider.label());
    }
    id
}

pub fn reconcile(vault: &dyn Vault, logins: &mut Logins, probe: &Probe) -> bool {
    let native = &probe.account;
    if !native.signed_in {
        return false;
    }
    let provider = native.provider;
    let Some(slot) = logins.active().and_then(|id| logins.slot(id)).cloned() else {
        adopt(vault, logins, probe);
        return true;
    };
    let found = owner_at(vault, provider, Place::Native);
    let learned = found.is_some() && slot.owner != found && learn_parked(vault, provider, logins);
    let (moved, settled) = match (provider, found) {
        (AgentProvider::Codex, _) if native.email.is_some() => {
            (!slot.holds(native), Slot::named(&slot.id, native))
        }
        (_, Some(found)) => (
            slot.owner.as_ref().is_some_and(|owner| *owner != found)
                || logins.owned_by(&found).is_some_and(|id| id != slot.id),
            Slot::settled(&slot.id, probe, Some(found), Some(&slot)),
        ),
        _ => return false,
    };
    if moved {
        log::info!(target: "zz::agent", "the {} login changed outside zz; {} no longer holds it", provider.label(), slot.id);
        logins.active = None;
        adopt(vault, logins, probe);
        return true;
    }
    if settled == slot {
        return learned;
    }
    logins.upsert(settled);
    true
}

pub fn migrate(
    vault: &dyn Vault,
    provider: AgentProvider,
    folders: &[(String, PathBuf)],
    name: impl Fn(&Path) -> Option<Probe>,
) -> Logins {
    let label = provider.label();
    let mut logins = Logins::default();
    for (id, home) in folders {
        let login = match legacy(provider, home) {
            Ok(Some(login)) => login,
            Ok(None) => continue,
            Err(error) => {
                log::warn!(target: "zz::agent", "could not read the {label} account {id}: {error}");
                continue;
            }
        };
        let Some(probe) = name(home).filter(|probe| probe.account.signed_in) else {
            continue;
        };
        if let Err(error) = vault.write(provider, Place::Parked(id), &login) {
            log::warn!(target: "zz::agent", "could not keep the {label} account {id}: {error}");
            continue;
        }
        logins.upsert(Slot::settled(id, &probe, owner_of(provider, &login), None));
    }
    logins
}

#[must_use]
pub fn listed(
    vault: &dyn Vault,
    provider: AgentProvider,
    logins: &Logins,
    native: Option<&AgentAccount>,
) -> Vec<AgentAccount> {
    let active = logins.active();
    let unnamed = active.is_none().then(|| AgentAccount {
        id: None,
        ..native.cloned().unwrap_or(AgentAccount {
            provider,
            id: None,
            signed_in: false,
            email: None,
            organization: None,
            plan: None,
        })
    });
    let slots = logins.slots.iter().map(|slot| {
        let signed_in = if active == Some(slot.id.as_str()) {
            native.is_none_or(|native| native.signed_in)
        } else {
            matches!(vault.read(provider, Place::Parked(&slot.id)), Ok(Some(_)))
        };
        AgentAccount {
            provider,
            id: Some(slot.id.clone()),
            signed_in,
            email: slot.email.clone(),
            organization: slot.organization.clone(),
            plan: slot.plan.clone(),
        }
    });
    unnamed.into_iter().chain(slots).collect()
}

#[must_use]
pub fn manages(provider: AgentProvider) -> bool {
    swaps(provider) && load(provider).is_some_and(|logins| logins.active().is_some())
}

pub fn guard() -> MutexGuard<'static, ()> {
    LOCK.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn root() -> io::Result<PathBuf> {
    if crate::sandboxed() {
        return Ok(test_root());
    }
    crate::paths::accounts_dir()
}

fn test_root() -> PathBuf {
    static ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    ROOT.get_or_init(|| {
        let root = std::env::temp_dir().join(format!("zz-logins-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        root
    })
    .clone()
}

fn record_path() -> io::Result<PathBuf> {
    Ok(root()?.join(RECORD_FILE))
}

fn read_record() -> BTreeMap<String, Logins> {
    record_path()
        .and_then(fs::read)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

#[must_use]
pub fn load(provider: AgentProvider) -> Option<Logins> {
    read_record().remove(provider.as_str())
}

pub fn store(provider: AgentProvider, logins: &Logins) -> io::Result<()> {
    let mut record = read_record();
    record.insert(provider.as_str().to_owned(), logins.clone());
    let bytes = serde_json::to_vec_pretty(&record).map_err(io::Error::other)?;
    write_private(&record_path()?, &bytes)
}

fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use zz_daemon_client::user_data::{
        restrict_directory_to_current_user, restrict_to_current_user,
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        restrict_directory_to_current_user(parent)?;
    }
    let staged = path.with_extension("zz-staged");
    fs::write(&staged, bytes)?;
    restrict_to_current_user(&staged)?;
    fs::rename(&staged, path)
}

#[derive(Clone, Copy, Debug, Default)]
pub struct System;

impl Vault for System {
    fn read(&self, provider: AgentProvider, place: Place<'_>) -> io::Result<Option<Login>> {
        match location(provider, place)? {
            Location::File(path) => match fs::read(&path) {
                Ok(bytes) => Ok(Some(Login(bytes))),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(error),
            },
            #[cfg(target_os = "macos")]
            Location::Keychain { service, account } => keychain::read(&service, &account),
        }
    }

    fn write(&self, provider: AgentProvider, place: Place<'_>, login: &Login) -> io::Result<()> {
        match location(provider, place)? {
            Location::File(path) => write_private(&path, login.bytes()),
            #[cfg(target_os = "macos")]
            Location::Keychain { service, account } => keychain::write(&service, &account, login),
        }
    }

    fn remove(&self, provider: AgentProvider, place: Place<'_>) -> io::Result<()> {
        match location(provider, place)? {
            Location::File(path) => match fs::remove_file(&path) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
                _ => Ok(()),
            },
            #[cfg(target_os = "macos")]
            Location::Keychain { service, account } => keychain::remove(&service, &account),
        }
    }
}

enum Location {
    File(PathBuf),
    #[cfg(target_os = "macos")]
    Keychain {
        service: String,
        account: String,
    },
}

fn location(provider: AgentProvider, place: Place<'_>) -> io::Result<Location> {
    let unsupported = || {
        io::Error::new(
            io::ErrorKind::Unsupported,
            format!("{} keeps one login", provider.label()),
        )
    };
    if crate::sandboxed() && swaps(provider) {
        let name = match place {
            Place::Native => "native",
            Place::Parked(id) => id,
        };
        return Ok(Location::File(
            test_root().join("vault").join(provider.as_str()).join(name),
        ));
    }
    match (provider, place) {
        (AgentProvider::ClaudeCode, Place::Native) => claude_native(),
        (AgentProvider::Codex, Place::Native) => codex_native(None),
        (AgentProvider::ClaudeCode | AgentProvider::Codex, Place::Parked(id)) => {
            parked(provider, id)
        }
        _ => Err(unsupported()),
    }
}

#[cfg(target_os = "macos")]
fn claude_native() -> io::Result<Location> {
    Ok(Location::Keychain {
        service: claude_service(std::env::var_os("CLAUDE_CONFIG_DIR").as_deref()),
        account: keychain::user()?,
    })
}

#[cfg(not(target_os = "macos"))]
fn claude_native() -> io::Result<Location> {
    let config = crate::claude::sessions::config_home()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no Claude Code folder"))?;
    Ok(Location::File(config.join(".credentials.json")))
}

fn codex_native(home: Option<&Path>) -> io::Result<Location> {
    let home = home
        .map(Path::to_path_buf)
        .or_else(crate::codex::codex_home)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no Codex folder"))?;
    Ok(Location::File(home.join("auth.json")))
}

#[cfg(target_os = "macos")]
fn parked(provider: AgentProvider, id: &str) -> io::Result<Location> {
    let root = crate::paths::accounts_dir()?;
    Ok(Location::Keychain {
        service: keychain::PARKED_SERVICE.to_owned(),
        account: format!("{}|{}|{id}", root.display(), provider.as_str()),
    })
}

#[cfg(not(target_os = "macos"))]
fn parked(provider: AgentProvider, id: &str) -> io::Result<Location> {
    Ok(Location::File(
        crate::paths::accounts_dir()?
            .join(PARKED_DIRECTORY)
            .join(provider.as_str())
            .join(id),
    ))
}

#[cfg(target_os = "macos")]
#[must_use]
pub fn claude_service(config: Option<&std::ffi::OsStr>) -> String {
    use sha2::{Digest as _, Sha256};

    const SERVICE: &str = "Claude Code-credentials";
    match config.filter(|config| !config.is_empty()) {
        None => SERVICE.to_owned(),
        Some(config) => {
            let digest = Sha256::digest(config.as_encoded_bytes());
            let suffix = hex(&digest[..4]);
            format!("{SERVICE}-{suffix}")
        }
    }
}

#[cfg(target_os = "macos")]
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        })
}

pub fn legacy(provider: AgentProvider, home: &Path) -> io::Result<Option<Login>> {
    if crate::sandboxed() {
        return Ok(None);
    }
    let location = match provider {
        #[cfg(target_os = "macos")]
        AgentProvider::ClaudeCode => Location::Keychain {
            service: claude_service(Some(home.as_os_str())),
            account: keychain::user()?,
        },
        #[cfg(not(target_os = "macos"))]
        AgentProvider::ClaudeCode => Location::File(home.join(".credentials.json")),
        AgentProvider::Codex => codex_native(Some(home))?,
        _ => return Ok(None),
    };
    match location {
        Location::File(path) => match fs::read(&path) {
            Ok(bytes) => Ok(Some(Login(bytes))),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        },
        #[cfg(target_os = "macos")]
        Location::Keychain { service, account } => keychain::read(&service, &account),
    }
}

#[cfg(target_os = "macos")]
mod keychain {
    use std::{
        io::{self, Write as _},
        process::{Command, Stdio},
    };

    use zz_daemon_client::unmasked::SpawnUnmasked as _;

    use super::Login;

    pub(super) const PARKED_SERVICE: &str = "zz-agent-login";
    const SECURITY: &str = "/usr/bin/security";
    const NOT_FOUND: i32 = 44;
    const TYPED_LINE_LIMIT: usize = 4094;

    pub(super) fn user() -> io::Result<String> {
        std::env::var("USER")
            .or_else(|_| std::env::var("LOGNAME"))
            .ok()
            .filter(|user| !user.is_empty())
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no user name"))
    }

    pub(super) fn read(service: &str, account: &str) -> io::Result<Option<Login>> {
        let output = Command::new(SECURITY)
            .args(["find-generic-password", "-a", account, "-s", service, "-w"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output_unmasked()?;
        if output.status.code() == Some(NOT_FOUND) {
            return Ok(None);
        }
        if !output.status.success() {
            return Err(io::Error::other(format!(
                "security find-generic-password exited with {}",
                output.status
            )));
        }
        let mut bytes = output.stdout;
        if bytes.last() == Some(&b'\n') {
            bytes.pop();
        }
        Ok(Some(Login(bytes)))
    }

    pub(super) fn write(service: &str, account: &str, login: &Login) -> io::Result<()> {
        if [service, account]
            .iter()
            .any(|value| value.contains(['"', '\\', '\n']))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a Keychain name holds a quote, backslash or line break",
            ));
        }
        let hex = super::hex(login.bytes());
        let status = if let Some(line) = typed_line(service, account, &hex) {
            let mut child = Command::new(SECURITY)
                .arg("-i")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn_unmasked()?;
            {
                let mut input = child
                    .stdin
                    .take()
                    .ok_or_else(|| io::Error::other("security has no stdin"))?;
                writeln!(input, "{line}")?;
            }
            child.wait()?
        } else {
            Command::new(SECURITY)
                .args(["add-generic-password", "-U", "-a", account, "-s", service])
                .args(["-X", &hex])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status_unmasked()?
        };
        if !status.success() {
            return Err(io::Error::other(format!(
                "security add-generic-password exited with {status}"
            )));
        }
        match read(service, account)? {
            Some(written) if written == *login => Ok(()),
            _ => Err(io::Error::other("the Keychain did not keep the login")),
        }
    }

    pub(super) fn typed_line(service: &str, account: &str, hex: &str) -> Option<String> {
        let line =
            format!("add-generic-password -U -a \"{account}\" -s \"{service}\" -X \"{hex}\"");
        (line.len() <= TYPED_LINE_LIMIT).then_some(line)
    }

    pub(super) fn remove(service: &str, account: &str) -> io::Result<()> {
        let status = Command::new(SECURITY)
            .args(["delete-generic-password", "-a", account, "-s", service])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status_unmasked()?;
        if status.success() || status.code() == Some(NOT_FOUND) {
            Ok(())
        } else {
            Err(io::Error::other(format!(
                "security delete-generic-password exited with {status}"
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    #[derive(Default)]
    struct Memory(RefCell<BTreeMap<String, Vec<u8>>>);

    impl Memory {
        fn key(place: Place<'_>) -> String {
            match place {
                Place::Native => "native".to_owned(),
                Place::Parked(id) => id.to_owned(),
            }
        }

        fn holds(&self) -> BTreeMap<String, String> {
            self.0
                .borrow()
                .iter()
                .map(|(key, bytes)| (key.clone(), String::from_utf8(bytes.clone()).unwrap()))
                .collect()
        }
    }

    impl Vault for Memory {
        fn read(&self, _: AgentProvider, place: Place<'_>) -> io::Result<Option<Login>> {
            Ok(self.0.borrow().get(&Self::key(place)).cloned().map(Login))
        }

        fn write(&self, _: AgentProvider, place: Place<'_>, login: &Login) -> io::Result<()> {
            self.0
                .borrow_mut()
                .insert(Self::key(place), login.0.clone());
            Ok(())
        }

        fn remove(&self, _: AgentProvider, place: Place<'_>) -> io::Result<()> {
            self.0.borrow_mut().remove(&Self::key(place));
            Ok(())
        }
    }

    fn account(email: &str) -> AgentAccount {
        AgentAccount {
            provider: AgentProvider::ClaudeCode,
            id: None,
            signed_in: true,
            email: Some(email.to_owned()),
            organization: Some(format!("{email}'s Organization")),
            plan: Some("max".to_owned()),
        }
    }

    fn probe(email: &str, organization: Option<&str>) -> Probe {
        Probe {
            account: account(email),
            organization: organization.map(str::to_owned),
        }
    }

    fn stored(owner: &str, token: &str) -> String {
        format!(
            r#"{{"claudeAiOauth":{{"accessToken":"{token}","accountUuid":"{owner}-user","organizationUuid":"{owner}-org"}}}}"#
        )
    }

    fn login(owner: &str, token: &str) -> Login {
        Login(stored(owner, token).into_bytes())
    }

    fn slot(id: &str, email: Option<&str>, owner: Option<&str>) -> Slot {
        Slot {
            id: id.to_owned(),
            email: email.map(str::to_owned),
            organization: email.map(|email| format!("{email}'s Organization")),
            plan: email.map(|_| "max".to_owned()),
            owner: owner.map(|owner| Owner {
                account: format!("{owner}-user"),
                organization: format!("{owner}-org"),
            }),
        }
    }

    fn holding(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn each_login_lives_in_exactly_one_place_through_swaps_and_sign_ins() {
        let claude = AgentProvider::ClaudeCode;
        let vault = Memory::default();
        let mut logins = Logins::default();
        vault
            .write(claude, Place::Native, &Login(b"work".to_vec()))
            .unwrap();
        assert_eq!(
            adopt(&vault, &mut logins, &probe("work@example.com", None)),
            "a1"
        );

        let copied = begin_sign_in(&vault, claude, &logins).unwrap();
        vault
            .write(claude, Place::Native, &Login(b"home".to_vec()))
            .unwrap();
        let landed = finish_sign_in(
            &vault,
            claude,
            &mut logins,
            copied.as_deref(),
            &probe("home@example.com", None),
            None,
            false,
        );
        assert_eq!(landed, Ok(("a2".to_owned(), Landing::Active)));
        assert_eq!(
            vault.holds(),
            holding(&[("native", "home"), ("a1", "work")])
        );

        swap(&vault, claude, &mut logins, "a1").unwrap();
        vault
            .write(claude, Place::Native, &Login(b"work, renewed".to_vec()))
            .unwrap();
        swap(&vault, claude, &mut logins, "a2").unwrap();
        assert_eq!(logins.active(), Some("a2"));
        assert_eq!(
            vault.holds(),
            holding(&[("native", "home"), ("a1", "work, renewed")])
        );

        let copied = begin_sign_in(&vault, claude, &logins).unwrap();
        vault
            .write(claude, Place::Native, &Login(b"work, again".to_vec()))
            .unwrap();
        let landed = finish_sign_in(
            &vault,
            claude,
            &mut logins,
            copied.as_deref(),
            &probe("work@example.com", None),
            None,
            true,
        );
        assert_eq!(landed, Ok(("a1".to_owned(), Landing::Parked)));
        assert_eq!(logins.active(), Some("a2"));
        assert_eq!(
            vault.holds(),
            holding(&[("native", "home"), ("a1", "work, again")])
        );
        assert_eq!(logins.slots.len(), 2);
    }

    #[test]
    fn a_login_named_from_another_accounts_cache_gives_way_to_the_slot_that_owns_it() {
        let claude = AgentProvider::ClaudeCode;
        let (home, work) = ("home@example.com", "work@example.com");
        let vault = Memory::default();
        let mut logins = Logins {
            active: Some("a2".to_owned()),
            slots: vec![slot("a1", Some(home), None), slot("a2", Some(work), None)],
        };
        vault
            .write(claude, Place::Parked("a1"), &login("home", "first"))
            .unwrap();
        vault
            .write(claude, Place::Native, &Login(b"no ids".to_vec()))
            .unwrap();
        let cached = probe(work, Some("work-org"));
        assert!(!reconcile(&vault, &mut logins, &cached));

        vault
            .write(claude, Place::Native, &login("home", "second"))
            .unwrap();
        assert!(reconcile(&vault, &mut logins, &cached));
        assert_eq!(logins.active(), Some("a1"));
        assert_eq!(
            logins.slots,
            [
                slot("a1", Some(home), Some("home")),
                slot("a2", Some(work), None)
            ]
        );
        let second = stored("home", "second");
        assert_eq!(vault.holds(), holding(&[("native", &second)]));

        let copied = begin_sign_in(&vault, claude, &logins).unwrap();
        vault
            .write(claude, Place::Native, &login("work", "third"))
            .unwrap();
        let landed = finish_sign_in(
            &vault,
            claude,
            &mut logins,
            copied.as_deref(),
            &cached,
            None,
            false,
        );
        assert_eq!(landed, Ok(("a2".to_owned(), Landing::Active)));
        assert_eq!(
            logins.slots,
            [
                slot("a1", Some(home), Some("home")),
                slot("a2", Some(work), Some("work"))
            ]
        );
        let third = stored("work", "third");
        assert_eq!(
            vault.holds(),
            holding(&[("native", &third), ("a1", &second)])
        );
    }

    #[test]
    fn a_login_takes_its_name_only_from_a_cache_of_its_own_organization() {
        let claude = AgentProvider::ClaudeCode;
        let (home, work) = ("home@example.com", "work@example.com");
        let vault = Memory::default();
        let mut logins = Logins::default();
        vault
            .write(claude, Place::Native, &login("home", "first"))
            .unwrap();
        let stale = probe(work, Some("work-org"));
        assert_eq!(adopt(&vault, &mut logins, &stale), "a1");
        assert_eq!(logins.slots, [slot("a1", None, Some("home"))]);

        let fresh = probe(home, Some("home-org"));
        assert!(reconcile(&vault, &mut logins, &fresh));
        assert_eq!(logins.slots, [slot("a1", Some(home), Some("home"))]);
        assert!(!reconcile(&vault, &mut logins, &stale));

        vault
            .write(claude, Place::Native, &login("work", "second"))
            .unwrap();
        assert!(reconcile(&vault, &mut logins, &fresh));
        assert_eq!(logins.active(), Some("a2"));
        assert_eq!(
            logins.slots,
            [
                slot("a1", Some(home), Some("home")),
                slot("a2", None, Some("work"))
            ]
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_login_too_long_for_one_typed_line_goes_in_as_arguments() {
        let fits = keychain::typed_line("s", "ab", &"0".repeat(4050));
        assert_eq!(fits.map(|line| line.len()), Some(4094));
        assert_eq!(keychain::typed_line("s", "ab", &"0".repeat(4052)), None);
    }
}
