#![allow(missing_docs)]

//! A user-friendly API for controlling sonos systems similar to the
//! controller app, with room-by-room (or group-by-group) controls.

use crate::{
    discover, speaker::extract_zone_topology, speaker::ZONE_GROUP_TOPOLOGY, Error, Result, Service,
    Speaker, SpeakerInfo, Uri, URN,
};
use futures_util::{
    future::Future,
    stream::{StreamExt, TryStreamExt},
};
use std::{fmt, time::Duration};
use tokio::{
    sync::{mpsc, oneshot},
    time,
};
use Command::*;

const TIMEOUT_SEC: u32 = 300;
const RENEW_SEC: u32 = 60;

#[derive(Debug)]
pub enum Command {
    TopologyEvent(Vec<(String, Vec<SpeakerInfo>)>),
    PlayInRoom(String),
    SubscriptionLost(Error),
    Break,
}

#[derive(Default)]
struct Subscriber {
    service: Option<rupnp::Service>,
    url: Option<http::Uri>,
    sid: Option<String>,
    abort_listener: Option<Box<dyn FnOnce() -> () + Send>>,
    abort_timer: Option<Box<dyn FnOnce() -> () + Send>>,
    tx: Option<mpsc::Sender<Command>>,
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

#[derive(Debug, Default)]
/// The manager owns the Speakers and keeps track of the topology
/// so it can perform actions using the appropriate coordinating speakers.
pub struct Controller {
    pub(crate) speakers: Vec<Speaker>,
    pub(crate) topology: Vec<(String, Vec<SpeakerInfo>)>,
    subscriber: Subscriber,
}

impl Controller {
    /// Get a manager.
    pub async fn new() -> Result<Controller> {
        let speakers = Self::discover_speakers().await?;

        let topology = match speakers.get(0) {
            Some(speaker) => speaker._zone_group_state().await?,
            None => Vec::new(),
        };

        // Set-up to listen for topology changes
        Ok(Controller {
            speakers,
            topology,
            ..Controller::default()
        })
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
            println!("Adding UUID: {}", uuid);
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
        println!("Toplogy: {:?}", topology);
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

    /// Run the event loop
    pub async fn run(
        &mut self,
        tx: mpsc::Sender<Command>,
        mut rx: mpsc::Receiver<Command>,
    ) -> Result<(), Error> {
        let (service, url) = self.get_a_service_and_url(&ZONE_GROUP_TOPOLOGY)?;
        self.subscriber.subscribe(service, url, tx.clone()).await?;

        println!("Listening for commands");
        while let Some(cmd) = rx.recv().await {
            match cmd {
                TopologyEvent(topology) => {
                    println!("Received topology");
                    if let Err(err) = self.update_from_topology(topology).await {
                        println!("Topo: {:?}", err);
                    }
                }
                SubscriptionLost(err) => {
                    println!("Lost subscription: {}", err);
                    // Let's try to setup a new subscription.
                    let (service, url) = self.get_a_service_and_url(&ZONE_GROUP_TOPOLOGY)?;
                    self.subscriber.shutdown().await?;
                    self.subscriber.subscribe(service, url, tx.clone()).await?;
                }
                Break => {
                    rx.close(); // Process any commands already sent
                                // self.subscriber.terminate_tasks()?;
                }
                _ => (),
            }
        }
        // if we made it this far, abort
        println!("aborting");
        self.subscriber.shutdown().await
    }
}

impl Subscriber {
    async fn subscribe(
        &mut self,
        service: rupnp::Service,
        url: http::Uri,
        tx: mpsc::Sender<Command>,
    ) -> Result<()> {
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
                    Ok(topology) => tx.send(TopologyEvent(topology)).await.unwrap(),
                    _ => continue,
                }
            }
        };
        self.abort_listener = Some(self.spawn_interruptable(listen_task));

        let tx = self.tx.as_ref().unwrap().clone();
        let timer_task = async move {
            time::sleep(Duration::from_millis((RENEW_SEC * 1000).into())).await;
            loop {
                if let Err(err) = service.renew_subscription(&url, &sid, TIMEOUT_SEC).await {
                    tx.send(SubscriptionLost(Error::UPnP(err))).await.unwrap();
                }
                println!("Renewed subscription");
                time::sleep(Duration::from_millis((RENEW_SEC * 1000).into())).await;
            }
        };
        self.abort_timer = Some(self.spawn_interruptable(timer_task));

        Ok(())
    }

    fn spawn_interruptable<C>(&self, task: C) -> Box<dyn FnOnce() -> () + Send>
    where
        C: Future + Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        tokio::spawn(async move {
            tokio::select! {
                _ = rx => println!("Shutting down task"),
                _ = task => ()
            }
        });
        Box::new(|| tx.send(()).unwrap())
    }

    /// This may return an error 412 when the subscription has already lapsed.
    async fn unsubscribe(&mut self) -> Result<()> {
        use Error::SubscriberError;
        println!("Unsubscribing");

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
