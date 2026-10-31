//! Small byte strings stored in JSON as hex (padding leftovers, odd names).

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Hex(pub Vec<u8>);

impl Hex {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Keep `bytes` only if any of them is non-zero (zero padding is implied).
    pub fn nonzero(bytes: &[u8]) -> Hex {
        if bytes.iter().any(|&b| b != 0) {
            // Trailing zeros are implied too.
            let end = bytes.iter().rposition(|&b| b != 0).map_or(0, |p| p + 1);
            Hex(bytes[..end].to_vec())
        } else {
            Hex::default()
        }
    }
}

pub fn encode(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(DIGITS[(b >> 4) as usize] as char);
        s.push(DIGITS[(b & 15) as usize] as char);
    }
    s
}

pub fn decode(s: &str) -> Result<Vec<u8>, String> {
    let digits: Vec<u8> = s.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if !digits.len().is_multiple_of(2) {
        return Err("odd number of hex digits".into());
    }
    let val = |c: u8| -> Result<u8, String> {
        (c as char)
            .to_digit(16)
            .map(|d| d as u8)
            .ok_or_else(|| format!("invalid hex digit {:?}", c as char))
    };
    digits
        .chunks(2)
        .map(|p| Ok(val(p[0])? << 4 | val(p[1])?))
        .collect()
}

impl Serialize for Hex {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for Hex {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        decode(&s).map(Hex).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let b = vec![0u8, 1, 0xab, 0xff];
        assert_eq!(decode(&encode(&b)).unwrap(), b);
        assert!(decode("abc").is_err());
        assert!(decode("zz").is_err());
    }

    #[test]
    fn nonzero_trims() {
        assert!(Hex::nonzero(&[0, 0, 0]).is_empty());
        assert_eq!(Hex::nonzero(&[0, 5, 0, 0]).0, vec![0, 5]);
    }
}
