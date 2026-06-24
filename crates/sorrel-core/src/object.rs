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
        self.0.iter().map(|byte| format!("{byte:02x}")).collect()
    }
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
        formatter.write_str(&self.to_hex())
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
        /// Two-character slice that could not be decoded.
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
    fn object_id_parse_rejects_wrong_length() {
        assert_eq!(
            "abc".parse::<ObjectId>().unwrap_err(),
            ObjectIdParseError::InvalidLength {
                actual: 3,
                expected: OBJECT_ID_HEX_LEN
            }
        );
    }
}
