use crate::{
    args,
    content::Content,
    track::{Track, TrackInfo},
    urns::*,
    utils::{self, HashMapExt},
    Error, RepeatMode, Result, Snapshot,
};

use roxmltree::{Document, Node};
use rupnp::{ssdp::URN, Device};
use std::{collections::HashMap, hash::Hash, hash::Hasher, net::Ipv4Addr};

pub(crate) const EXTRA_DEVICE_FIELDS: &[&str; 2] = &["roomName", "UDN"];

const DEFAULT_ARGS: &str = "<InstanceID>0</InstanceID>";

#[derive(Debug, Clone)]
/// A sonos speaker, wrapping a UPnP-Device and providing user-oriented methods in an asynyronous
/// API.
pub struct Speaker {
    device: Device,
    info: SpeakerInfo,
}

#[allow(missing_docs)]
impl Speaker {
    /// Creates a speaker from an already found UPnP-Device.
    /// Returns `None` when the URN type doesn't match the `schemas-upnp-org:device:ZonePlayer:1`,
    /// which is used by sonos devices.
    pub fn from_device(device: Device) -> Option<Self> {
        if device.device_type() == &SONOS_URN {
            let name = device.get_extra_property("roomName")?.to_string();
            let uuid = device.get_extra_property("UDN")?[5..].to_string();
            let location = device.url().to_string();
            let info = SpeakerInfo {
                name,
                uuid,
                location,
            };
            Some(Self { device, info })
        } else {
            None
        }
    }

    /// Creates a speaker from an IPv4 address.
    /// It returns `Ok(None)` when the device was found but isn't a sonos player.
    pub async fn from_ip(addr: Ipv4Addr) -> Result<Option<Speaker>> {
        let uri = format!("http://{}:1400/xml/device_description.xml", addr)
            .parse()
            .expect("is always valid");

        Ok(Device::from_url_and_properties(uri, EXTRA_DEVICE_FIELDS)
            .await
            .map(Speaker::from_device)?)
    }

    // Creates a speaker from a SpeakerInfo
    pub async fn from_speaker_info(info: &SpeakerInfo) -> Result<Option<Self>> {
        let url = info.location().parse();
        let device = Device::from_url_and_properties(url?, EXTRA_DEVICE_FIELDS).await?;
        Ok(Self::from_device(device))
    }

    pub fn device(&self) -> &Device {
        &self.device
    }

    pub async fn update_name(&mut self) -> Result<&str> {
        if let Ok(new_name) = self
            .action(DEVICE_PROPERTIES, "GetZoneAttributes", "")
            .await?
            .extract("CurrentZoneName")
        {
            self.info.name = new_name;
        };
        Ok(&self.info.name)
    }

    pub fn name(&self) -> &str {
        &self.info.name
    }

    pub fn uuid(&self) -> &str {
        &self.info.uuid
    }

    // AV_TRANSPORT
    pub async fn stop(&self) -> Result<()> {
        self.action(AV_TRANSPORT, "Stop", DEFAULT_ARGS)
            .await
            .map(drop)
    }
    pub async fn play(&self) -> Result<()> {
        self.action(AV_TRANSPORT, "Play", args! { "InstanceID": 0, "Speed": 1 })
            .await
            .map(drop)
    }
    pub async fn pause(&self) -> Result<()> {
        let res = self.action(AV_TRANSPORT, "Pause", DEFAULT_ARGS).await;
        match res {
            Ok(_) => Ok(()),
            Err(Error::UPnP(rupnp::Error::HttpErrorCode(code))) if code.as_u16() == 500 => Ok(()),
            Err(Error::UPnP(rupnp::Error::UPnPError(err))) if err.err_code() == 701 => Ok(()),
            Err(err) => Err(err),
        }
    }

    pub async fn play_or_pause(&self) -> Result<()> {
        if self.is_playing().await? {
            self.pause().await
        } else {
            self.play().await
        }
    }

