use thiserror::*;

#[derive(Error, Debug)]
/// Errors relevant to Manager module
pub enum Error {
    /// Errors from the core crate
    #[error(transparent)]
    Sonor(#[from] crate::Error),
    /// Error with subscriptions
    #[error("Error in event subscription: {0}")]
    SubscriberError(String),
    /// If the controller panics or drops the receiver
    #[error("Controller has dropped the receiver")]
    MessageSendError,
    /// If the controller panics formulation a response
    #[error("Controller has dropped the response sender")]
    MessageRecvError,
    /// Controller not initialized
    #[error("Controller not initialized")]
    ControllerNotInitialized,
    /// Zone does not exist
    #[error("The requested zone name is not valid")]
    ZoneDoesNotExist,
}