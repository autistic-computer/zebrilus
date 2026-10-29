use std::{
    collections::{HashMap, HashSet},
    fmt::Write as _,
    sync::{Arc, LazyLock, Mutex, OnceLock},
};

use async_channel::{Receiver, Sender};

pub(crate) const DEFAULT_SCHEME: &str = "app";
pub(crate) const DEFAULT_PAGE_GLOBAL: &str = "app";
pub const MAX_ARTIFACT_PATH_BYTES: usize = 4096;
pub const MAX_ARTIFACT_REPLY_BYTES: usize = 64 * 1024;
pub const ARTIFACT_PROFILE: &str = "artifacts";
const TOKEN_LEN: usize = 32;

pub struct ArtifactFile {
    pub mime: String,
    pub bytes: Vec<u8>,
    pub reply: bool,
}

type Responder = Box<dyn FnOnce(Result<ArtifactFile, String>) + Send>;

pub struct ArtifactRequest {
    pub token: String,
    pub path: String,
    responder: Responder,
}

impl ArtifactRequest {
    pub fn respond(self, result: Result<ArtifactFile, String>) {
        if let Ok(file) = &result {
            set_reply_allowed(&self.token, file.reply);
        }
        (self.responder)(result);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactReply {
    pub token: String,
    pub text: String,
}

static REQUESTS: LazyLock<(Sender<ArtifactRequest>, Receiver<ArtifactRequest>)> =
    LazyLock::new(async_channel::unbounded);
static REPLIES: LazyLock<(Sender<ArtifactReply>, Receiver<ArtifactReply>)> =
    LazyLock::new(async_channel::unbounded);
static REPLY_TOKENS: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Mutex::default);
static LOCAL_PAGES: LazyLock<Mutex<HashMap<String, (Arc<[u8]>, bool)>>> =
    LazyLock::new(Mutex::default);
static NAMES: OnceLock<(&'static str, &'static str)> = OnceLock::new();

#[cfg_attr(not(feature = "cef-runtime"), allow(dead_code))]
pub(crate) fn set_names(scheme: &'static str, page_global: &'static str) {
    let _ = NAMES.set((scheme, page_global));
}

fn names() -> (&'static str, &'static str) {
    NAMES
        .get()
        .copied()
        .unwrap_or((DEFAULT_SCHEME, DEFAULT_PAGE_GLOBAL))
}

#[must_use]
pub fn local_scheme() -> &'static str {
    names().0
}

#[cfg_attr(not(feature = "cef-runtime"), allow(dead_code))]
pub(crate) fn page_global() -> &'static str {
    names().1
}

pub fn host_local_page(html: &str, reply: bool) -> Result<String, String> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| error.to_string())?;
    let token = bytes
        .iter()
        .fold(String::with_capacity(32), |mut token, byte| {
            let _ = write!(token, "{byte:02x}");
            token
        });
    LOCAL_PAGES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(token.clone(), (Arc::from(html.as_bytes()), reply));
    Ok(format!("{}://{token}/", local_scheme()))
}

pub fn drop_local_page(url: &str) {
    if let Some((token, _)) = parse_artifact_url(url) {
        LOCAL_PAGES
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&token);
        set_reply_allowed(&token, false);
    }
}

#[must_use]
pub fn is_local_page(token: &str) -> bool {
    LOCAL_PAGES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .contains_key(token)
}

fn local_page(token: &str, path: &str) -> Option<Result<ArtifactFile, String>> {
    let (bytes, reply) = LOCAL_PAGES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(token)
        .cloned()?;
    Some(if path.is_empty() || path == "index.html" {
        Ok(ArtifactFile {
            mime: "text/html".to_owned(),
            bytes: bytes.to_vec(),
            reply,
        })
    } else {
        Err(format!("{path} is not part of this preview"))
    })
}

#[must_use]
pub fn artifact_requests() -> Receiver<ArtifactRequest> {
    REQUESTS.1.clone()
}

#[must_use]
pub fn artifact_replies() -> Receiver<ArtifactReply> {
    REPLIES.1.clone()
}

#[cfg_attr(not(feature = "cef-runtime"), allow(dead_code))]
pub(crate) fn request_artifact(
    token: String,
    path: String,
    respond: impl FnOnce(Result<ArtifactFile, String>) + Send + 'static,
) {
    if let Some(result) = local_page(&token, &path) {
        if let Ok(file) = &result {
            set_reply_allowed(&token, file.reply);
        }
        respond(result);
        return;
    }
    let request = ArtifactRequest {
        token,
        path,
        responder: Box::new(respond),
    };
    if let Err(error) = REQUESTS.0.try_send(request) {
        error
            .into_inner()
            .respond(Err("the browser is shutting down".to_owned()));
    }
}

#[cfg_attr(not(feature = "cef-runtime"), allow(dead_code))]
pub(crate) fn send_reply(token: &str, text: String) -> Result<(), String> {
    if !reply_allowed(token) {
        return Err("this page was opened without replies".to_owned());
    }
    if text.len() > MAX_ARTIFACT_REPLY_BYTES {
        return Err(format!(
            "a reply may be at most {MAX_ARTIFACT_REPLY_BYTES} bytes"
        ));
    }
    REPLIES
        .0
        .try_send(ArtifactReply {
            token: token.to_owned(),
            text,
        })
        .map_err(|_| "the browser is shutting down".to_owned())
}