    pub async fn next(&self) -> Result<()> {
        self.action(AV_TRANSPORT, "Next", DEFAULT_ARGS)
            .await
            .map(drop)
    }
    pub async fn previous(&self) -> Result<()> {
        self.action(AV_TRANSPORT, "Previous", DEFAULT_ARGS)
            .await
            .map(drop)
    }

    pub async fn skip_to(&self, seconds: u32) -> Result<()> {
        let args = args! { "InstanceID": 0, "Unit": "REL_TIME", "Target": utils::seconds_to_str(seconds.into())};
        self.action(AV_TRANSPORT, "Seek", args).await.map(drop)
    }
    pub async fn skip_by(&self, seconds: i32) -> Result<()> {
        let args = args! { "InstanceID": 0, "Unit": "TIME_DELTA", "Target": utils::seconds_to_str(seconds.into())};
        self.action(AV_TRANSPORT, "Seek", args).await.map(drop)
    }
    /// The first track number is 1.
    pub async fn seek_track(&self, track_no: u32) -> Result<()> {
        let args = args! { "InstanceID": 0, "Unit": "TRACK_NR", "Target": track_no };
        self.action(AV_TRANSPORT, "Seek", args).await.map(drop)
    }

    pub async fn playback_mode(&self) -> Result<(RepeatMode, bool)> {
        let play_mode = self
            .action(AV_TRANSPORT, "GetTransportSettings", DEFAULT_ARGS)
            .await?
            .extract("PlayMode")?;

        match play_mode.to_uppercase().as_str() {
            "NORMAL" => Ok((RepeatMode::None, false)),
            "REPEAT_ALL" => Ok((RepeatMode::All, false)),
            "REPEAT_ONE" => Ok((RepeatMode::One, false)),
            "SHUFFLE_NOREPEAT" => Ok((RepeatMode::None, true)),
            "SHUFFLE" => Ok((RepeatMode::All, true)),
            "SHUFFLE_REPEAT_ONE" => Ok((RepeatMode::One, true)),
            _ => Err(Error::UPnP(rupnp::Error::invalid_response(
                crate::datatypes::ParseRepeatModeError,
            ))),
        }
    }
    pub async fn repeat_mode(&self) -> Result<RepeatMode> {
        self.playback_mode()
            .await
            .map(|(repeat_mode, _)| repeat_mode)
    }
    pub async fn shuffle(&self) -> Result<bool> {
        self.playback_mode().await.map(|(_, shuffle)| shuffle)
    }

    pub async fn set_playback_mode(&self, repeat_mode: RepeatMode, shuffle: bool) -> Result<()> {
        let playback_mode = match (repeat_mode, shuffle) {
            (RepeatMode::None, false) => "NORMAL",
            (RepeatMode::One, false) => "REPEAT_ONE",
            (RepeatMode::All, false) => "REPEAT_ALL",
            (RepeatMode::None, true) => "SHUFFLE_NOREPEAT",
            (RepeatMode::One, true) => "SHUFFLE_REPEAT_ONE",
            (RepeatMode::All, true) => "SHUFFLE",
        };
        self.action(
            AV_TRANSPORT,
            "SetPlayMode",
            args! { "InstanceID": 0, "NewPlayMode": playback_mode },
        )
        .await
        .map(drop)
    }
    pub async fn set_repeat_mode(&self, repeat_mode: RepeatMode) -> Result<()> {
        self.set_playback_mode(repeat_mode, self.shuffle().await?)
            .await
    }
    pub async fn set_shuffle(&self, shuffle: bool) -> Result<()> {
        self.set_playback_mode(self.repeat_mode().await?, shuffle)
            .await
    }

    pub async fn crossfade(&self) -> Result<bool> {
        self.action(AV_TRANSPORT, "GetCrossfadeMode", DEFAULT_ARGS)
            .await?
            .extract("CrossfadeMode")
            .and_then(utils::parse_bool)
    }
    pub async fn set_crossfade(&self, crossfade: bool) -> Result<()> {
        let args = args! { "InstanceID": 0, "CrossfadeMode": crossfade as u8 };
        self.action(AV_TRANSPORT, "SetCrossfadeMode", args)
            .await
            .map(drop)
    }

