use std::str::FromStr;
use std::{fmt, path::Path};

use rand::distr::{Alphanumeric, SampleString};
use serde::de::Error;
use serde::{Deserialize, Deserializer};

const MAX_LENGTH: usize = 36;

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Name(String);

impl Name {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();

        if value.is_empty() {
            return Err(String::from("name must not be empty"));
        }
        if value.len() > MAX_LENGTH {
            return Err(format!(
                "name must not be longer than {MAX_LENGTH} characters"
            ));
        }
        if value.starts_with('-') {
            return Err(String::from("name must not start with `-`"));
        }
        if let Some(character) = value.chars().find(|c| !Self::is_allowed(*c)) {
            return Err(format!(
                "name contains the invalid character `{character}`, only `A-Z`, `a-z`, `0-9`, `-` and `_` are allowed"
            ));
        }

        Ok(Self(value))
    }

    pub fn sanitize(value: &str) -> Option<Self> {
        let sanitized: String = value
            .chars()
            .map(|c| if Self::is_allowed(c) { c } else { '-' })
            .take(MAX_LENGTH)
            .collect();

        Self::new(sanitized.trim_matches('-')).ok()
    }

    pub fn randomized() -> Self {
        Self(Alphanumeric.sample_string(&mut rand::rng(), MAX_LENGTH))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn is_allowed(character: char) -> bool {
        // TODO: Do we need to support more?
        character.is_ascii_alphanumeric() || character == '-' || character == '_'
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Name {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsRef<Path> for Name {
    fn as_ref(&self) -> &Path {
        Path::new(&self.0)
    }
}

impl FromStr for Name {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl<'de> Deserialize<'de> for Name {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_reasonable_names() {
        assert!(Name::new("foo").is_ok());
        assert!(Name::new("foo-bar").is_ok());
        assert!(Name::new("foo_bar").is_ok());
        assert!(Name::new("foo_bar_2").is_ok());
        assert!(Name::new("A1").is_ok());
    }

    #[test]
    fn rejects_invalid_or_not_allowed() {
        assert!(Name::new("").is_err());
        assert!(Name::new("..").is_err());
        assert!(Name::new("../../etc").is_err());
        assert!(Name::new("/etc/foo").is_err());
        assert!(Name::new("x console=ttyS0 init=/bin/sh").is_err());
        assert!(Name::new("x\tinit=/bin/sh").is_err());
        assert!(Name::new("x\ninit=/bin/sh").is_err());
        assert!(Name::new("-cache").is_err());
        assert!(Name::new("-").is_err());
        assert!(Name::new("a".repeat(MAX_LENGTH)).is_ok());
        assert!(Name::new("a".repeat(MAX_LENGTH + 1)).is_err());
    }

    #[test]
    fn sanitizes_arbitrary_input() {
        assert_eq!(
            Name::sanitize("my project!").unwrap().as_str(),
            "my-project"
        );
        assert_eq!(
            Name::sanitize("a".repeat(50).as_str()).unwrap().as_str(),
            "a".repeat(MAX_LENGTH)
        );
        assert!(Name::sanitize("").is_none());
        assert!(Name::sanitize("///").is_none());
    }
}