fn set_reply_allowed(token: &str, allowed: bool) {
    let mut tokens = REPLY_TOKENS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if allowed {
        tokens.insert(token.to_owned());
    } else {
        tokens.remove(token);
    }
}

fn reply_allowed(token: &str) -> bool {
    REPLY_TOKENS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .contains(token)
}

#[must_use]
pub fn parse_artifact_url(url: &str) -> Option<(String, String)> {
    parse_url(local_scheme(), url)
}

fn parse_url(scheme: &str, url: &str) -> Option<(String, String)> {
    let rest = url.strip_prefix(scheme)?.strip_prefix("://")?;
    let rest = rest.split(['?', '#']).next().unwrap_or_default();
    let (token, path) = rest.split_once('/').unwrap_or((rest, ""));
    if token.len() != TOKEN_LEN || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let path = percent_decode(path)?;
    if path.len() > MAX_ARTIFACT_PATH_BYTES {
        return None;
    }
    Some((token.to_ascii_lowercase(), path))
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while let Some(&byte) = bytes.get(index) {
        if byte == b'%' {
            let hex = text.get(index + 1..index + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            out.push(byte);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg_attr(not(feature = "cef-runtime"), allow(dead_code))]
pub(crate) fn error_page(reason: &str) -> Vec<u8> {
    let reason = reason
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!(
        "<!doctype html><meta charset=utf-8><title>Page unavailable</title>\
         <body style=\"font:14px system-ui;margin:2em;color:#888\"><p>{reason}</p></body>"
    )
    .into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn artifact_urls_give_their_token_and_decoded_path() {
        assert_eq!(
            parse_url(
                "zz",
                &format!(
                    "zz://{}/my%20report.html?x=1#top",
                    TOKEN.to_ascii_uppercase()
                )
            ),
            Some((TOKEN.to_owned(), "my report.html".to_owned()))
        );
        assert_eq!(
            parse_url("zz", &format!("zz://{TOKEN}")),
            Some((TOKEN.to_owned(), String::new()))
        );
        assert_eq!(
            parse_url("zz", &format!("zz://{TOKEN}/lib/a.js")),
            Some((TOKEN.to_owned(), "lib/a.js".to_owned()))
        );
        assert_eq!(parse_url("zz", &format!("https://{TOKEN}/")), None);
        assert_eq!(parse_url("app", &format!("zz://{TOKEN}/")), None);
        assert_eq!(parse_url("z", &format!("zz://{TOKEN}/")), None);
        assert_eq!(parse_url("zz", "zz:///x"), None);
        assert_eq!(parse_url("zz", "zz://a.b/x"), None);
        assert_eq!(parse_url("zz", &format!("zz://{TOKEN}/%zz")), None);
    }

    #[test]
    fn urls_the_wire_would_refuse_never_parse() {
        assert_eq!(parse_url("zz", "zz://ab12/x"), None);
        assert_eq!(parse_url("zz", &format!("zz://{TOKEN}0/x")), None);
        assert_eq!(parse_url("zz", &format!("zz://{}/x", "g".repeat(32))), None);
        let longest = "x".repeat(MAX_ARTIFACT_PATH_BYTES);
        assert!(parse_url("zz", &format!("zz://{TOKEN}/{longest}")).is_some());
        assert_eq!(parse_url("zz", &format!("zz://{TOKEN}/{longest}x")), None);
    }

    #[test]
    fn replies_over_the_wire_limit_are_refused() {
        let url = host_local_page("<p>live</p>", true).unwrap();
        let (token, _) = parse_artifact_url(&url).unwrap();
        request_artifact(token.clone(), String::new(), |_| {});
        assert!(send_reply(&token, "x".repeat(MAX_ARTIFACT_REPLY_BYTES)).is_ok());
        assert!(send_reply(&token, "x".repeat(MAX_ARTIFACT_REPLY_BYTES + 1)).is_err());
        drop_local_page(&url);
    }

    #[test]
    fn local_pages_answer_without_the_daemon_until_dropped() {
        let url = host_local_page("<p>preview</p>", false).unwrap();
        let (token, _) = parse_artifact_url(&url).unwrap();
        assert!(is_local_page(&token));
        let page = local_page(&token, "").unwrap().unwrap();
        assert_eq!(page.bytes, b"<p>preview</p>");
        assert!(!page.reply);
        assert!(local_page(&token, "other.js").unwrap().is_err());
        drop_local_page(&url);
        assert!(local_page(&token, "").is_none());
        assert!(!is_local_page(&token));
    }

    #[test]
    fn a_live_page_may_reply_until_dropped() {
        let url = host_local_page("<p>live</p>", true).unwrap();
        let (token, _) = parse_artifact_url(&url).unwrap();
        assert!(send_reply(&token, "early".to_owned()).is_err());
        request_artifact(token.clone(), String::new(), |result| {
            assert!(result.unwrap().reply);
        });
        assert!(send_reply(&token, "hi".to_owned()).is_ok());
        drop_local_page(&url);
        assert!(send_reply(&token, "late".to_owned()).is_err());
    }

    #[test]
    fn error_pages_escape_the_reason() {
        let page = String::from_utf8(error_page("<script>")).unwrap();
        assert!(page.contains("&lt;script&gt;"));
    }
}
