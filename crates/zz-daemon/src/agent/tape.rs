use std::{
    ffi::OsStr,
    fs::{self, File},
    io::Write as _,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use parking_lot::Mutex;
use serde_json::{Value, json};
use zz_protocol::AgentProvider;

const TAPE_DIRECTORY_ENVIRONMENT_VARIABLE: &str = "ZZ_AGENT_TAPE_DIR";

static STARTED: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy)]
pub(crate) enum Direction {
    In,
    Out,
}

pub(crate) struct Tape {
    started: Instant,
    file: Mutex<Option<(PathBuf, File)>>,
}

impl Tape {
    pub(crate) fn start<'a>(
        provider: AgentProvider,
        program: &OsStr,
        args: impl IntoIterator<Item = &'a OsStr>,
        cwd: Option<&Path>,
    ) -> Option<Arc<Self>> {
        let directory = PathBuf::from(std::env::var_os(TAPE_DIRECTORY_ENVIRONMENT_VARIABLE)?);
        let path = directory.join(format!(
            "{}-{}-{}-{}.ndjson",
            provider.as_str(),
            chrono::Local::now().format("%Y%m%dT%H%M%S"),
            std::process::id(),
            STARTED.fetch_add(1, Ordering::Relaxed) + 1,
        ));
        let file = match fs::create_dir_all(&directory).and_then(|()| File::create(&path)) {
            Ok(file) => file,
            Err(error) => {
                log::warn!(target: "zz::agent", "could not start the agent tape {}: {error}", path.display());
                return None;
            }
        };
        let cwd = cwd
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok());
        let tape = Arc::new(Self {
            started: Instant::now(),
            file: Mutex::new(Some((path, file))),
        });
        tape.write(
            "0",
            "meta",
            &json!({
                "provider": provider.as_str(),
                "program": program.to_string_lossy(),
                "args": args.into_iter().map(OsStr::to_string_lossy).collect::<Vec<_>>(),
                "cwd": cwd.map(|cwd| cwd.to_string_lossy().into_owned()),
                "started": SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |since| u64::try_from(since.as_millis()).unwrap_or(u64::MAX)),
            }),
        );
        Some(tape)
    }

    pub(crate) fn line(&self, direction: Direction, line: &str) {
        let line = line.trim_end_matches(['\r', '\n']);
        let message =
            serde_json::from_str::<Value>(line).unwrap_or_else(|_| Value::String(line.to_owned()));
        let direction = match direction {
            Direction::In => "in",
            Direction::Out => "out",
        };
        self.write(&self.seconds(), direction, &message);
    }

    pub(crate) fn exit(&self, message: &Value) {
        self.write(&self.seconds(), "exit", message);
        *self.file.lock() = None;
    }

    fn seconds(&self) -> String {
        format!("{:.3}", self.started.elapsed().as_secs_f64())
    }

    fn write(&self, seconds: &str, direction: &str, message: &Value) {
        let mut file = self.file.lock();
        let Some((path, handle)) = file.as_mut() else {
            return;
        };
        let record = format!("{{\"t\":{seconds},\"dir\":\"{direction}\",\"msg\":{message}}}\n");
        if let Err(error) = handle
            .write_all(record.as_bytes())
            .and_then(|()| handle.flush())
        {
            log::warn!(target: "zz::agent", "stopped the agent tape {}: {error}", path.display());
            *file = None;
        }
    }
}

impl Drop for Tape {
    fn drop(&mut self) {
        if self.file.get_mut().is_some() {
            self.write(&self.seconds(), "exit", &Value::Null);
        }
    }
}
