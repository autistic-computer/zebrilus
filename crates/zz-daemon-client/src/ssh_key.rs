use std::{
    fs, io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use crate::{unmasked::SpawnUnmasked as _, user_data};

const KEY_FILE_NAME: &str = "id_ed25519";

pub fn ssh_key_path() -> io::Result<PathBuf> {
    let data = user_data::platform_data_dir().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "could not resolve the current user's application-data directory",
        )
    })?;
    Ok(data
        .join(zz_protocol::app_identity::DIRECTORY)
        .join("ssh")
        .join(KEY_FILE_NAME))
}

pub fn ssh_public_key() -> io::Result<String> {
    ensure_key(
        &ssh_key_path()?,
        &key_comment(crate::short_device_name().as_deref()),
    )
}

fn key_comment(device: Option<&str>) -> String {
    let name = device
        .unwrap_or_default()
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        })
        .collect::<String>();
    format!("zz@{}", if name.is_empty() { "host" } else { &name })
}

fn ensure_key(path: &Path, comment: &str) -> io::Result<String> {
    let directory = path
        .parent()
        .ok_or_else(|| io::Error::other("the zz key path has no directory"))?;
    fs::create_dir_all(directory)?;
    user_data::restrict_directory_to_current_user(directory)?;
    if !path.exists() {
        generate_key(directory, path, comment)?;
    }
    user_data::restrict_to_current_user(path)?;
    let public = path.with_extension("pub");
    if let Ok(line) = fs::read_to_string(&public)
        && is_zz_key_line(line.trim_end())
    {
        return Ok(line.trim_end().to_owned());
    }
    let line = derive_public_line(path, comment)?;
    fs::write(&public, format!("{line}\n"))?;
    Ok(line)
}

fn generate_key(directory: &Path, path: &Path, comment: &str) -> io::Result<()> {
    let staging = tempfile::Builder::new()
        .prefix(".keygen-")
        .tempdir_in(directory)?;
    let staged = staging.path().join(KEY_FILE_NAME);
    let output = Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-C", comment, "-f"])
        .arg(&staged)
        .stdin(Stdio::null())
        .output_unmasked()
        .map_err(|error| {
            io::Error::new(error.kind(), format!("could not run ssh-keygen: {error}"))
        })?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "ssh-keygen failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    match fs::hard_link(&staged, path) {
        Ok(()) => fs::rename(staged.with_extension("pub"), path.with_extension("pub")),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error),
    }
}

fn derive_public_line(path: &Path, comment: &str) -> io::Result<String> {
    let output = Command::new("ssh-keygen")
        .arg("-y")
        .arg("-f")
        .arg(path)
        .stdin(Stdio::null())
        .output_unmasked()?;
    let derived = String::from_utf8_lossy(&output.stdout);
    let mut fields = derived.split_whitespace();
    let line = match (fields.next(), fields.next()) {
        (Some(kind), Some(key)) => format!("{kind} {key} {comment}"),
        _ => String::new(),
    };
    if output.status.success() && is_zz_key_line(&line) {
        Ok(line)
    } else {
        Err(io::Error::other(format!(
            "{} is not an ed25519 key ssh-keygen can read",
            path.display()
        )))
    }
}

pub(crate) fn is_zz_key_line(line: &str) -> bool {
    let mut fields = line.split(' ');
    let (Some("ssh-ed25519"), Some(key), Some(comment), None) =
        (fields.next(), fields.next(), fields.next(), fields.next())
    else {
        return false;
    };
    let body = key.trim_end_matches('=');
    let name_byte = |byte: u8| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-');
    key.len() - body.len() <= 2
        && !body.is_empty()
        && body
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/'))
        && comment
            .strip_prefix("zz@")
            .is_some_and(|name| !name.is_empty() && name.bytes().all(name_byte))
}

pub(crate) fn key_body(line: &str) -> &str {
    line.rsplit_once(' ').map_or(line, |(body, _)| body)
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt as _;

    use super::*;

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn the_zz_key_is_made_once_private_and_named_for_this_machine() {
        let data = tempfile::tempdir().unwrap();
        let path = data.path().join("zz/ssh/id_ed25519");

        let line = ensure_key(&path, "zz@testbox").unwrap();
        assert!(is_zz_key_line(&line), "{line}");
        assert!(line.ends_with(" zz@testbox"), "{line}");
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(path.parent().unwrap()), 0o700);
        assert_eq!(
            fs::read_to_string(path.with_extension("pub")).unwrap(),
            format!("{line}\n")
        );
        assert_eq!(
            fs::read_dir(path.parent().unwrap()).unwrap().count(),
            2,
            "the staging directory must not outlive the key"
        );

        assert_eq!(ensure_key(&path, "zz@renamed").unwrap(), line);

        fs::remove_file(path.with_extension("pub")).unwrap();
        assert_eq!(ensure_key(&path, "zz@testbox").unwrap(), line);
    }

    #[test]
    fn the_comment_names_the_device_in_characters_a_key_line_allows() {
        assert_eq!(key_comment(Some("macbook")), "zz@macbook");
        assert_eq!(key_comment(Some("Fabrício's box")), "zz@Fabrciosbox");
        assert_eq!(key_comment(None), "zz@host");
        assert_eq!(key_comment(Some("'")), "zz@host");
    }

    #[test]
    fn a_key_line_from_another_machine_must_be_one_bare_zz_ed25519_key() {
        let key =
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIGW3/OPNnD2l/h6LfRGUUDgkTB5X9f1CU1CuD1GC4d5u";
        assert!(is_zz_key_line(&format!("{key} zz@macbook")));
        assert!(is_zz_key_line("ssh-ed25519 AAAA+/9== zz@xps.tail-net_1"));
        for rejected in [
            format!("command=\"sh\" {key} zz@macbook"),
            format!("restrict {key} zz@macbook"),
            format!("{key} zz@macbook\n{key} zz@evil"),
            format!("{key} zz@macbook\n"),
            format!("{key} zz@mac book"),
            format!("{key} zz@"),
            format!("{key} zz-iphone"),
            key.to_owned(),
            format!(" {key} zz@macbook"),
            format!("{key}  zz@macbook"),
            "ssh-rsa AAAAB3NzaC1yc2E zz@macbook".to_owned(),
            "sk-ssh-ed25519@openssh.com AAAA zz@macbook".to_owned(),
            "ssh-ed25519 AAAA=== zz@macbook".to_owned(),
            "ssh-ed25519 AA=A zz@macbook".to_owned(),
            "ssh-ed25519 = zz@macbook".to_owned(),
            "ssh-ed25519 AAAA zz@mac;rm".to_owned(),
        ] {
            assert!(!is_zz_key_line(&rejected), "{rejected:?}");
        }
        assert_eq!(key_body(&format!("{key} zz@macbook")), key);
    }
}
