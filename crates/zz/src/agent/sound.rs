use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock, PoisonError},
    time::Instant,
};

use zz_client::attention::{Chime, bell_due, chime_due, chime_wav};

const SOUND_ENV: &str = "ZZ_AGENT_SOUND";
const PLAYER_DEADLINE: std::time::Duration = std::time::Duration::from_secs(5);

static LAST_CHIME: Mutex<Option<Instant>> = Mutex::new(None);
static LAST_DISCONNECT: Mutex<Option<Instant>> = Mutex::new(None);
static LAST_BELL: Mutex<Option<Instant>> = Mutex::new(None);

pub(crate) fn play(chime: Chime) {
    if cfg!(test) || !enabled(std::env::var_os(SOUND_ENV).as_deref()) || !due(chime, Instant::now())
    {
        return;
    }
    std::thread::spawn(move || {
        if let Err(error) = play_blocking(chime) {
            log::debug!(target: "zz::agent::sound", "chime playback failed: {error}");
        }
    });
}

fn due(chime: Chime, now: Instant) -> bool {
    let last =
        |slot: &'static Mutex<Option<Instant>>| slot.lock().unwrap_or_else(PoisonError::into_inner);
    match chime {
        Chime::Request | Chime::Failed | Chime::Done => chime_due(&mut last(&LAST_CHIME), now),
        Chime::Disconnected => chime_due(&mut last(&LAST_DISCONNECT), now),
        Chime::Bell => bell_due(&mut last(&LAST_BELL), now),
        Chime::Sent => true,
    }
}

fn enabled(value: Option<&OsStr>) -> bool {
    value.is_none_or(|value| value != "0")
}

fn play_blocking(chime: Chime) -> Result<(), String> {
    let path = materialize(chime)?;
    let mut child = spawn_player(path).ok_or_else(|| "no audio player available".to_owned())?;
    let deadline = std::time::Instant::now() + PLAYER_DEADLINE;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => return Err(format!("player exited with {status}")),
            Ok(None) if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("player timed out".to_owned());
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(25)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error.to_string());
            }
        }
    }
}

fn materialize(chime: Chime) -> Result<&'static Path, String> {
    static FILES: OnceLock<[Result<PathBuf, String>; Chime::ALL.len()]> = OnceLock::new();
    let files = FILES.get_or_init(|| {
        Chime::ALL.map(|chime| {
            let path = std::env::temp_dir().join(format!(
                "zz-agent-{}-{}.wav",
                chime.name(),
                std::process::id()
            ));
            std::fs::write(&path, chime_wav(chime)).map_err(|error| error.to_string())?;
            Ok(path)
        })
    });
    files[chime as usize]
        .as_deref()
        .map_err(|error: &String| error.clone())
}

#[cfg(target_os = "macos")]
fn spawn_player(path: &Path) -> Option<std::process::Child> {
    player_command("afplay", &[], path)
}

#[cfg(target_os = "linux")]
fn spawn_player(path: &Path) -> Option<std::process::Child> {
    [("paplay", &[][..]), ("pw-play", &[]), ("aplay", &["-q"])]
        .into_iter()
        .find_map(|(program, args)| player_command(program, args, path))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn spawn_player(_path: &Path) -> Option<std::process::Child> {
    None
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn player_command(program: &str, args: &[&str], path: &Path) -> Option<std::process::Child> {
    use zz_daemon_client::unmasked::SpawnUnmasked as _;

    std::process::Command::new(program)
        .args(args)
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn_unmasked()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_env_kill_switch_only_answers_to_zero() {
        assert!(enabled(None));
        assert!(enabled(Some(OsStr::new("1"))));
        assert!(enabled(Some(OsStr::new(""))));
        assert!(!enabled(Some(OsStr::new("0"))));
    }
}
