use crate::lexer::LexError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseError {
    pub message: std::string::String,
    pub location: usize,
}

impl ParseError {
    pub(super) fn new(location: usize, message: impl Into<std::string::String>) -> Self {
        Self {
            location,
            message: message.into(),
        }
    }
}

impl From<LexError> for ParseError {
    fn from(value: LexError) -> Self {
        Self {
            message: value.message,
            location: value.location,
        }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at byte {}", self.message, self.location)
    }
}

impl std::error::Error for ParseError {}
