use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Audio device error: {0}")]
    AudioDevice(#[from] windows::core::Error),

    #[error("Audio capture failed: {0}")]
    AudioCapture(String),

    #[error("Window operation failed: {0}")]
    WindowError(String),

    #[error("Window handle is invalid")]
    InvalidWindowHandle,

    #[error("Taskbar not found")]
    TaskbarNotFound,

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Config file parse error: {0}")]
    ConfigParseError(#[from] serde_json::Error),

    #[error("File operation failed: {0}")]
    FileError(#[from] std::io::Error),

    #[error("Single instance error: {0}")]
    SingleInstance(String),

    #[error("Initialization error: {0}")]
    Initialization(String),

    #[error("FFT error: {0}")]
    FftError(String),

    #[error("UI rendering error: {0}")]
    RenderError(String),

    #[error("Unknown error: {0}")]
    Unknown(String),
}

pub type Result<T> = std::result::Result<T, AppError>;

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        if let Some(e) = err.downcast_ref::<windows::core::Error>() {
            AppError::AudioDevice(e.clone())
        } else {
            AppError::Unknown(err.to_string())
        }
    }
}

impl From<&str> for AppError {
    fn from(s: &str) -> Self {
        AppError::Unknown(s.to_string())
    }
}

impl From<String> for AppError {
    fn from(s: String) -> Self {
        AppError::Unknown(s)
    }
}