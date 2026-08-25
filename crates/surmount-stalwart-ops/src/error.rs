use std::fmt;
use std::io;

#[derive(Debug)]
pub struct ToolError {
    pub message: String,
    pub exit_code: i32,
}

impl ToolError {
    pub fn fail(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            exit_code: 1,
        }
    }
    pub fn blocked(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            exit_code: 2,
        }
    }
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for ToolError {}
impl From<io::Error> for ToolError {
    fn from(e: io::Error) -> Self {
        Self::fail(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, ToolError>;
