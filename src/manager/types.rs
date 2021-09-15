use tokio::sync::{mpsc, oneshot};

use crate::{Snapshot, SpeakerInfo, URN};

use super::Error;

#[derive(Debug)]
pub(super) enum ZoneAction {
    Exists,
    PlayNow { uri: String, metadata: String },
    Pause,
    TakeSnapshot,
    ApplySnapshot(Snapshot),
}

#[derive(Debug)]
pub(super) enum Command {
    DoZoneAction(Responder, ZoneName, ZoneAction),
}

#[derive(Debug)]
pub enum Response {
    Ok,
    NotOk,
    Snapshot(Snapshot),
}

#[derive(Debug, Clone)]
pub(super) enum Event {
    TopoUpdate(Option<Uuid>, Topology),
    AVTransUpdate(Option<Uuid>, AVStatus),
    SubscribeError(Option<Uuid>, URN),
    NoOp,
}

pub(crate) type Uuid = String;
pub(super) type CmdSender = mpsc::Sender<Command>;
pub(super) type EventReceiver = tokio::sync::watch::Receiver<Event>;

pub(super) type ReducedTopology = Vec<(Uuid, Vec<Uuid>)>;
pub(super) type Topology = Vec<(Uuid, Vec<SpeakerInfo>)>;
pub(super) type AVStatus = Vec<(String, String)>;
pub(super) type Result<T, E = Error> = std::result::Result<T, E>;

/// Type for zone name
pub type ZoneName = String;

/// Type for response channel
pub type Responder = oneshot::Sender<Response>;