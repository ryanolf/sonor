#![allow(missing_docs)]

//! API backend for tracking sonos system topology

use crate::{Error, Result, Service, Speaker, SpeakerInfo, Snapshot, URN, Uri, discover, manager::ZoneAction, speaker::ZONE_GROUP_TOPOLOGY, speaker::extract_zone_topology};
use futures_util::{
    future::Future,
    stream::{StreamExt, TryStreamExt},
};
use log::debug;
use std::{fmt, time::Duration};
use tokio::{
    sync::{mpsc, oneshot},
    time,
};
use Command::*;

const TIMEOUT_SEC: u32 = 300;
const RENEW_SEC: u32 = 60;

#[derive(Debug)]
pub enum Event {
    TopologyUpdate(Vec<(String, Vec<SpeakerInfo>)>),
    ReSubscribe,
    ListenerDown,
}

type ZoneName = String;
type Responder = oneshot::Sender<Response>;

#[derive(Debug)]
pub enum Command {
    Emit(Event),
    Break,
    DoZoneAction(Responder, ZoneName, ZoneAction)
}

#[derive(Debug)]
pub enum Response {
    Ok,
    NotOk,
    Snapshot(Snapshot)
}

pub type ControllerSender = mpsc::Sender<Command>;
pub type ControllerReceiver = mpsc::Receiver<Command>;
type AbortCallback = Box<dyn FnOnce() -> () + Send + Sync>;

#[derive(Default)]
struct Subscriber {
    service: Option<rupnp::Service>,
    url: Option<http::Uri>,
    sid: Option<String>,
    abort_listener: Option<AbortCallback>,
    abort_timer: Option<AbortCallback>,
    tx: Option<ControllerSender>,
}

#[derive(Debug)]
/// The manager owns the Speakers and keeps track of the topology
/// so it can perform actions using the appropriate coordinating speakers.
pub struct Controller {
    pub(crate) speakers: Vec<Speaker>,
    pub(crate) topology: Vec<(String, Vec<SpeakerInfo>)>,
    subscriber: Subscriber,
    tx_rx: (ControllerSender, ControllerReceiver),
}

impl Default for Controller {
    fn default() -> Self {
        Self {
            speakers: Default::default(),
            topology: Default::default(),
            subscriber: Default::default(),
            tx_rx: mpsc::channel(32),
        }
    }
}

impl Controller {
    /// Get a manager.
    pub fn new() -> Controller {
        Controller::default()
    }

    pub async fn init(&mut self) -> Result<ControllerSender> {
        self.speakers = Self::discover_speakers().await?;

        match self.speakers.get(0) {
            Some(speaker) => self.topology = speaker._zone_group_state().await?,
            None => (),
        };

        Ok(self.tx_rx.0.clone())
    }

    async fn discover_speakers() -> Result<Vec<Speaker>> {
        discover(Duration::from_secs(5))
            .await?
            .try_collect::<Vec<Speaker>>()
            .await
    }

    /// Get the speakers.
    pub fn speakers(&self) -> &Vec<Speaker> {
        &self.speakers
    }

    /// Blah
    #[cfg(debug_assertions)]
    pub fn add_speaker(&mut self) {
        let mut new_speaker = self.speakers[0].clone();
        new_speaker.info.uuid = "blahblah".into();
        self.speakers.push(new_speaker);
    }

    /// Blah
    #[cfg(debug_assertions)]
    pub fn drop_speaker(&mut self) {
        self.speakers.pop().unwrap();
    }

    /// Get the topology.
    pub fn topology(&self) -> &Vec<(String, Vec<SpeakerInfo>)> {
        &self.topology
    }

    // Update speakers and topology
    // TODO: this should update the speaker info in case of location or name change
    async fn update_from_topology(
        &mut self,
        topology: Vec<(String, Vec<SpeakerInfo>)>,
    ) -> Result<()> {

        let mut new_uuids: Vec<&str> = topology
            .iter()
            .flat_map(|(_, infos)| infos)
            .map(SpeakerInfo::uuid)
            .collect();

        // Drop speakers that are no longer in the topology
        self.speakers
            .retain(|speaker| new_uuids.contains(&speaker.uuid()));

        let current_uuids: Vec<&str> = self.speakers.iter().map(|speaker| speaker.uuid()).collect();

        // Check if we have any new speakers
        new_uuids.retain(|uuid| !current_uuids.contains(&uuid));

        for uuid in new_uuids.into_iter() {
            debug!("Adding UUID: {}", uuid);
            let info: &SpeakerInfo = &topology
                .iter()
                .flat_map(|(_, infos)| infos)
                .find(|info| info.uuid == uuid)
                .unwrap();
            let new_speaker = Speaker::from_speaker_info(info)
                .await?
                .ok_or(Error::GetZoneGroupStateReturnedNonSonos)
                .unwrap();
            self.speakers.push(new_speaker);
        }
        debug!("Toplogy: {:?}", topology);
        self.topology = topology;
        Ok(())
    }

