#![allow(missing_docs, indirect_structural_match)]

//! API backend for tracking sonos system topology

mod zoneaction;
pub use zoneaction::ZoneAction;

use super::{subscriber::Subscriber, Command, *};
use crate::{
    discover_one, speaker::AV_TRANSPORT, speaker::ZONE_GROUP_TOPOLOGY,
    Service, Speaker, SpeakerInfo, Uri, URN,
};
use futures_util::stream::{SelectAll, StreamExt};
use log::{debug, warn};
use std::{time::Duration};
use tokio::{select, sync::mpsc};
use tokio_stream::wrappers::WatchStream;

type CmdReceiver = mpsc::Receiver<Command>;

#[derive(Debug)]
pub(crate) struct SpeakerData {
    pub(crate) speaker: Speaker,
    transport_subscription: Option<Subscriber>,
    pub(crate) transport_data: AVStatus,
}

impl SpeakerData {
    fn new(speaker: Speaker) -> SpeakerData {
        SpeakerData {
            speaker,
            transport_data: Default::default(),
            transport_subscription: Default::default(),
        }
    }
}

#[derive(Debug, Default)]
/// The manager owns the Speakers and keeps track of the topology
/// so it can perform actions using the appropriate coordinating speakers.
pub(super) struct Controller {
    speakerdata: Vec<SpeakerData>,
    topology: ReducedTopology,
    topology_subscription: Subscriber,
    queued_event_handles: Vec<EventReceiver>,
    rx: Option<CmdReceiver>,
}

impl Controller {
    /// Get a controller.
    pub fn new() -> Controller {
        Controller::default()
    }

    /// Initialize the controller
    ///     * Discover speakers and topology
    ///     * Return Sender for sending commands
    pub async fn init(&mut self) -> Result<CmdSender> {
        self.discover_system().await?;
        let (tx, rx) = mpsc::channel(32);
        self.rx = Some(rx);
        Ok(tx)
    }

    async fn discover_system(&mut self) -> Result<()> {
        let speaker = discover_one(Duration::from_secs(5)).await?;
        self.update_from_topology(speaker._zone_group_state().await?.into_iter().collect())
            .await
            .unwrap_or_else(|err| warn!("Error updating system topology: {:?}", err));
        Ok(())
    }

    /// Get a reference to the vector of speakers.
    pub fn speakers(&self) -> Vec<&Speaker> {
        self.speakerdata.iter().map(|sd| &sd.speaker).collect()
    }

    /// Add a random speaker that doesn't really exist
    #[cfg(debug_assertions)]
    pub fn _add_speaker(&mut self) {
        let mut new_speaker = self.speakerdata[0].speaker.clone();
        new_speaker.info.uuid = "blahblah".into();
        self.speakerdata.push(SpeakerData::new(new_speaker));
    }

    /// Drop a speaker for no good reason
    #[cfg(debug_assertions)]
    pub fn _drop_speaker(&mut self) {
        self.speakerdata.pop().unwrap();
    }

    /// Update speakers and topology
    async fn update_from_topology(&mut self, system_topology: Topology) -> Result<()> {
        let topology: ReducedTopology = system_topology
            .iter()
            .map(|(uuid, infos)| {
                (
                    uuid.to_owned(),
                    infos.iter().map(|info| info.uuid().to_owned()).collect(),
                )
            })
            .collect();
        let infos: Vec<SpeakerInfo> = system_topology
            .into_iter()
            .flat_map(|(_, infos)| infos)
            .collect();

        // Drop speakers and subscriptions that are no longer in the topology
        // Todo: (speakers, av_transport_data, subscription) should probably be
        // a single tuple. Seems like we search them all together alot
        self.speakerdata.retain(|sd| {
            infos
                .iter()
                .any(|info| info.uuid().eq_ignore_ascii_case(sd.speaker.uuid()))
        });

        // Check if we have any new speakers in the system and add them. Update speaker info otherwise
        for info in infos.into_iter() {
            if let Some(speakerdata) = self
                .speakerdata
                .iter_mut()
                .find(|sd| sd.speaker.uuid().eq_ignore_ascii_case(info.uuid()))
            {
                speakerdata.speaker.info = info;
            } else {
                let new_speaker = Speaker::from_speaker_info(&info)
                    .await?
                    .ok_or(crate::Error::SpeakerNotIncludedInOwnZoneGroupState)?;

                // Subscribe to AV Transport events on new speakers
                let mut new_speakerdata = SpeakerData::new(new_speaker);
                if let Some((device_sub, rx)) = self
                    .get_av_transport_subscription(&new_speakerdata.speaker)
                    .await
                {
                    new_speakerdata.transport_subscription = Some(device_sub);
                    self.queued_event_handles.push(rx);
                }
                debug!("Adding UUID: {}", info.uuid());
                self.speakerdata.push(new_speakerdata);
            }
        }

        self.topology = topology;
        Ok(())
    }

