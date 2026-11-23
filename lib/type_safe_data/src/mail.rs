use std::fmt::{Display, Formatter, write};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EmailAddress(String);

impl EmailAddress {
    pub fn new(s: impl AsRef<str>) -> Option<Self> {
        todo!()
    }
}

impl AsRef<str> for EmailAddress {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Display for EmailAddress {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<EmailAddress> for String {
    fn from(value: EmailAddress) -> Self {
        value.0
    }
}