    fn get_a_service_and_url(&self, urn: &URN) -> Result<(Service, Uri)> {
        let speaker;
        if self.speakers.len() > 0 {
            // Chose a random speaker. We may have lost subscription to topology
            // because the last speaker went offline.. and we don't know.
            // There's a chance we can recover if we find an extant speaker.
            let i = fastrand::usize(..self.speakers.len());
            speaker = self.speakers.get(i).unwrap();
        } else {
            return Err(Error::NoSpeakersDetected);
        }

        speaker
            .device
            .find_service(urn)
            .ok_or(Error::MissingServiceForUPnPAction {
                service: urn.clone(),
                action: String::new(),
                payload: String::new(),
            })
            .map(|service| (service.clone(), speaker.device.url().clone()))
    }

    async fn handle_event(&mut self, event: Event) -> Result<()> {
        use Event::*;
        match event {
            TopologyUpdate(topology) => {
                debug!("Received topology");
                if let Err(err) = self.update_from_topology(topology).await {
                    debug!("Topo: {:?}", err);
                }
            }
            ReSubscribe | ListenerDown => {
                debug!("Recreating subscriber");
                // Let's try to setup a new subscription.
                let (service, url) = self.get_a_service_and_url(&ZONE_GROUP_TOPOLOGY)?;
                self.subscriber.shutdown().await?;
                self.subscriber.subscribe(service, url, self.tx_rx.0.clone()).await?;
            }
        }
        Ok(())
    }

    async fn handle_zone_action(&self, tx: Responder, name: String, action: ZoneAction) -> Result<()> {
        use ZoneAction::*;
        match action {
            // TODO: use cached state to find track number
            PlayNow { uri, metadata } => {
                if let Some(coordinator) = self.get_coordinator_for_name(&name) {
                    log::debug!("Attempting to set {} transport to {}", name, uri);
                    if let Ok(_) = coordinator
                        .set_transport_uri(&uri, &metadata)
                        .await
                        .and(coordinator.play().await) {
                        tx.send(Response::Ok).unwrap_or(());
                        return Ok(())
                    }
                }
                tx.send(Response::NotOk).unwrap_or(())
                
                // let track_no = match coordinator.track().await? {
                //     Some(TrackInfo{track_no, ..}) => track_no,
                //     _ => 0 // Not sure about this
                // };
            },

            Exists => {
                if self.speakers.iter().any(|s| s.info.name == name) {
                    tx.send(Response::Ok).unwrap_or(());
                } else {
                    tx.send(Response::NotOk).unwrap_or(());
                }
            },

            Pause => {
                if let Some(coordinator) = self.get_coordinator_for_name(&name) {
                    log::debug!("Attempting to pause on {}", name);
                    match coordinator.pause().await {
                        Ok(_) => tx.send(Response::Ok).unwrap_or(()),
                        _ => tx.send(Response::NotOk).unwrap_or(())
                    }
                }
            },

            TakeSnapshot => {
                if let Some(coordinator) = self.get_coordinator_for_name(&name) {
                    log::debug!("Attempting to take a snapshot on {}", name);
                    match coordinator.snapshot().await {
                        Ok(snapshot) => tx.send(Response::Snapshot(snapshot)).unwrap_or(()),
                        _ => tx.send(Response::NotOk).unwrap_or(())
                    }
                }
            },

            ApplySnapshot(snapshot) => {
                if let Some(coordinator) = self.get_coordinator_for_name(&name) {
                    log::debug!("Attempting to apply a snapshot on {}", name);
                    match coordinator.apply(snapshot).await {
                        Ok(_) => tx.send(Response::Ok).unwrap_or(()),
                        _ => tx.send(Response::NotOk).unwrap_or(())
                    };
                }
            },
        }
        Ok(())
    }

    /// Run the event loop
    pub async fn run(&mut self) -> Result<()> {
        use Command::*;
        let (service, url) = self.get_a_service_and_url(&ZONE_GROUP_TOPOLOGY)?;
        self.subscriber.subscribe(service, url, self.tx_rx.0.clone()).await?;

        debug!("Listening for commands");
        while let Some(cmd) = self.tx_rx.1.recv().await {
            match cmd {
                Emit(event) => self.handle_event(event).await?,
                Break => self.tx_rx.1.close(), // Process any commands already sent
                DoZoneAction(tx, name, action) => self.handle_zone_action(tx, name, action).await?,
            };
        }
        debug!("aborting");
        self.subscriber.shutdown().await
    }

