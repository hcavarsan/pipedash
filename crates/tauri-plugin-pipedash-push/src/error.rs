use serde::{
    ser::Serializer,
    Serialize,
};

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Push notifications are not supported on this platform")]
    Unsupported,
    #[error("Notification permission was denied")]
    PermissionDenied,
    #[error("Failed to register for push notifications: {0}")]
    Registration(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.to_string().as_ref())
    }
}
