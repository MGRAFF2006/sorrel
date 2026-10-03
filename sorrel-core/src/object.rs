use std::{fmt, str::FromStr};

/// Number of bytes in a Sorrel object identifier.
pub const OBJECT_ID_BYTES: usize = 32;

/// Number of lowercase hexadecimal characters in a Sorrel object identifier.
pub const OBJECT_ID_HEX_LEN: usize = OBJECT_ID_BYTES * 2;

/// Content-addressed identifier for bytes stored by Sorrel.
///
/// Sorrel object IDs are currently the BLAKE3 digest of the object's bytes.
/// Higher-level typed objects can canonicalize their byte representation before
/// writing to the store.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ObjectId([u8; OBJECT_ID_BYTES]);

impl ObjectId {
    /// Returns the object ID for `bytes`.
    #[must_use]
    pub fn for_bytes(bytes: &[u8]) -> Self {
        Self(*blake3::hash(bytes).as_bytes())
    }

    /// Builds an object ID from raw digest bytes.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; OBJECT_ID_BYTES]) -> Self {
        Self(bytes)
    }

    /// Returns the raw digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; OBJECT_ID_BYTES] {
        &self.0
    }

    /// Returns the lowercase hexadecimal representation of this object ID.
    #[must_use]
    pub fn to_hex(self) -> String {
        blake3::Hash::from_bytes(self.0).to_hex().to_string()
    }
}

/// Parses a lowercase hexadecimal object ID string.
///
/// This is the preferred entry point at HTTP and other text boundaries where
/// callers hold a `&str` rather than using [`FromStr`] on [`ObjectId`].
pub fn parse_object_id_hex(hex: &str) -> Result<ObjectId, ObjectIdParseError> {
    hex.parse()
}

impl fmt::Debug for ObjectId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ObjectId")
            .field(&self.to_hex())
            .finish()
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(blake3::Hash::from_bytes(self.0).to_hex().as_str())
    }
}

impl FromStr for ObjectId {
    type Err = ObjectIdParseError;

    fn from_str(hex: &str) -> Result<Self, Self::Err> {
        if hex.len() != OBJECT_ID_HEX_LEN {
            return Err(ObjectIdParseError::InvalidLength {
                actual: hex.len(),
                expected: OBJECT_ID_HEX_LEN,
            });
        }

        // Restrict input to hexadecimal ASCII before UTF-8 pair slicing and
        // integer parsing, which would otherwise accept a leading '+' sign.
        if let Some((index, character)) = hex.char_indices().find(|(_, c)| !c.is_ascii_hexdigit()) {
            return Err(ObjectIdParseError::InvalidHex {
                index,
                value: character.to_string(),
            });
        }

        let mut bytes = [0; OBJECT_ID_BYTES];
        for (index, byte) in bytes.iter_mut().enumerate() {
            let offset = index * 2;
            *byte = u8::from_str_radix(&hex[offset..offset + 2], 16).map_err(|_| {
                ObjectIdParseError::InvalidHex {
                    index: offset,
                    value: hex[offset..offset + 2].to_owned(),
                }
            })?;
        }

        Ok(Self(bytes))
    }
}

/// Error returned when parsing an [`ObjectId`] from text fails.
#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub enum ObjectIdParseError {
    /// The provided hex string had the wrong length.
    #[error("object id has invalid length {actual}; expected {expected}")]
    InvalidLength {
        /// Actual input length in bytes.
        actual: usize,
        /// Expected lowercase hexadecimal length.
        expected: usize,
    },

    /// The provided hex string contains non-hexadecimal characters.
    #[error("object id contains invalid hex byte {value:?} at index {index}")]
    InvalidHex {
        /// Byte index where invalid hex started.
        index: usize,
        /// Invalid hexadecimal pair or non-ASCII character.
        value: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_ids_are_stable_blake3_hashes() {
        let id = ObjectId::for_bytes(b"sorrel");

        assert_eq!(id.to_string(), blake3::hash(b"sorrel").to_hex().to_string());
    }

    #[test]
    fn object_ids_round_trip_through_hex() {
        let id = ObjectId::for_bytes(b"round trip");
        let parsed: ObjectId = id.to_string().parse().unwrap();

        assert_eq!(parsed, id);
    }

    #[test]
    fn parse_object_id_hex_round_trips() {
        let id = ObjectId::for_bytes(b"round trip");
        let parsed = parse_object_id_hex(&id.to_hex()).unwrap();
        assert_eq!(parsed, id);
    }

    #[test]
    fn object_id_parse_rejects_wrong_length() {
        assert_eq!(
            "abc".parse::<ObjectId>().unwrap_err(),
            ObjectIdParseError::InvalidLength {
                actual: 3,
                expected: OBJECT_ID_HEX_LEN
            }
        );
    }

    #[test]
    fn object_id_parse_rejects_unicode_at_utf8_boundaries_without_panicking() {
        for prefix_len in [0, 1, 31, 61] {
            let hex = format!("{}€{}", "0".repeat(prefix_len), "0".repeat(61 - prefix_len));
            assert_eq!(hex.len(), OBJECT_ID_HEX_LEN);
            assert_eq!(
                hex.parse::<ObjectId>().unwrap_err(),
                ObjectIdParseError::InvalidHex {
                    index: prefix_len,
                    value: "€".into(),
                }
            );
        }
    }

    #[test]
    fn object_id_hex_encoder_preserves_all_bytes_and_uppercase_parsing() {
        for start in (0..=224).step_by(32) {
            let bytes = std::array::from_fn(|index| (start + index) as u8);
            let id = ObjectId::from_bytes(bytes);
            let expected: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
            assert_eq!(id.to_hex(), expected);
            assert_eq!(id.to_string(), expected);
            assert_eq!(expected.to_uppercase().parse::<ObjectId>().unwrap(), id);
        }
        assert_eq!(
            format!("{}xy", "0".repeat(62))
                .parse::<ObjectId>()
                .unwrap_err(),
            ObjectIdParseError::InvalidHex {
                index: 62,
                value: "x".into()
            }
        );
    }

    #[test]
    fn object_id_parse_rejects_signs_and_whitespace() {
        for invalid in ["+0", "-0", " 0", "0 ", "\t0", "0\n"] {
            let hex = format!("{}{invalid}", "0".repeat(62));
            assert!(matches!(
                hex.parse::<ObjectId>(),
                Err(ObjectIdParseError::InvalidHex { .. })
            ));
        }
    }
}