    pub async fn is_playing(&self) -> Result<bool> {
        self.action(AV_TRANSPORT, "GetTransportInfo", DEFAULT_ARGS)
            .await?
            .extract("CurrentTransportState")
            .map(|x| x.eq_ignore_ascii_case("playing"))
    }

    pub async fn track(&self) -> Result<Option<TrackInfo>> {
        let mut map = self
            .action(AV_TRANSPORT, "GetPositionInfo", DEFAULT_ARGS)
            .await?;

        let track_no: u32 = map.extract("Track")?.parse().unwrap();
        let duration = map
            .extract("TrackDuration")
            .unwrap_or_else(|_| "0:0:0".into());
        let elapsed = map.extract("RelTime").unwrap_or_else(|_| "0:0:0".into());

        // e.g. speaker was playing spotify, then spotify disconnected but sonos is still on
        // "x-sonos-vli"
        if duration.eq_ignore_ascii_case("not_implemented")
            || elapsed.eq_ignore_ascii_case("not_implemented")
        {
            return Ok(None);
        }

        let metadata = match map.remove("TrackMetaData") {
            Some(metadata) => metadata,
            None => return Ok(None),
        };

        let duration = utils::seconds_from_str(&duration)?;
        let elapsed = utils::seconds_from_str(&elapsed)?;

        let doc = Document::parse(&metadata)?;
        let item = utils::find_root_node(&doc, "item", "Track Metadata")?;
        let track = Track::from_xml(item)?;

        Ok(Some(TrackInfo::new(
            track, metadata, track_no, duration, elapsed,
        )))
    }

    // RENDERING_CONTROL

    pub async fn volume(&self) -> Result<u16> {
        let args = args! { "InstanceID": 0, "Channel": "Master" };
        self.action(RENDERING_CONTROL, "GetVolume", args)
            .await?
            .extract("CurrentVolume")
            .and_then(|x| {
                x.parse()
                    .map_err(|e| rupnp::Error::invalid_response(e).into())
            })
    }
    pub async fn set_volume(&self, volume: u16) -> Result<()> {
        let args = args! { "InstanceID": 0, "Channel": "Master", "DesiredVolume": volume };
        self.action(RENDERING_CONTROL, "SetVolume", args)
            .await
            .map(drop)
    }
    pub async fn set_volume_relative(&self, adjustment: i32) -> Result<u16> {
        let args = args! { "InstanceID": 0, "Channel": "Master", "Adjustment": adjustment };
        self.action(RENDERING_CONTROL, "SetRelativeVolume", args)
            .await?
            .extract("NewVolume")
            .and_then(|x| {
                x.parse()
                    .map_err(|e| rupnp::Error::invalid_response(e).into())
            })
    }

    pub async fn mute(&self) -> Result<bool> {
        let args = args! { "InstanceID": 0, "Channel": "Master" };
        self.action(RENDERING_CONTROL, "GetMute", args)
            .await?
            .extract("CurrentMute")
            .and_then(utils::parse_bool)
    }
    pub async fn set_mute(&self, mute: bool) -> Result<()> {
        let args = args! { "InstanceID": 0, "Channel": "Master", "DesiredMute": mute as u8 };
        self.action(RENDERING_CONTROL, "SetMute", args)
            .await
            .map(drop)
    }

