use super::*;

#[must_use]
pub fn paste_upload_extension_is_valid(extension: &str) -> bool {
    !extension.is_empty()
        && extension.len() <= MAX_PASTE_UPLOAD_EXTENSION_BYTES
        && extension
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PasteUploadPurpose {
    PastePath,
    RecordPastedImage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PastedImageFormat {
    Png,
    Jpeg,
    Gif,
    Webp,
}

impl PastedImageFormat {
    #[must_use]
    pub fn from_extension(extension: &str) -> Option<Self> {
        Some(match extension {
            "png" => Self::Png,
            "jpg" | "jpeg" => Self::Jpeg,
            "gif" => Self::Gif,
            "webp" => Self::Webp,
            _ => return None,
        })
    }
}

fn deserialize_client_file_path<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_text(deserializer, MAX_CLIENT_FILE_PATH_BYTES)
}

fn deserialize_client_file_bytes<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    let bytes: Vec<u8> = serde_bytes::deserialize(deserializer)?;
    if bytes.len() > MAX_CLIENT_FILE_BYTES {
        return Err(D::Error::invalid_length(
            bytes.len(),
            &"a client file payload within the wire byte limit",
        ));
    }
    Ok(bytes)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClientFileOperation {
    Read,
    Write {
        append: bool,
        #[serde(
            serialize_with = "serde_bytes::serialize",
            deserialize_with = "deserialize_client_file_bytes"
        )]
        data: Vec<u8>,
    },
    ReadStdin {
        binary: bool,
    },
    ReadStdinChunk,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientFileRequest {
    pub request_id: u64,
    #[serde(deserialize_with = "deserialize_client_file_path")]
    pub path: String,
    pub operation: ClientFileOperation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientFileResponse {
    pub request_id: u64,
    #[serde(
        serialize_with = "serde_bytes::serialize",
        deserialize_with = "deserialize_client_file_bytes"
    )]
    pub data: Vec<u8>,
    #[serde(deserialize_with = "deserialize_optional_client_file_error")]
    pub error: Option<String>,
}

fn deserialize_optional_client_file_error<'de, D>(
    deserializer: D,
) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let text = Option::<String>::deserialize(deserializer)?;
    match text {
        Some(text) if text.len() > MAX_GUI_TEXT_BYTES => Err(D::Error::invalid_length(
            text.len(),
            &"a client file error within the wire byte limit",
        )),
        text => Ok(text),
    }
}
