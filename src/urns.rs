#![allow(missing_docs)]

//! URNs used by Sonos devices.

use rupnp::ssdp::URN;

pub const SONOS_URN: URN = URN::device("schemas-upnp-org", "ZonePlayer", 1);
pub const AV_TRANSPORT: &URN = &URN::service("schemas-upnp-org", "AVTransport", 1);
pub const DEVICE_PROPERTIES: &URN = &URN::service("schemas-upnp-org", "DeviceProperties", 1);
pub const RENDERING_CONTROL: &URN = &URN::service("schemas-upnp-org", "RenderingControl", 1);
pub const ZONE_GROUP_TOPOLOGY: &URN = &URN::service("schemas-upnp-org", "ZoneGroupTopology", 1);
pub const CONTENT_DIRECTORY: &URN = &URN::service("schemas-upnp-org", "ContentDirectory", 1);
pub const QUEUE: &URN = &URN::service("schemas-sonos-com", "Queue", 1);
pub const MUSIC_SERVICES: &URN = &URN::service("schemas-upnp-org", "MusicServices", 1);