    pub async fn bass(&self) -> Result<i8> {
        self.action(RENDERING_CONTROL, "GetBass", DEFAULT_ARGS)
            .await?
            .extract("CurrentBass")
            .and_then(|x| {
                x.parse()
                    .map_err(|e| rupnp::Error::invalid_response(e).into())
            })
    }
    pub async fn set_bass(&self, bass: i8) -> Result<()> {
        let args = args! { "InstanceID": 0, "DesiredBass": bass };
        self.action(RENDERING_CONTROL, "SetBass", args)
            .await
            .map(drop)
    }
    pub async fn treble(&self) -> Result<i8> {
        self.action(RENDERING_CONTROL, "GetTreble", DEFAULT_ARGS)
            .await?
            .extract("CurrentTreble")
            .and_then(|x| {
                x.parse()
                    .map_err(|e| rupnp::Error::invalid_response(e).into())
            })
    }
    pub async fn set_treble(&self, treble: i8) -> Result<()> {
        self.action(
            RENDERING_CONTROL,
            "SetTreble",
            args! { "InstanceID": 0, "DesiredTreble": treble },
        )
        .await
        .map(drop)
    }
    pub async fn loudness(&self) -> Result<bool> {
        let args = args! { "InstanceID": 0, "Channel": "Master" };
        self.action(RENDERING_CONTROL, "GetLoudness", args)
            .await?
            .extract("CurrentLoudness")
            .and_then(utils::parse_bool)
    }
    pub async fn set_loudness(&self, loudness: bool) -> Result<()> {
        let args =
            args! { "InstanceID": 0, "Channel": "Master", "DesiredLoudness": loudness as u8 };
        self.action(RENDERING_CONTROL, "SetLoudness", args)
            .await
            .map(drop)
    }

    // Queue
    pub async fn queue(&self) -> Result<Vec<Track>> {
        let args = args! { "QueueID": 0, "StartingIndex": 0, "RequestedCount": std::u32::MAX };
        let result = self
            .action(QUEUE, "Browse", args)
            .await?
            .extract("Result")?;

        Document::parse(&result)?
            .root()
            .first_element_child()
            .ok_or_else(|| rupnp::Error::ParseError("Queue Response contains no children"))?
            .children()
            .filter(roxmltree::Node::is_element)
            .map(Track::from_xml)
            .collect()
    }

    // TODO test the next ones
    pub async fn remove_track(&self, track_no: u32) -> Result<()> {
        let args = args! { "InstanceID": 0, "ObjectID": format!("Q:0/{}", track_no + 1) };
        self.action(AV_TRANSPORT, "RemoveTrackFromQueue", args)
            .await
            .map(drop)
    }

    /// Enqueues a track at the end of the queue.
    pub async fn queue_end(&self, uri: &str, metadata: &str) -> Result<()> {
        let args = args! { "InstanceID": 0, "EnqueuedURI": uri, "EnqueuedURIMetaData": metadata, "DesiredFirstTrackNumberEnqueued": 0, "EnqueueAsNext": 0 };
        self.action(AV_TRANSPORT, "AddURIToQueue", args)
            .await
            .map(drop)
    }

    /// Enqueues a track as the next one.
    pub async fn queue_next(&self, uri: &str, metadata: &str, track_no: Option<u32>) -> Result<()> {
        let args = args! { "InstanceID": 0, "EnqueuedURI": uri, "EnqueuedURIMetaData": metadata, "DesiredFirstTrackNumberEnqueued": track_no.unwrap_or(0), "EnqueueAsNext": 1 };
        self.action(AV_TRANSPORT, "AddURIToQueue", args)
            .await
            .map(drop)
    }

    pub async fn clear_queue(&self) -> Result<()> {
        self.action(AV_TRANSPORT, "RemoveAllTracksFromQueue", DEFAULT_ARGS)
            .await
            .map(drop)
    }

    pub(crate) async fn _zone_group_state(&self) -> Result<Vec<(String, Vec<SpeakerInfo>)>> {
        let state = self
            .action(ZONE_GROUP_TOPOLOGY, "GetZoneGroupState", "")
            .await?
            .extract("ZoneGroupState")?;

        extract_zone_topology(&state)
    }

    /// Returns all groups in the system as a map from the group coordinators UUID to a list of [Speaker Info](struct.SpeakerInfo.html)s.
    pub async fn zone_group_state(&self) -> Result<HashMap<String, Vec<SpeakerInfo>>> {
        Ok(self._zone_group_state().await?.into_iter().collect())
    }

    /// Form a group with a player.
    /// The UUID should look like this: 'RINCON_000E5880EA7601400'.
    async fn join_uuid(&self, uuid: &str) -> Result<()> {
        let args = args! { "InstanceID": 0, "CurrentURI": format!("x-rincon:{}", uuid), "CurrentURIMetaData": "" };
        self.action(AV_TRANSPORT, "SetAVTransportURI", args)
            .await
            .map(drop)
    }

