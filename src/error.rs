use thiserror::Error;

#[derive(Error, Debug)]
pub enum NydusError {
    #[error("Instance not found: {0}")]
    InstanceNotFound(String),

    #[error("Profile not found: {0}")]
    ProfileNotFound(String),

    #[error("AWS API error: {0}")]
    AwsError(String),

    #[error("State database error: {0}")]
    StateError(#[from] rusqlite::Error),

    #[error("Config error: {0}")]
    ConfigError(String),

    #[error("SSH error: {0}")]
    SshError(String),

    #[error("Tunnel error: {0}")]
    TunnelError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Sync error: {0}")]
    SyncError(String),
}

pub type Result<T> = std::result::Result<T, NydusError>;
