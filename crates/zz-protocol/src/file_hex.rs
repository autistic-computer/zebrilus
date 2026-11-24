pub const CHUNK_BYTES: usize = 256 * 1024;

#[must_use]
pub fn encode(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        hex.push(char::from(DIGITS[usize::from(byte >> 4)]));
        hex.push(char::from(DIGITS[usize::from(byte & 15)]));
    }
    hex
}

#[must_use]
pub fn decode(hex: &str) -> Option<Vec<u8>> {
    let digit = |byte: u8| {
        char::from(byte)
            .to_digit(16)
            .and_then(|digit| u8::try_from(digit).ok())
    };
    let (pairs, rest) = hex.trim().as_bytes().as_chunks::<2>();
    if !rest.is_empty() {
        return None;
    }
    pairs
        .iter()
        .map(|[high, low]| Some(digit(*high)? << 4 | digit(*low)?))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{decode, encode};

    #[test]
    fn bytes_survive_the_trip_through_hex() {
        let bytes = (0..=255).collect::<Vec<u8>>();
        assert_eq!(encode(&[0, 15, 16, 255]), "000f10ff");
        assert_eq!(decode(&encode(&bytes)), Some(bytes));
        assert_eq!(decode(" 000F10ff\n"), Some(vec![0, 15, 16, 255]));
        assert_eq!(decode(""), Some(Vec::new()));
        assert_eq!(decode("0"), None);
        assert_eq!(decode("zz"), None);
    }
}
