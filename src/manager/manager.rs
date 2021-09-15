#![allow(missing_docs)]
use tokio::{sync::oneshot, task::JoinHandle};

use crate::{Snapshot};
use super::{*, Error::*, controller::Controller, metadata::guess_uri_and_metadata};

#[derive(Default, Debug)]
pub struct Manager {
    controller_handle: Option<JoinHandle<Controller>>,
    tx: Option<CmdSender>,
}


use ZoneAction::*;

#[derive(Debug)]
pub struct Zone<'a> {
    manager: &'a Manager,
    name: String,
}

impl<'a> Zone<'a> {
    async fn action(&self, action: ZoneAction) -> Result<Response> {
        let (tx, rx) = oneshot::channel();
        self.manager
            .tx
            .as_ref()
            .ok_or(ControllerNotInitialized)?
            .send(Command::DoZoneAction(tx, self.name.clone(), action))
            .await
            .map_err(|_| MessageSendError)?;
        rx.await.map_err(|_| MessageRecvError)
    }

    pub async fn play_now(&self, uri: &str) -> Result<()> {
        let (uri, metadata) = guess_uri_and_metadata(uri);
        match self.action(PlayNow { uri, metadata }).await? {
            Response::Ok => Ok(()),
            _ => Err(ZoneDoesNotExist),
        }
    }
    pub async fn pause(&self) -> Result<()> {
        match self.action(Pause).await? {
            Response::Ok => Ok(()),
            _ => Err(ZoneDoesNotExist),
        }
    }
    pub async fn take_snapshot(&self) -> Result<Snapshot> {
        match self.action(TakeSnapshot).await? {
            Response::Snapshot(snapshot) => Ok(snapshot),
            _ => Err(ZoneDoesNotExist),
        }
    }
    pub async fn apply_snapshot(&self, snapshot: Snapshot) -> Result<()> {
        match self.action(ApplySnapshot(snapshot)).await? {
            Response::Ok => Ok(()),
            _ => Err(ZoneDoesNotExist),
        }
    }
}

impl Manager {
    pub async fn new() -> Result<Manager> {
        let mut controller = Controller::new();

        let tx = Some(controller.init().await?);
        log::debug!("Initialized controller with devices:");
        for device in controller.speakers().iter() {
            log::debug!("     - {}", device.name());
        }

        let controller_handle = Some(tokio::spawn(async move {
            if let Err(e) = controller.run().await {
                log::error!("Controller shut down: {}", e)
            };
            log::debug!("Controller terminated on purpose?");
            controller
        }));

        Ok(Manager {
            controller_handle,
            tx,
        })
    }

    pub async fn get_zone(&self, zone_name: &str) -> Result<Zone<'_>> {
        let zone = Zone {
            manager: self,
            name: zone_name.to_string(),
        };
        match zone.action(Exists).await? {
            Response::Ok => Ok(zone),
            _ => Err(ZoneDoesNotExist),
        }
    }
}
