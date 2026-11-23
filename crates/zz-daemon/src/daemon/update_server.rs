use semver::Version;
use zz_config::{
    owned::{self, InstallError},
    update::Progress,
};

use super::*;

const USAGE: &str = "usage: update-server --version version | --remove";

enum Request {
    Install(Version),
    Remove,
}

struct Resolve(Option<Arc<terminal_requests::CommandState>>);

impl Resolve {
    fn done(mut self, result: Result<String, String>) {
        if let Some(state) = self.0.take() {
            state.resolve(
                result
                    .map(|output| Execution {
                        output: output.into(),
                        effects: Vec::new(),
                    })
                    .map_err(|error| ServerError::InvalidCommand(error).into()),
            );
        }
    }
}

impl Drop for Resolve {
    fn drop(&mut self) {
        if let Some(state) = self.0.take() {
            state.resolve(Err(ServerError::InvalidCommand(
                "update-server stopped before it finished".to_owned(),
            )
            .into()));
        }
    }
}

impl Shared {
    pub(super) fn update_server(
        self: &Arc<Self>,
        client: ClientId,
        args: &[RawText],
    ) -> Result<Execution, DaemonError> {
        let request = match args {
            [flag, version] if flag == "--version" => Request::Install(
                Version::parse(version.as_str().trim_start_matches('v')).map_err(|_| {
                    ServerError::CommandParse(format!("not a zz version: {version}"))
                })?,
            ),
            [flag] if flag == "--remove" => Request::Remove,
            _ => return Err(ServerError::CommandParse(USAGE.to_owned()).into()),
        };
        let wait = terminal_requests::CommandWait::new(self);
        let resolve = Resolve(Some(wait.start()));
        let shared = Arc::clone(self);
        let job = thread::Builder::new()
            .name("zz-update-server".to_owned())
            .spawn(move || {
                let result = match request {
                    Request::Install(version) => install(&shared, client, &version),
                    Request::Remove => remove(),
                };
                resolve.done(result);
            });
        if let Err(error) = job {
            log::warn!("could not start update-server: {error}");
        }
        wait.finish(self, Execution::default())
    }
}

fn install(shared: &Shared, client: ClientId, version: &Version) -> Result<String, String> {
    let mut downloading = false;
    let result = owned::install(version, |progress| {
        let line = match progress {
            Progress::Downloading { .. } if !downloading => {
                downloading = true;
                format!("downloading zz {version}\n")
            }
            Progress::Verifying => "checking the download\n".to_owned(),
            Progress::Staging => format!("unpacking zz {version}\n"),
            Progress::Downloading { .. } | Progress::Running { .. } => return,
        };
        shared.release_command_stdout(client, &mut RawText::from(line));
    });
    match result {
        Ok(copy) => Ok(format!("zz {} installed\n", copy.info.version)),
        Err(InstallError::Current(current)) => Ok(format!("already zz {current}\n")),
        Err(error) => Err(error.to_string()),
    }
}

fn remove() -> Result<String, String> {
    match owned::remove() {
        Ok(Some(info)) => Ok(format!("removed zz {}\n", info.version)),
        Ok(None) => Ok("no owned copy to remove\n".to_owned()),
        Err(error) => Err(error.to_string()),
    }
}