    /// Form a group with a player.
    /// Returns `false` when no player with that roomname exists.
    /// `roomname` is compared case insensitively.
    pub async fn join(&self, roomname: &str) -> Result<bool> {
        let topology = self._zone_group_state().await?;
        let uuid = topology
            .iter()
            .flat_map(|(_, speakers)| speakers)
            .find(|speaker_info| speaker_info.name().eq_ignore_ascii_case(roomname))
            .map(SpeakerInfo::uuid);

        if let Some(uuid) = uuid {
            self.join_uuid(uuid).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Leave the current group.
    /// Does nothing when the speaker already has no group.
    pub async fn leave(&self) -> Result<()> {
        self.action(
            AV_TRANSPORT,
            "BecomeCoordinatorOfStandaloneGroup",
            DEFAULT_ARGS,
        )
        .await
        .map(drop)
    }

    /// Set the transport URI for the speaker.
    /// Note that (at least my old Play:5 gen 1 speaker) will only accept urls without
    /// '?foo=bar' query parameters that end with '.mp3' or '.wav' etc.
    pub async fn set_transport_uri(&self, uri: &str, metadata: &str) -> Result<()> {
        let args = args! { "InstanceID": 0, "CurrentURI": uri, "CurrentURIMetaData": metadata };
        self.action(AV_TRANSPORT, "SetAVTransportURI", args)
            .await
            .map(drop)
    }

    /// Get the current transport URI for the speaker.
    pub async fn transport_uri(&self) -> Result<Option<String>> {
        let uri = self
            .action(AV_TRANSPORT, "GetMediaInfo", DEFAULT_ARGS)
            .await?
            .remove("CurrentURI");
        Ok(uri)
    }

    #[allow(unused)]
    /// returns a map of lowercase service name to a tuple of (sid, capabilities, stype)
    async fn music_services(&self) -> Result<(Vec<u32>, HashMap<String, (u32, u32, u32)>)> {
        let mut map = self
            .action(MUSIC_SERVICES, "ListAvailableServices", "")
            .await?;
        let descriptor_list = map.extract("AvailableServiceDescriptorList")?;
        let service_type_list = map.extract("AvailableServiceTypeList")?;

        let available_services: Vec<u32> = service_type_list
            .split(',')
            .map(|x| x.parse())
            .collect::<Result<_, _>>()
            .map_err(rupnp::Error::invalid_response)?;

        let document = Document::parse(&descriptor_list)?;
        let services = utils::find_root_node(&document, "Services", "DescriptorList")?
            .children()
            .map(|node| -> Result<_> {
                let id = utils::try_find_node_attribute(node, "Id")?;
                let name = utils::try_find_node_attribute(node, "Name")?;
                let capabilities = utils::try_find_node_attribute(node, "Capabilities")?;

                let id = id.parse().map_err(rupnp::Error::invalid_response)?;
                let capabilities = capabilities
                    .parse()
                    .map_err(rupnp::Error::invalid_response)?;
                let s_type = id << (8 + 7);
                Ok((name.to_lowercase(), (id, capabilities, s_type)))
            })
            .collect::<Result<_, _>>()?;

        Ok((available_services, services))
    }

    pub async fn browse(&self, object_id: &str, start: u32, limit: u32) -> Result<Vec<Content>> {
        let args = args! { "ObjectID": object_id, "BrowseFlag": "BrowseDirectChildren", "StartingIndex": start, "RequestedCount": limit, "Filter" : "", "SortCriteria" : "" };
        let result = self
            .action(CONTENT_DIRECTORY, "Browse", args)
            .await?
            .extract("Result")?;
        // log::debug!("{:#?}", result);

        Document::parse(&result)?
            .root()
            .first_element_child()
            .ok_or_else(|| rupnp::Error::ParseError("Browse Response contains no children"))?
            .children()
            .filter(roxmltree::Node::is_element)
            .map(Content::from_xml)
            .collect()
    }

    /// Take a snapshot of the state the speaker is in right now.
    /// The saved information is the speakers volume, it's currently played song and were you were in the song.
    pub async fn snapshot(&self) -> Result<Snapshot> {
        Snapshot::from_speaker(self).await
    }

    /// Applies a snapshot previously taken by the [snapshot](struct.Speaker.html#method.snapshot)-method.
    pub async fn apply(&self, snapshot: Snapshot) -> Result<()> {
        snapshot.apply(self).await
    }

    /// Execute some UPnP Action on the device.
    /// A list of services, devices and actions of the 'ZonePlayer:1' standard can be found [here](https://github.com/jakobhellermann/sonos/tree/master/zoneplayer).
    pub async fn action(
        &self,
        service: &URN,
        action: &str,
        payload: &str,
    ) -> Result<HashMap<String, String>> {
        Ok(self
            .device
            .find_service(service)
            .ok_or_else(|| Error::MissingServiceForUPnPAction {
                service: service.clone(),
                action: action.to_string(),
                payload: payload.to_string(),
            })?
            .action(self.device.url(), action, payload)
            .await?)
    }
}

/// A more lightweight representation of a speaker containing only the name, uuid and location.
/// It gets returned by the [zone_group_state](struct.Speaker.html#method.zone_group_state) function.
#[derive(Debug, Eq, Clone)]
pub struct SpeakerInfo {
    name: String,
    uuid: String,
    location: String,
}
impl PartialEq for SpeakerInfo {
    fn eq(&self, other: &Self) -> bool {
        self.uuid().eq_ignore_ascii_case(other.uuid())
    }
}
impl Hash for SpeakerInfo {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.uuid.hash(state);
    }
}

#[allow(missing_docs)]
impl SpeakerInfo {
    pub fn from_xml(node: Node<'_, '_>) -> Result<Self> {
        let mut uuid = None;
        let mut name = None;
        let mut location = None;

        for attr in node.attributes() {
            match attr.name().to_lowercase().as_str() {
                "uuid" => uuid = Some(attr.value()),
                "location" => location = Some(attr.value()),
                "zonename" => name = Some(attr.value()),
                _ => (),
            }
        }

        Ok(Self {
            name: name
                .ok_or_else(|| {
                    rupnp::Error::XmlMissingElement(
                        "RoomName".to_string(),
                        "ZoneGroupMember".to_string(),
                    )
                })?
                .to_string(),
            uuid: uuid
                .ok_or_else(|| {
                    rupnp::Error::XmlMissingElement(
                        "UUID".to_string(),
                        "ZoneGroupMember".to_string(),
                    )
                })?
                .to_string(),
            location: location
                .ok_or_else(|| {
                    rupnp::Error::XmlMissingElement(
                        "Location".to_string(),
                        "ZoneGroupMember".to_string(),
                    )
                })?
                .to_string(),
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn uuid(&self) -> &str {
        &self.uuid
    }
    pub fn location(&self) -> &str {
        &self.location
    }
}

/// Extracts the zone topology from the given XML string, which should contain a
/// `<ZoneGroups>` element.
///
/// Returns a vector of tuples, where the first element is the coordinator's
/// UUID and the second element is a vector of
/// [SpeakerInfo](struct.SpeakerInfo.html)s.
pub fn extract_zone_topology(state_xml: &str) -> Result<Vec<(String, Vec<SpeakerInfo>)>> {
    let doc = Document::parse(&state_xml)?;
    let state = utils::find_root_node(&doc, "ZoneGroups", "Zone Group Topology")?;

    state
        .children()
        .filter(Node::is_element)
        .filter(|c| c.tag_name().name().eq_ignore_ascii_case("ZoneGroup"))
        .map(|group| {
            let coordinator = utils::try_find_node_attribute(group, "Coordinator")?.to_string();
            let members = group
                .children()
                .filter(Node::is_element)
                .filter(|c| c.tag_name().name().eq_ignore_ascii_case("ZoneGroupMember"))
                .filter(|c| c.attribute("Invisible") != Some("1"))
                .map(SpeakerInfo::from_xml)
                .collect::<Result<Vec<_>>>()?;
            Ok((coordinator, members))
        })
        .collect()
}
