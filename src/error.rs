use std::error::Error;
use std::fmt;
use tokio::task::JoinError;

#[derive(Debug)]
pub enum SharadError {
    Io(std::io::Error),
    OpenAI(async_openai::error::OpenAIError),
    SerdeJson(serde_json::Error),
    Other(String),
    Message(String),
}

impl fmt::Display for SharadError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SharadError::Io(e) => write!(f, "IO error: {}", e),
            SharadError::OpenAI(e) => write!(f, "OpenAI error: {}", e),
            SharadError::SerdeJson(e) => write!(f, "Serde JSON error: {}", e),
            SharadError::Other(e) => write!(f, "Other error: {}", e),
            SharadError::Message(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for SharadError {}

impl From<std::io::Error> for SharadError {
    fn from(err: std::io::Error) -> Self {
        SharadError::Io(err)
    }
}

impl From<async_openai::error::OpenAIError> for SharadError {
    fn from(err: async_openai::error::OpenAIError) -> Self {
        SharadError::OpenAI(err)
    }
}

impl From<serde_json::Error> for SharadError {
    fn from(err: serde_json::Error) -> Self {
        SharadError::SerdeJson(err)
    }
}
impl From<Box<dyn Error>> for SharadError {
    fn from(err: Box<dyn Error>) -> Self {
        SharadError::Other(err.to_string())
    }
}

impl From<JoinError> for SharadError {
    fn from(err: JoinError) -> Self {
        SharadError::Other(err.to_string())
    }
}
