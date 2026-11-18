use std::{
    fs,
    io::{self, Write as _},
    path::Path,
};

use ed25519_dalek::{Signature, Signer as _, SigningKey, VerifyingKey};

const CONTEXT: &[u8] = b"zz daemon identity v1";
const SEED_BYTES: usize = 32;

pub type Nonce = [u8; 32];
pub type Key = [u8; 32];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proof {
    pub key: Key,
    pub signature: Vec<u8>,
}

fn signed(nonce: &Nonce) -> Vec<u8> {
    [CONTEXT, nonce.as_slice()].concat()
}

#[must_use]
pub fn prove(seed: &[u8; SEED_BYTES], nonce: &Nonce) -> Proof {
    let signing = SigningKey::from_bytes(seed);
    Proof {
        key: signing.verifying_key().to_bytes(),
        signature: signing.sign(&signed(nonce)).to_bytes().to_vec(),
    }
}

#[must_use]
pub fn proves(key: &Key, nonce: &Nonce, signature: &[u8]) -> bool {
    let (Ok(key), Ok(signature)) = (
        VerifyingKey::from_bytes(key),
        Signature::from_slice(signature),
    ) else {
        return false;
    };
    key.verify_strict(&signed(nonce), &signature).is_ok()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    #[error("it no longer proves which zz it is; this may not be your machine")]
    Silent,
    #[error("it could not prove which zz it is; this may not be your machine")]
    Forged,
    #[error(
        "it is not the zz saved under this name; remove the machine and add it again if zz was reinstalled there"
    )]
    Changed,
}

pub fn judge(
    pinned: Option<&str>,
    nonce: &Nonce,
    proof: Option<(&Key, &[u8])>,
) -> Result<Option<String>, Refusal> {
    let Some((key, signature)) = proof else {
        return pinned.map_or(Ok(None), |_| Err(Refusal::Silent));
    };
    if !proves(key, nonce, signature) {
        return Err(Refusal::Forged);
    }
    let shown = key_text(key);
    match pinned {
        None => Ok(Some(shown)),
        Some(saved) if saved == shown => Ok(None),
        Some(_) => Err(Refusal::Changed),
    }
}

pub fn nonce() -> io::Result<Nonce> {
    let mut nonce = [0_u8; 32];
    getrandom::fill(&mut nonce).map_err(io::Error::other)?;
    Ok(nonce)
}

#[must_use]
pub fn key_text(key: &Key) -> String {
    use std::fmt::Write as _;

    key.iter().fold(String::new(), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}

pub fn seed(path: &Path) -> io::Result<[u8; SEED_BYTES]> {
    match read_seed(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        result => return result,
    }
    let directory = path
        .parent()
        .ok_or_else(|| io::Error::other("the identity file has no directory"))?;
    fs::create_dir_all(directory)?;
    crate::user_data::restrict_directory_to_current_user(directory)?;
    let mut fresh = [0_u8; SEED_BYTES];
    getrandom::fill(&mut fresh).map_err(io::Error::other)?;
    let staged = path.with_extension(format!("new-{}", std::process::id()));
    let _ = fs::remove_file(&staged);
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(&staged)?;
    file.write_all(&fresh)?;
    file.sync_all()?;
    drop(file);
    let linked = fs::hard_link(&staged, path);
    let _ = fs::remove_file(&staged);
    match linked {
        Err(error) if error.kind() != io::ErrorKind::AlreadyExists => Err(error),
        _ => read_seed(path),
    }
}

fn read_seed(path: &Path) -> io::Result<[u8; SEED_BYTES]> {
    fs::read(path)?.try_into().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} does not hold a 32-byte key", path.display()),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_proof_holds_only_for_its_key_nonce_and_signature() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("tailnet").join("identity");
        let first = seed(&path).unwrap();
        assert_eq!(seed(&path).unwrap(), first);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }

        let nonce = [7_u8; 32];
        let proof = prove(&first, &nonce);
        assert_eq!(proof.signature.len(), 64);
        assert!(proves(&proof.key, &nonce, &proof.signature));
        assert!(!proves(&proof.key, &[8_u8; 32], &proof.signature));
        assert!(!proves(
            &prove(&[1_u8; 32], &nonce).key,
            &nonce,
            &proof.signature
        ));
        assert!(!proves(&proof.key, &nonce, &proof.signature[..63]));
        let mut forged = proof.signature.clone();
        forged[0] ^= 1;
        assert!(!proves(&proof.key, &nonce, &forged));
        let zero = "3b6a27bcceb6a42d62a3a8d02a6f0d73653215771de243a63ac048a18b59da29";
        let known = prove(&[0_u8; 32], &nonce);
        assert_eq!(key_text(&known.key), zero);

        let shown = Some((&known.key, known.signature.as_slice()));
        assert_eq!(judge(None, &nonce, None), Ok(None));
        assert_eq!(judge(None, &nonce, shown), Ok(Some(zero.to_owned())));
        assert_eq!(judge(Some(zero), &nonce, shown), Ok(None));
        assert_eq!(judge(Some(zero), &nonce, None), Err(Refusal::Silent));
        assert_eq!(
            judge(Some(&key_text(&proof.key)), &nonce, shown),
            Err(Refusal::Changed)
        );
        assert_eq!(
            judge(Some(zero), &nonce, Some((&known.key, forged.as_slice()))),
            Err(Refusal::Forged)
        );
        assert_eq!(judge(None, &[9_u8; 32], shown), Err(Refusal::Forged));
    }
}
