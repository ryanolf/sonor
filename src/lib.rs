#![warn(
    nonstandard_style,
    rust_2018_idioms,
    future_incompatible,
    missing_debug_implementations,
    missing_docs,
)]

//! This crate is a Sonos controller library written in Rust.
//! It operates asynchronously and aims for a simple to use yet powerful API.
//!
//! # Example
//! ```rust,no_run
//! # use futures::prelude::*;
//! # use std::time::Duration;
//! # async fn f() -> Result<(), sonor::Error> {
//! let speaker = sonor::find("your room name", Duration::from_secs(2)).await?
//!     .expect("room exists");
//!
//! println!("The volume is currently at {}", speaker.volume().await?);
//!
//! match speaker.track().await? {
//!     Some(track_info) => println!("- Currently playing '{}", track_info.track()),
//!     None => println!("- No track currently playing"),
//! }
//!
//! speaker.clear_queue().await?;
//!
//! speaker.join("some other room").await?;
//!
//! # Ok(())
//! # };
//! ```
//! For a full list of actions implemented, look at the [Speaker](struct.Speaker.html) docs.
//!
//! If your use case isn't covered, this crate also exposes the raw UPnP Action API
//! [here](struct.Speaker.html#method.action).
//! It can be used like this:
//! ```rust,no_run
//! # use futures::prelude::*;
//! # use std::time::Duration;
//! use sonor::URN;
//! # async fn f() -> Result<(), sonor::Error> {
//! # let speaker = sonor::find("your room name", Duration::from_secs(2)).await?.expect("room exists");
//!
//! let service = URN::service("schemas-upnp-org", "GroupRenderingControl", 1);
//! let args = "<InstanceID>0</InstanceID>";
//! let response = speaker.action(&service, "GetGroupMute", args).await?;
//!
//! println!("{}", response["CurrentMute"]);
//!
//! # Ok(())
//! # };
//! ```

pub mod manager;
mod datatypes;
mod discovery;
mod snapshot;
mod speaker;
mod track;
mod content;
mod utils;

pub use datatypes::{RepeatMode, SpeakerInfo};
pub use discovery::{discover, discover_one, find};
pub use rupnp::{self, http::Uri, ssdp::URN, Service};
pub use snapshot::Snapshot;
pub use speaker::Speaker;
use thiserror::*;
pub use track::{Track, TrackInfo};

/// Represents an error encountered by Sonor
#[derive(Error, Debug)]
pub enum Error {
    /// Errors sourced from the rupnp crate
    #[error(transparent)]
    UPnP(#[from] rupnp::Error),
    /// Errors sourced from XML parsing
    #[error(transparent)]
    Xml(#[from] roxmltree::Error),
    /// Errors source from URI manipulation
    #[error(transparent)]
    InvalidUri(#[from] http::uri::InvalidUri),
    /// This error is produced when attempting to perform an action and
    /// the specified service is not present.
    #[error("Service {service} was not found when performing {action} with {payload}")]
    MissingServiceForUPnPAction {
        /// The required service for the action
        service: URN,
        /// The action to be performed
        action: String,
        /// The action payload
        payload: String,
    },
    /// The discovery can return an empty stream of speakers.
    #[error("No speakers detected")]
    NoSpeakersDetected,
    /// An impossible? situation where a speaker isn't included
    /// in its own zone group state
    #[error("asked for zone group state but the speaker doesn't seem to be included there")]
    SpeakerNotIncludedInOwnZoneGroupState,
    /// An impossible? situation where GetZoneGroupState returned non-Sonos devices
    #[error("The Sonos-specific GetZoneGroupState action returned non-Sonos devices")]
    GetZoneGroupStateReturnedNonSonos,
    /// An impossible? situation where non-Sonos devices responded
    /// to UPnP discovery for Sono devices
    #[error("UPnP discovery for Sonos devices returned non-Sonos devices")]
    NonSonosDevicesInSonosUPnPDiscovery,
}

type Result<T, E = Error> = std::result::Result<T, E>;
