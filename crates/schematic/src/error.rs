use std::fmt;

use crate::reader::ReadError;
use crate::value::Value;

pub const INTERRUPT: u64 = u64::MAX;

#[derive(Debug, Clone)]
pub struct Error {
    pub payload: Value,
    pub escape: Option<u64>,
}

impl Error {
    pub fn new(kind: &str, message: impl Into<String>) -> Error {
        let payload = Value::map([
            (Value::keyword("kind"), Value::keyword(kind)),
            (Value::keyword("message"), Value::string(&message.into())),
        ]);
        Error { payload, escape: None }
    }

    pub fn interrupt() -> Error {
        Error { escape: Some(INTERRUPT), ..Error::new("interrupted", "interrupted") }
    }

    pub fn raise(payload: Value) -> Error {
        Error { payload, escape: None }
    }

    pub fn escape(token: u64, value: Value) -> Error {
        Error { payload: value, escape: Some(token) }
    }

    pub fn syntax(message: impl Into<String>) -> Error {
        Error::new("syntax-error", message)
    }

    pub fn type_error(message: impl Into<String>) -> Error {
        Error::new("type-error", message)
    }

    pub fn message(&self) -> String {
        if let Value::Map(m) = &self.payload
            && let Some(Value::Str(s)) = m.get(&Value::keyword("message")) {
                return s.to_string();
            }
        self.payload.to_string()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for Error {}

impl From<ReadError> for Error {
    fn from(e: ReadError) -> Error {
        Error::new("read-error", e.to_string())
    }
}
