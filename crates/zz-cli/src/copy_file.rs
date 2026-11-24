use std::{
    fs,
    io::{Read as _, Seek as _, SeekFrom, Write as _},
    path::PathBuf,
};

use zz_protocol::{CommandInvocation, file_hex};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Place {
    Here(PathBuf),
    There { host: String, path: String },
}

impl Place {
    fn parse(word: &str, hosts: &[String]) -> Result<Self, String> {
        let here = || Ok(Self::Here(PathBuf::from(word)));
        let Some((host, path)) = word.split_once(':') else {
            return here();
        };
        if host.len() < 2 || host.contains(['/', '\\']) || host.starts_with(['.', '~']) {
            return here();
        }
        if path.is_empty() || !hosts.iter().any(|name| name == host) {
            return Err(format!(
                "{word} names no host in zz/config (known: {}); write ./{word} for a file here",
                hosts.join(", ")
            ));
        }
        Ok(Self::There {
            host: host.to_owned(),
            path: path.to_owned(),
        })
    }

    fn name(&self) -> Option<String> {
        let path = match self {
            Self::Here(path) => path.to_string_lossy().into_owned(),
            Self::There { path, .. } => path.clone(),
        };
        path.rsplit(['/', '\\'])
            .find(|part| !part.is_empty())
            .map(str::to_owned)
    }

    fn named(self, name: Option<&str>) -> Self {
        match (self, name) {
            (Self::Here(path), Some(name)) if path.is_dir() => Self::Here(path.join(name)),
            (Self::There { host, path }, Some(name)) if path.ends_with(['/', '\\']) => {
                Self::There {
                    host,
                    path: format!("{path}{name}"),
                }
            }
            (place, _) => place,
        }
    }

    fn read(&self, offset: u64) -> Result<Vec<u8>, String> {
        match self {
            Self::Here(path) => {
                let failed = |error: std::io::Error| format!("{}: {error}", path.display());
                let mut file = fs::File::open(path).map_err(failed)?;
                if !file.metadata().map_err(failed)?.is_file() {
                    return Err(format!(
                        "{} is not a file; pack a folder into one file first",
                        path.display()
                    ));
                }
                file.seek(SeekFrom::Start(offset)).map_err(failed)?;
                let mut bytes = Vec::new();
                file.take(file_hex::CHUNK_BYTES as u64)
                    .read_to_end(&mut bytes)
                    .map_err(failed)?;
                Ok(bytes)
            }
            Self::There { host, path } => {
                let offset = offset.to_string();
                let hex = on_host(host, "read-file", &["-s", &offset, path])?;
                file_hex::decode(&hex).ok_or_else(|| format!("{host} sent bytes that are not hex"))
            }
        }
    }

    fn write(&self, bytes: &[u8], append: bool) -> Result<(), String> {
        match self {
            Self::Here(path) => fs::OpenOptions::new()
                .create(true)
                .write(true)
                .append(append)
                .truncate(!append)
                .open(path)
                .and_then(|mut file| file.write_all(bytes))
                .map_err(|error| format!("{}: {error}", path.display())),
            Self::There { host, path } => {
                let hex = file_hex::encode(bytes);
                let mut words = vec![path.as_str(), hex.as_str()];
                if append {
                    words.insert(0, "-a");
                }
                on_host(host, "write-file", &words).map(drop)
            }
        }
    }
}

fn on_host(host: &str, verb: &str, words: &[&str]) -> Result<String, String> {
    let mut client = crate::connect_host_command_client(host, verb == "read-file")?;
    let outcome = client
        .execute_streams(CommandInvocation::new(verb, words.iter().copied()))
        .map_err(|error| {
            let message = crate::command_error_message(&error);
            if message.contains("unknown command") {
                format!("{host}'s zz is too old to copy files; update it with zz update {host}")
            } else {
                message
            }
        })?;
    if outcome.exit_code == 0 {
        Ok(outcome.stdout.to_string())
    } else {
        Err(outcome.stderr.trim().to_owned())
    }
}

fn copy(words: &[String], hosts: &[String]) -> Result<String, String> {
    let [source, destination] = words else {
        return Err("usage: copy-file [HOST:]SOURCE [HOST:]DESTINATION".to_owned());
    };
    let source = Place::parse(source, hosts)?;
    let destination = Place::parse(destination, hosts)?.named(source.name().as_deref());
    let mut copied = 0_u64;
    loop {
        let chunk = source.read(copied)?;
        destination.write(&chunk, copied > 0)?;
        copied += chunk.len() as u64;
        if chunk.len() < file_hex::CHUNK_BYTES {
            let place = match &destination {
                Place::Here(path) => path.display().to_string(),
                Place::There { host, path } => format!("{host}:{path}"),
            };
            return Ok(format!("{place}: {copied} bytes"));
        }
    }
}

pub(crate) fn run(words: &[String]) -> Result<String, String> {
    let hosts = zz_daemon_client::fleet_hosts_with_tailnet()
        .map_err(|error| format!("could not read zz/config: {error}"))?
        .0
        .into_iter()
        .map(|host| host.name)
        .collect::<Vec<_>>();
    copy(words, &hosts)
}

#[cfg(test)]
mod tests {
    use super::{Place, copy};

    #[test]
    fn a_file_longer_than_one_chunk_arrives_whole_and_a_folder_takes_its_name() {
        let hosts = ["macbook".to_owned()];
        let there = |path: &str| Place::There {
            host: "macbook".to_owned(),
            path: path.to_owned(),
        };
        assert_eq!(
            Place::parse("macbook:~/a b.png", &hosts),
            Ok(there("~/a b.png"))
        );
        assert_eq!(
            Place::parse("macbook:C:\\Users\\x.txt", &hosts),
            Ok(there("C:\\Users\\x.txt"))
        );
        for here in ["C:\\Users\\x.txt", "./other:x", "/tmp/a:b", "plain.txt"] {
            assert_eq!(Place::parse(here, &hosts), Ok(Place::Here(here.into())));
        }
        assert_eq!(
            Place::parse("other:/tmp/x", &hosts),
            Err(
                "other:/tmp/x names no host in zz/config (known: macbook); write ./other:/tmp/x for a file here"
                    .to_owned()
            )
        );
        assert_eq!(there("/tmp/").named(Some("x.png")), there("/tmp/x.png"));
        assert_eq!(
            there("/tmp/y.png").named(Some("x.png")),
            there("/tmp/y.png")
        );

        let folder = tempfile::tempdir().expect("temporary folder");
        let source = folder.path().join("frame.bin");
        let bytes = (0..700_000_u32)
            .map(|index| (index % 251) as u8)
            .collect::<Vec<_>>();
        std::fs::write(&source, &bytes).expect("source");
        let into = folder.path().join("out");
        std::fs::create_dir(&into).expect("destination folder");
        let words = [source.display().to_string(), into.display().to_string()];
        assert_eq!(
            copy(&words, &hosts),
            Ok(format!(
                "{}: 700000 bytes",
                into.join("frame.bin").display()
            ))
        );
        assert_eq!(std::fs::read(into.join("frame.bin")).expect("copy"), bytes);
        assert_eq!(
            copy(&[into.display().to_string(), "x".to_owned()], &hosts),
            Err(format!(
                "{} is not a file; pack a folder into one file first",
                into.display()
            ))
        );
    }
}