    async fn get_av_transport_subscription(
        &mut self,
        new_speaker: &Speaker,
    ) -> Option<(Subscriber, EventReceiver)> {
        let mut device_sub = Subscriber::new();
        if let Some(service) = new_speaker.device.find_service(AV_TRANSPORT) {
            if let Ok(rx) = device_sub.subscribe(
                service.clone(),
                new_speaker.device.url().clone(),
                Some(new_speaker.uuid().to_owned()),
            ) {
                return Some((device_sub, rx));
            }
        }
        None
    }

    fn get_a_service_and_url(&self, urn: &URN) -> Result<(Service, Uri)> {
        let speaker;
        if !self.speakerdata.is_empty() {
            // Chose a random speaker. We may have lost subscription to topology
            // because the last speaker went offline.. and we don't know.
            // There's a chance we can recover quickly if we find an extant speaker.
            let i = fastrand::usize(..self.speakerdata.len());
            speaker = &self.speakerdata.get(i).unwrap().speaker;
        } else {
            return Err(Sonor(crate::Error::NoSpeakersDetected));
        }

        speaker
            .device
            .find_service(urn)
            .ok_or(crate::Error::MissingServiceForUPnPAction {
                service: urn.clone(),
                action: String::new(),
                payload: String::new(),
            })
            .map_err(Sonor)
            .map(|service| (service.clone(), speaker.device.url().clone()))
    }

    /// Handle events. Deal with errors here. Only return an error if it is
    /// unrecoverable and should break the non-event loop, e.g. all speakers
    /// offline.
    async fn handle_event(&mut self, event: Event) -> Result<()> {
        use Event::*;
        match event {
            TopoUpdate(_uuid, topology) => {
                debug!(
                    "Got topology update: {}",
                    topology
                        .iter()
                        .map(|(u, s)| format!(
                            "{} => {:?}, ",
                            self.get_speaker_by_uuid(u)
                                .map(|s| s.name())
                                .unwrap_or_default(),
                            s.iter().map(|i| i.name()).collect::<Vec<&str>>()
                        ))
                        .collect::<String>()
                );
                self.update_from_topology(topology)
                    .await
                    .unwrap_or_else(|err| warn!("Error updating system topology: {:?}", err))
            }
            AVTransUpdate(uuid, data) => {
                // let keys = ["CurrentPlayMode", "CurrentTrack", "CurrentCrossfadeMode", "AVTransportURI"];
                debug!(
                    "Got AVTransUpdate for {} (coord: {})",
                    self.get_speaker_by_uuid(uuid.as_ref().unwrap())
                        .map(|s| s.info.name())
                        .unwrap_or_default(),
                    self.get_coordinator_for_uuid(uuid.as_ref().unwrap())
                        .map(|s| s.info.name())
                        .unwrap_or_default()
                );
                // debug!("... {:?}", data.iter().filter(|(s, d)| keys.contains(&s.as_str())).collect::<Vec<&(String,String)>>());
                if let Some(uuid) = uuid {
                    self.update_avtransport_data(uuid, data)
                } else {
                    warn!("Missing UUID for AV Transport update")
                }
            }
            // Todo: forward updates to subscribers. Zone updates should always
            // come from Controller. Non-contorllers send updates but don't
            // know what the4y are playing
            SubscribeError(uuid, urn) => {
                debug!(
                    "Subscription {} on {} lost",
                    urn,
                    uuid.as_deref().unwrap_or("unknown")
                );
                match &urn {
                    ZONE_GROUP_TOPOLOGY => {
                        // The speaker we were getting updates from may have gone offline. Try another
                        let (service, url) = self.get_a_service_and_url(ZONE_GROUP_TOPOLOGY)?;
                        self.topology_subscription = Subscriber::new();
                        match self.topology_subscription.subscribe(service, url, None) {
                            Ok(rx) => self.queued_event_handles.push(rx),
                            Err(err) => {
                                log::warn!(
                                    "Having trouble subscribing to topology updates: {}",
                                    err
                                );
                                log::warn!("  ...attempting to rediscover system");
                                self.discover_system().await.map(|_| ())?;
                                log::warn!("  ...success!");
                            }
                        }
                    }
                    AV_TRANSPORT => {
                        // The speaker we are subscribing to may have gone
                        // offline or gotten a new IP. In case its the later,
                        // the SpeakerInfo and Device could be out of sync
                        let uuid = &uuid.unwrap();
                        if let Some(mut speakerdata) = self.pop_speakerdata_by_uuid(uuid) {
                            if let Ok(Some(speaker)) =
                                Speaker::from_speaker_info(&speakerdata.speaker.info).await
                            {
                                // The speaker still exists! Resubscribe
                                log::debug!(
                                    "Recreating speaker {}. Did it's IP change?",
                                    speaker.info.name
                                );
                                match self.get_av_transport_subscription(&speaker).await {
                                    Some((sub, rx)) => {
                                        speakerdata.transport_subscription = Some(sub);
                                        self.queued_event_handles.push(rx);
                                    }
                                    None => speakerdata.transport_subscription = None,
                                }
                            }
                            // Put the speakerdata back. If speaker is gone, next topo update will clean it up
                            self.speakerdata.push(speakerdata);
                        }
                    }
                    _ => (),
                }
            }
            NoOp => (),
        };
        Ok(())
    }

