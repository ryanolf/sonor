//! A user-friendly API for controlling sonos systems similar to the
//! controller app, with room-by-room (or group-by-group) controls.

mod controller;
mod manager;
mod metadata;
mod error;
mod subscriber;
mod types;
use types::{Command, Response};
mod test;

pub use manager::*;
pub use types::*;
// pub use controller::*;
pub use error::Error;