    fn get_speaker_with_name(&self, name: &str) -> Option<&Speaker> {
        self.speakers.iter().find(|s| s.info.name().eq_ignore_ascii_case(name))
    }

    fn get_speaker_by_uuid(&self, uuid: &str) -> Option<&Speaker> {
        self.speakers
            .iter()
            .find(|s| s.info.uuid().eq_ignore_ascii_case(uuid))
    }

    fn get_coordinator_for_name(&self, name: &str) -> Option<&Speaker> {
        let coordinator_uuid = self.topology
            .iter()
            .find_map(|(coordinator_uuid, infos)| 
                infos.iter()
                    .find(|info| info.name().eq_ignore_ascii_case(name))
                    .and(Some(coordinator_uuid)))?;
        self.get_speaker_by_uuid(coordinator_uuid)
    }
}

impl Subscriber {
    async fn subscribe(
        &mut self,
        service: rupnp::Service,
        url: http::Uri,
        tx: ControllerSender,
    ) -> Result<()> {
        use Event::*;

        let (sid, mut stream) = service.subscribe(&url, TIMEOUT_SEC).await?;
        // Clone all so they can be moved later into async tasks
        self.service = Some(service.clone());
        self.sid = Some(sid.clone());
        self.tx = Some(tx.clone());
        self.url = Some(url.clone());

        let listen_task = async move {
            while let Some(Ok(state_vars)) = stream.next().await {
                let encoded_xml = state_vars.get("ZoneGroupState").unwrap();
                match extract_zone_topology(&encoded_xml) {
                    Ok(topology) => tx.send(Emit(TopologyUpdate(topology))).await.unwrap(),
                    _ => continue,
                }
            }
            tx.send(Emit(ListenerDown)).await.unwrap();
        };
        self.abort_listener = Some(self.spawn_interruptable(listen_task));

        let tx = self.tx.as_ref().unwrap().clone();
        let timer_task = async move {
            time::sleep(Duration::from_millis((RENEW_SEC * 1000).into())).await;
            loop {
                if let Err(err) = service.renew_subscription(&url, &sid, TIMEOUT_SEC).await {
                    log::info!("{}", Error::UPnP(err));
                    tx.send(Emit(ReSubscribe)).await.unwrap();
                }
                debug!("Renewed subscription");
                time::sleep(Duration::from_millis((RENEW_SEC * 1000).into())).await;
            }
        };
        self.abort_timer = Some(self.spawn_interruptable(timer_task));

        Ok(())
    }

    fn spawn_interruptable<C>(&self, task: C) -> AbortCallback
    where
        C: Future + Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        tokio::spawn(async move {
            tokio::select! {
                _ = rx => debug!("Shutting down task"),
                _ = task => ()
            }
        });
        Box::new(|| tx.send(()).unwrap_or(()))
    }

    /// This may return an error 412 when the subscription has already lapsed.
    async fn unsubscribe(&mut self) -> Result<()> {
        use Error::SubscriberError;
        debug!("Unsubscribing");

        self.service
            .take()
            .ok_or(SubscriberError("No service to unsubscribe from".into()))?
            .unsubscribe(
                &self.url.take().ok_or(SubscriberError("No url!".into()))?,
                &self.sid.take().ok_or(SubscriberError("No sid!".into()))?,
            )
            .await
            .map_err(Error::UPnP)
    }

    fn terminate_tasks(&mut self) -> Result<()> {
        let mut res = Ok(());

        // Shut down the listener
        if let Some(abort) = self.abort_listener.take() {
            abort();
        } else {
            res = Err(Error::SubscriberError("No task handle!".into()));
        }

        // Stop the re-subscribe timer
        if let Some(abort) = self.abort_timer.take() {
            abort();
        } else {
            res = res.and(Err(Error::SubscriberError("No timer handle!".into())));
        }
        res
    }

    async fn shutdown(&mut self) -> Result<()> {
        let res = self.terminate_tasks();
        // Unsubscribe. Ignore Http errors
        match self.unsubscribe().await {
            Ok(_) => res,
            Err(Error::UPnP(rupnp::Error::HttpErrorCode(_))) => res,
            Err(e) => res.and(Err(e)),
        }
    }
}

impl Drop for Subscriber {
    fn drop(&mut self) {
        self.terminate_tasks().unwrap_or(())
    }
}

impl fmt::Debug for Subscriber {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Subscriber")
            .field("service", &self.service)
            .field("url", &self.url)
            .field("sid", &self.sid)
            .field("tx", &self.tx)
            .field(
                "abort_timer",
                match &self.abort_timer {
                    Some(_) => &"Present",
                    _ => &"Absent",
                },
            )
            .field(
                "abort_listener",
                match &self.abort_listener {
                    Some(_) => &"Present",
                    _ => &"Absent",
                },
            )
            .finish()
    }
}