    /// Handle zone actions. Deal with errors here. Only return an error if it
    /// is unrecoverable and should break the non-event loop.
    async fn handle_zone_action(
        &self,
        tx: Responder,
        name: String,
        action: ZoneAction,
    ) -> Result<()> {

        debug!("Got {:?}", action);
        action.handle_action(&self, tx, name).await
    }

    /// Run the event loop.
    ///
    ///     * Subscribe and listen to events on the sonos system
    ///     * Keep system state up-to-date 
    ///     * Listen for commands from clients to perform actions on zones.
    ///
    /// Will return an error if system goes offline.
    ///
    /// Whether this function returns an error or not, the reciever will drop
    /// and the controller will need to be re-initialized.

    pub async fn run(&mut self) -> Result<()> {
        use Command::*;

        let mut event_stream = SelectAll::new();
        // Subscribe for topology updates. Any device will do.
        let (service, url) = self.get_a_service_and_url(ZONE_GROUP_TOPOLOGY)?;
        let topo_rx = self.topology_subscription.subscribe(service, url, None)?;
        event_stream.push(WatchStream::new(topo_rx));

        let mut rx = self.rx.take().ok_or(ControllerNotInitialized)?;

        debug!("Listening for commands");
        loop {
            event_stream.extend(self.queued_event_handles.drain(..).map(WatchStream::new));
            select! {
                maybe_command = rx.recv() => match maybe_command {
                    Some(cmd) => match cmd {
                        DoZoneAction(tx, name, action) => self.handle_zone_action(tx, name, action).await?,
                    },
                    None => break
                },
                maybe_event = event_stream.next() => match maybe_event {
                    Some(event) => self.handle_event(event).await?,
                    None => warn!("No active subscriptions... all devices unreachable?"),
                }
            }
        }
        // put reciever back if we exit gracefully? 
        // self.rx = Some(rx);
        debug!("aborting");
        // self.topology_subscription.shutdown().await
        Ok(())
    }

    fn get_speaker_with_name(&self, name: &str) -> Option<&Speaker> {
        self.speakerdata.iter().find_map(|s| {
            match s.speaker.info.name().eq_ignore_ascii_case(name) {
                true => Some(&s.speaker),
                false => None,
            }
        })
    }

    fn get_speaker_by_uuid(&self, uuid: &str) -> Option<&Speaker> {
        self.speakerdata
            .iter()
            .find_map(|s| match s.speaker.info.uuid().eq_ignore_ascii_case(uuid) {
                true => Some(&s.speaker),
                false => None,
            })
    }

    fn get_speakerdata_by_uuid(&self, uuid: &str) -> Option<&SpeakerData> {
        self.speakerdata
            .iter()
            .find(|s| s.speaker.info.uuid().eq_ignore_ascii_case(uuid))
    }

    fn pop_speakerdata_by_uuid(&mut self, uuid: &str) -> Option<SpeakerData> {
        self.speakerdata
            .iter()
            .position(|s| s.speaker.info.uuid().eq_ignore_ascii_case(uuid))
            .map(|idx| self.speakerdata.swap_remove(idx))
    }

    fn get_coordinator_for_name(&self, name: &str) -> Option<&Speaker> {
        let speaker = self.get_speaker_with_name(name)?;
        self.get_coordinator_for_uuid(speaker.uuid())
    }

    fn get_coordinatordata_for_name(&self, name: &str) -> Option<&SpeakerData> {
        let speaker = self.get_speaker_with_name(name)?;
        self.get_coordinatordata_for_uuid(speaker.uuid())
    }

    fn get_coordinator_for_uuid(&self, speaker_uuid: &str) -> Option<&Speaker> {
        let coordinator_uuid = self.topology.iter().find_map(|(coordinator_uuid, uuids)| {
            uuids
                .iter()
                .find(|&uuid| uuid.eq_ignore_ascii_case(speaker_uuid))
                .and(Some(coordinator_uuid))
        })?;
        self.get_speaker_by_uuid(coordinator_uuid)
    }

    fn get_coordinatordata_for_uuid(&self, speaker_uuid: &str) -> Option<&SpeakerData> {
        let coordinator_uuid = self.topology.iter().find_map(|(coordinator_uuid, uuids)| {
            uuids
                .iter()
                .find(|&uuid| uuid.eq_ignore_ascii_case(speaker_uuid))
                .and(Some(coordinator_uuid))
        })?;
        self.get_speakerdata_by_uuid(coordinator_uuid)
    }

    fn update_avtransport_data(&mut self, uuid: Uuid, data: Vec<(String, String)>) {
        match self.speakerdata.iter_mut().find(|sd| sd.speaker.uuid().eq_ignore_ascii_case(&uuid)) {
            Some(sd) => sd.transport_data = data,
            None => warn!("Received AV Transport data for non-existant speaker {}", uuid),
        };
    }
}
