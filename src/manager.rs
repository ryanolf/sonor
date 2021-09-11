#![allow(missing_docs)]

//! A user-friendly API for controlling sonos systems similar to the
//! controller app, with room-by-room (or group-by-group) controls.
use tokio::{sync::oneshot, task::JoinHandle};

use crate::{Error, Result, Snapshot, controller::{Command::*, Controller, ControllerSender, Response}, metadata::guess_uri_and_metadata};

#[derive(Default, Debug)]
pub struct Manager {
    controller_handle: Option<JoinHandle<Controller>>,
    tx: Option<ControllerSender>,
}

#[derive(Debug)]
pub enum ZoneAction {
    Exists,
    PlayNow { uri: String, metadata: String },
    Pause,
    TakeSnapshot,
    ApplySnapshot(Snapshot)
}
use ZoneAction::*;

#[derive(Debug)]
pub struct Zone<'a> {
    manager: &'a Manager,
    name: String
}

impl<'a> Zone<'a> {
    async fn action(&self, action: ZoneAction) -> Result<Response> {
        let (tx, rx) = oneshot::channel();
        self.manager
            .tx
            .as_ref()
            .ok_or(Error::ControllerNotInitialized)?
            .send(DoZoneAction(tx, self.name.clone(), action))
            .await
            .map_err(Error::MessageSendError)?;
        rx.await.map_err(Error::MessageRecvError)
    }

    pub async fn play_now(&self, uri: &str) -> Result<()> {
        let (uri, metadata) = guess_uri_and_metadata(uri);
        match self.action(PlayNow { uri, metadata }).await? {
            Response::Ok => Ok(()),
            _ => Err(Error::ZoneDoesNotExist)
        }
    }
    pub async fn pause(&self) -> Result<()> {
        match self.action(Pause).await? {
            Response::Ok => Ok(()),
            _ => Err(Error::ZoneDoesNotExist)
        }
    }
    pub async fn take_snapshot(&self) -> Result<Snapshot> {
        match self.action(TakeSnapshot).await? {
            Response::Snapshot(snapshot) => Ok(snapshot),
            _ => Err(Error::ZoneDoesNotExist)
        }
    }
    pub async fn apply_snapshot(&self, snapshot: Snapshot) -> Result<()> {
        match self.action(ApplySnapshot(snapshot)).await? {
            Response::Ok => Ok(()),
            _ => Err(Error::ZoneDoesNotExist)
        }
    }
}

impl Manager {
    pub async fn new() -> Result<Manager> {
        let mut controller = Controller::new();

        let tx = Some(controller.init().await?);

        let controller_handle = Some(tokio::spawn(async move {
            match controller.run().await {
                Err(e) => log::error!("Controller says: {}", e),
                _ => (),
            };
            log::debug!("Controller terminated?");
            controller
        }));

        Ok(Manager {
            controller_handle,
            tx,
        })
    }

    pub async fn get_zone(&self, zone_name: &str) -> Result<Zone<'_>> {
        let zone = Zone{ manager: self, name: zone_name.to_string() };
        match zone.action(Exists).await? {
            Response::Ok => Ok(zone),
            _ => Err(Error::ZoneDoesNotExist)
        }
    }
}
