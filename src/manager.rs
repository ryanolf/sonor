#![allow(missing_docs, unused_macros)]

//! A user-friendly API for controlling sonos systems similar to the
//! controller app, with room-by-room (or group-by-group) controls.

mod controller;
mod mediasource;
mod metadata;
mod error;
mod subscriber;
mod types;
use types::{Command, Response};
mod test;

pub use types::*;
pub use error::Error;
pub use mediasource::MediaSource;
pub use controller::ZoneAction;
pub(crate) use controller::SpeakerData;

use self::{Error::*, controller::Controller};
use crate::Snapshot;
use controller::ZoneAction::*;

use tokio::{sync::oneshot, task::JoinHandle};

#[derive(Default, Debug)]
pub struct Manager {
    controller_handle: Option<JoinHandle<Controller>>,
    tx: Option<CmdSender>,
}

#[derive(Debug)]
pub struct Zone<'a> {
    manager: &'a Manager,
    name: String,
}

macro_rules! action {
    ($fn:ident: $action:ident$(($invar:ident: $intyp:ty))? => $resp:ident($outvar:ident: $outtyp:ty)) => {
        pub async fn $fn(&self$(, $invar: $intyp)?) -> Result<$outtyp>{
            match self.action($action$(($invar))?).await? {
                Response::$resp($outvar) => Ok($outvar),
                _ => Err(ZoneActionError)
            }
        }
    };
}

impl<'a> Zone<'a> {
    pub async fn action(&self, action: ZoneAction) -> Result<Response> {
        let (tx, rx) = oneshot::channel();
        self.manager
            .tx
            .as_ref()
            .ok_or(ControllerNotInitialized)?
            .send(Command::DoZoneAction(tx, self.name.clone(), action))
            .await
            .map_err(|_| ControllerOffline)?;
        rx.await.map_err(|_| MessageRecvError)
    }

    action!(play_now: PlayNow(media: MediaSource) => Ok(__: ()));
    action!(queue_as_next: QueueAsNext(media: MediaSource) => Ok(__: ()));
    action!(clear_queue: ClearQueue => Ok(__: ()));
    action!(pause: Pause => Ok(__: ()));
    action!(take_snapshot: TakeSnapshot => Snapshot(snap: Snapshot));
    action!(apply_snapshot: ApplySnapshot(snap: Snapshot) => Ok(__: ()));
    action!(apply_snapshot: ApplySnapshot(snap: Snapshot) => Ok(__: ()));
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
            Response::Ok(_) => Ok(zone),
            _ => Err(ZoneDoesNotExist),
        }
    }
}

impl Drop for Manager {
    // The controller should shut down when we drop the transmitter, but just in case.
    fn drop(&mut self) {
        log::debug!("Dropping manager", );
        self.controller_handle.as_ref().map(JoinHandle::abort);
    }
}
