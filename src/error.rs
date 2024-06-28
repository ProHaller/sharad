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
    InvalidMenuSelection(String),
    InputError(String),
    AudioRecordingError(String),
    AudioPlaybackError(String),
    MissingAPIKey(String),
    Hound(hound::Error), // New variant for hound::Error
}

impl fmt::Display for SharadError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SharadError::Io(e) => write!(f, "IO error: {}", e),
            SharadError::OpenAI(e) => write!(f, "OpenAI error: {}", e),
            SharadError::SerdeJson(e) => write!(f, "Serde JSON error: {}", e),
            SharadError::Other(e) => write!(f, "Other error: {}", e),
            SharadError::Message(e) => write!(f, "{}", e),
            SharadError::InvalidMenuSelection(e) => write!(f, "Invalid menu selection: {}", e),
            SharadError::InputError(e) => write!(f, "Input error: {}", e),
            SharadError::AudioRecordingError(e) => write!(f, "Audio recording error: {}", e),
            SharadError::AudioPlaybackError(e) => write!(f, "Audio playback error: {}", e),
            SharadError::MissingAPIKey(key) => write!(f, "Missing API key: {}", key),
            SharadError::Hound(e) => write!(f, "Hound error: {}", e), // New display implementation
        }
    }
}

impl Error for SharadError {}

impl From<std::io::Error> for SharadError {
    fn from(error: std::io::Error) -> Self {
        SharadError::Io(error)
    }
}

impl From<async_openai::error::OpenAIError> for SharadError {
    fn from(error: async_openai::error::OpenAIError) -> Self {
        SharadError::OpenAI(error)
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

impl From<hound::Error> for SharadError {
    fn from(error: hound::Error) -> Self {
        SharadError::Hound(error)
    }
}
