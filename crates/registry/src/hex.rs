//! Minimal hex encode/decode. The registry crate's dependency set is frozen
//! to operant-ir, ed25519-dalek, base64, blake3, so this does not pull in a
//! `hex` crate for what is a dozen lines of code.

use std::fmt::Write as _;

use crate::error::RegistryError;

pub fn encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

pub fn decode(s: &str) -> Result<Vec<u8>, RegistryError> {
    let s = s.trim();
    if !s.len().is_multiple_of(2) {
        return Err(RegistryError::InvalidHex(s.to_string()));
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for chunk in bytes.chunks(2) {
        let hi = hex_val(chunk[0]).ok_or_else(|| RegistryError::InvalidHex(s.to_string()))?;
        let lo = hex_val(chunk[1]).ok_or_else(|| RegistryError::InvalidHex(s.to_string()))?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

fn hex_val(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let bytes = [0u8, 1, 2, 254, 255];
        let hex = encode(&bytes);
        assert_eq!(hex, "000102feff");
        assert_eq!(decode(&hex).unwrap(), bytes);
    }

    #[test]
    fn rejects_odd_length_and_bad_chars() {
        assert!(decode("abc").is_err());
        assert!(decode("zz").is_err());
    }

    #[test]
    fn trims_trailing_newline() {
        assert_eq!(decode("00ff\n").unwrap(), vec![0u8, 255]);
    }
}
