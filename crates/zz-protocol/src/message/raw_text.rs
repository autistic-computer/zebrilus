use super::*;

#[derive(Clone, Default)]
pub struct RawText {
    text: String,
    bytes: Option<Vec<u8>>,
}

impl RawText {
    #[must_use]
    pub fn from_bytes(bytes: impl Into<Vec<u8>>) -> Self {
        let bytes = bytes.into();
        match String::from_utf8(bytes) {
            Ok(text) => Self { text, bytes: None },
            Err(error) => {
                let bytes = error.into_bytes();
                let text = String::from_utf8_lossy(&bytes).into_owned();
                Self {
                    text,
                    bytes: Some(bytes),
                }
            }
        }
    }

    #[cfg(unix)]
    #[must_use]
    pub fn from_os_str(value: &std::ffi::OsStr) -> Self {
        use std::os::unix::ffi::OsStrExt as _;

        Self::from_bytes(value.as_bytes())
    }

    #[cfg(not(unix))]
    #[must_use]
    pub fn from_os_str(value: &std::ffi::OsStr) -> Self {
        Self::from(value.to_string_lossy().into_owned())
    }

    #[cfg(unix)]
    #[must_use]
    pub fn to_os_string(&self) -> std::ffi::OsString {
        use std::os::unix::ffi::OsStringExt as _;

        std::ffi::OsString::from_vec(self.as_bytes().to_vec())
    }

    #[cfg(not(unix))]
    #[must_use]
    pub fn to_os_string(&self) -> std::ffi::OsString {
        std::ffi::OsString::from(self.text.clone())
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.bytes.as_deref().unwrap_or(self.text.as_bytes())
    }

    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes.unwrap_or_else(|| self.text.into_bytes())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub fn to_utf8(&self) -> Option<&str> {
        self.bytes.is_none().then_some(self.text.as_str())
    }

    #[must_use]
    pub fn is_utf8(&self) -> bool {
        self.bytes.is_none()
    }

    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.as_bytes().len()
    }

    pub fn push_bytes(&mut self, bytes: &[u8]) {
        let mut current = std::mem::take(self).into_bytes();
        current.extend_from_slice(bytes);
        *self = Self::from_bytes(current);
    }

    #[must_use]
    pub fn join(parts: &[Self], separator: &[u8]) -> Self {
        let mut bytes = Vec::new();
        for (index, part) in parts.iter().enumerate() {
            if index != 0 {
                bytes.extend_from_slice(separator);
            }
            bytes.extend_from_slice(part.as_bytes());
        }
        Self::from_bytes(bytes)
    }

    #[must_use]
    pub fn split_once_byte(&self, separator: u8) -> Option<(Self, Self)> {
        let bytes = self.as_bytes();
        let position = bytes.iter().position(|byte| *byte == separator)?;
        Some((
            Self::from_bytes(&bytes[..position]),
            Self::from_bytes(&bytes[position + 1..]),
        ))
    }
}

impl std::ops::Deref for RawText {
    type Target = str;

    fn deref(&self) -> &str {
        &self.text
    }
}

impl AsRef<str> for RawText {
    fn as_ref(&self) -> &str {
        &self.text
    }
}

impl AsRef<[u8]> for RawText {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl PartialEq for RawText {
    fn eq(&self, other: &Self) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

impl Eq for RawText {}

impl PartialOrd for RawText {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RawText {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.text
            .cmp(&other.text)
            .then_with(|| self.as_bytes().cmp(other.as_bytes()))
    }
}

impl std::hash::Hash for RawText {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.text.hash(state);
    }
}

impl PartialEq<str> for RawText {
    fn eq(&self, other: &str) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

impl PartialEq<&str> for RawText {
    fn eq(&self, other: &&str) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

impl PartialEq<String> for RawText {
    fn eq(&self, other: &String) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

impl PartialEq<RawText> for str {
    fn eq(&self, other: &RawText) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

impl PartialEq<RawText> for &str {
    fn eq(&self, other: &RawText) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

impl PartialEq<RawText> for String {
    fn eq(&self, other: &RawText) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

impl From<String> for RawText {
    fn from(value: String) -> Self {
        Self {
            text: value,
            bytes: None,
        }
    }
}

impl From<&String> for RawText {
    fn from(value: &String) -> Self {
        Self::from(value.clone())
    }
}

impl From<&str> for RawText {
    fn from(value: &str) -> Self {
        Self::from(value.to_owned())
    }
}

impl From<&RawText> for RawText {
    fn from(value: &RawText) -> Self {
        value.clone()
    }
}

impl From<Vec<u8>> for RawText {
    fn from(value: Vec<u8>) -> Self {
        Self::from_bytes(value)
    }
}

impl From<&[u8]> for RawText {
    fn from(value: &[u8]) -> Self {
        Self::from_bytes(value)
    }
}

impl From<RawText> for String {
    fn from(value: RawText) -> Self {
        value.text
    }
}

impl fmt::Display for RawText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.text)
    }
}

impl fmt::Debug for RawText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.bytes {
            Some(bytes) => write!(formatter, "{:?}<{} bytes>", self.text, bytes.len()),
            None => fmt::Debug::fmt(&self.text, formatter),
        }
    }
}

impl std::borrow::Borrow<str> for RawText {
    fn borrow(&self) -> &str {
        &self.text
    }
}

impl FromIterator<RawText> for String {
    fn from_iter<I: IntoIterator<Item = RawText>>(iterator: I) -> Self {
        iterator.into_iter().map(String::from).collect()
    }
}

impl Serialize for RawText {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bytes(self.as_bytes())
    }
}

impl<'de> Deserialize<'de> for RawText {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::from_bytes(
            serde_bytes::ByteBuf::deserialize(deserializer)?.into_vec(),
        ))
    }
}
