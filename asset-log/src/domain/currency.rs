use std::{fmt, str::FromStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Currency([u8; 3]);

#[derive(Debug, thiserror::Error)]
#[error("invalid currency code: {0}")]
pub struct InvalidCurrency(pub String);

impl FromStr for Currency {
    type Err = InvalidCurrency;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let t = s.trim();
        if t.len() == 3 && t.bytes().all(|b| b.is_ascii_alphabetic()) {
            let u = t.to_ascii_uppercase();
            let b = u.as_bytes();
            Ok(Self([b[0], b[1], b[2]]))
        } else {
            Err(InvalidCurrency(s.to_owned()))
        }
    }
}

impl Currency {
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).expect("ascii")
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl TryFrom<String> for Currency {
    type Error = InvalidCurrency;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl From<Currency> for String {
    fn from(c: Currency) -> Self {
        c.to_string()
    }
}
