use std::{error::Error, fmt, io};

#[derive(Debug)]
pub enum SyncError {
    Io {
        operation: String,
        source: io::Error,
    },
    Subtitle(String),
    Audio(String),
    Ffmpeg(String),
    Config(String),
    NoSpeech,
    Json(serde_json::Error),
}

impl SyncError {
    pub fn io(operation: impl Into<String>, source: io::Error) -> Self {
        Self::Io {
            operation: operation.into(),
            source,
        }
    }
}

impl fmt::Display for SyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { operation, source } => write!(f, "{operation}: {source}"),
            Self::Subtitle(message) => write!(f, "Invalid subtitle: {message}"),
            Self::Audio(message) => write!(f, "Invalid audio: {message}"),
            Self::Ffmpeg(message) => write!(f, "FFmpeg failed: {message}"),
            Self::Config(message) => write!(f, "Invalid sync configuration: {message}"),
            Self::NoSpeech => f.write_str("No usable speech was detected in the audio"),
            Self::Json(error) => write!(f, "Could not encode the sync report: {error}"),
        }
    }
}

impl Error for SyncError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}
