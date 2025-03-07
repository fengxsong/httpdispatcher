use std::io;
use thiserror::Error;
use std::fmt;

#[derive(Error, Debug)]
pub enum RuntimeError {
    #[error("Component not found: {0}")]
    ComponentNotFound(String),

    #[error("Invalid input for component {0}: {1}")]
    InvalidInput(String, String),

    #[error("Component {component} failed to process event {event_id}: {reason}")]
    ProcessingError {
        component: String,
        event_id: String,
        reason: String,
    },

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Initialization error: {0}")]
    InitError(String),

    #[error("Channel error: {0}")]
    ChannelError(String),

    #[error("IO error: {0}")]
    IoError(#[from] io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("{context}: {source}")]
    WithContext {
        context: String,
        source: Box<RuntimeError>,
    },

    #[error("Runtime error: {0}")]
    Other(String),
}

impl RuntimeError {
    pub fn component_not_found(name: impl Into<String>) -> Self {
        RuntimeError::ComponentNotFound(name.into())
    }

    pub fn invalid_input(component: impl Into<String>, message: impl Into<String>) -> Self {
        RuntimeError::InvalidInput(component.into(), message.into())
    }

    pub fn processing_error(
        component: impl Into<String>,
        event_id: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        RuntimeError::ProcessingError {
            component: component.into(),
            event_id: event_id.into(),
            reason: reason.into(),
        }
    }

    pub fn config_error(msg: impl Into<String>) -> Self {
        RuntimeError::ConfigError(msg.into())
    }

    pub fn init_error(message: impl Into<String>) -> Self {
        RuntimeError::InitError(message.into())
    }

    pub fn channel_error(message: impl Into<String>) -> Self {
        RuntimeError::ChannelError(message.into())
    }

    pub fn with_context(self, context: impl Into<String>) -> Self {
        RuntimeError::WithContext {
            context: context.into(),
            source: Box::new(self),
        }
    }

    /// Returns true if the error is likely temporary and the operation could succeed if retried
    pub fn is_temporary(&self) -> bool {
        matches!(
            self,
            RuntimeError::IoError(_) | RuntimeError::ChannelError(_)
        )
    }

    /// Returns true if the error is fatal and retrying would not help
    pub fn is_fatal(&self) -> bool {
        matches!(
            self,
            RuntimeError::ConfigError(_) |
            RuntimeError::ComponentNotFound(_) |
            RuntimeError::InvalidInput(_, _)
        )
    }

    /// Returns true if this is a configuration-related error
    pub fn is_config_error(&self) -> bool {
        matches!(self, RuntimeError::ConfigError(_))
    }
}

impl From<anyhow::Error> for RuntimeError {
    fn from(err: anyhow::Error) -> Self {
        RuntimeError::Other(err.to_string())
    }
}

impl From<fmt::Error> for RuntimeError {
    fn from(err: fmt::Error) -> Self {
        RuntimeError::Other(err.to_string())
    }
}

impl From<serde_json::Error> for RuntimeError {
    fn from(err: serde_json::Error) -> Self {
        RuntimeError::SerializationError(err.to_string())
    }
}

impl From<toml::de::Error> for RuntimeError {
    fn from(err: toml::de::Error) -> Self {
        RuntimeError::SerializationError(err.to_string())
    }
}

impl From<serde_yaml::Error> for RuntimeError {
    fn from(err: serde_yaml::Error) -> Self {
        RuntimeError::SerializationError(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, RuntimeError>; 