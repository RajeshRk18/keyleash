use serde::{Deserialize, Serialize, de};
use std::fmt;
use thiserror::Error;

#[derive(Debug, Clone, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub struct SecretName(String);

#[derive(Debug, Eq, PartialEq, Error)]
pub enum SecretNameError {
    #[error("secret name is empty")]
    Empty,

    #[error("secret name is {len} bytes, maximum is 128 bytes")]
    TooLong { len: usize },

    #[error("invalid first byte {byte:?}")]
    InvalidFirstByte { byte: u8 },

    #[error("invalid byte {byte:?} at index {index}")]
    InvalidByte { byte: u8, index: usize },
}

fn validate(secret_name: &str) -> Result<(), SecretNameError> {
    let sec_bytes = secret_name.as_bytes();
    if sec_bytes.is_empty() {
        return Err(SecretNameError::Empty);
    }

    if sec_bytes.len() > 128 {
        return Err(SecretNameError::TooLong {
            len: secret_name.len(),
        });
    }

    if !sec_bytes[0].is_ascii_uppercase() {
        return Err(SecretNameError::InvalidFirstByte { byte: sec_bytes[0] });
    }

    for (i, &byte) in sec_bytes.iter().enumerate().skip(1) {
        if !(byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_') {
            return Err(SecretNameError::InvalidByte { byte, index: i });
        }
    }

    Ok(())
}

impl Serialize for SecretName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for SecretName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let string = String::deserialize(deserializer)?;
        let secret_name = SecretName::try_from(string).map_err(de::Error::custom)?;

        Ok(secret_name)
    }
}

impl TryFrom<&str> for SecretName {
    type Error = SecretNameError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        validate(value)?;

        Ok(SecretName(value.to_owned()))
    }
}

impl TryFrom<String> for SecretName {
    type Error = SecretNameError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        validate(&value)?;

        Ok(SecretName(value))
    }
}

impl AsRef<str> for SecretName {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for SecretName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::{SecretName, SecretNameError};

    #[test]
    fn accepts_canonical_names() {
        for value in ["X", "DATABASE_URL", "AWS_ACCESS_KEY_ID"] {
            let name = SecretName::try_from(value).unwrap();
            assert_eq!(name.as_ref(), value);
        }
    }

    #[test]
    fn rejects_empty_name() {
        assert_eq!(SecretName::try_from(""), Err(SecretNameError::Empty));
    }

    #[test]
    fn rejects_invalid_first_byte() {
        assert_eq!(
            SecretName::try_from("1PASSWORD_TOKEN"),
            Err(SecretNameError::InvalidFirstByte { byte: b'1' })
        );
    }

    #[test]
    fn rejects_invalid_later_byte() {
        assert_eq!(
            SecretName::try_from("DATABASE-URL"),
            Err(SecretNameError::InvalidByte {
                index: 8,
                byte: b'-',
            })
        );
    }

    #[test]
    fn rejects_name_longer_than_128_bytes() {
        let value = "A".repeat(129);
        assert_eq!(
            SecretName::try_from(value.as_str()),
            Err(SecretNameError::TooLong { len: 129 })
        );
    }

    #[test]
    fn serde_round_trip_preserves_validation() {
        let name = SecretName::try_from("DATABASE_URL").unwrap();
        let json = serde_json::to_string(&name).unwrap();
        assert_eq!(json, r#""DATABASE_URL""#);
        assert_eq!(serde_json::from_str::<SecretName>(&json).unwrap(), name);
        assert!(serde_json::from_str::<SecretName>(r#""database_url""#).is_err());
    }

    #[test]
    fn display_returns_the_canonical_name() {
        let name = SecretName::try_from("DATABASE_URL").unwrap();
        assert_eq!(name.to_string(), "DATABASE_URL");
    }
}
