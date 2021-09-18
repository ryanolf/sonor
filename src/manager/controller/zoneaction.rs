use crate::{Snapshot, manager::Responder};
use super::{controller::Controller, mediasource::MediaSource, Result, Response};

#[derive(Debug)]
pub enum ZoneAction {
    Exists,
    PlayNow(MediaSource),
    QueueAsNext(MediaSource),
    Pause,
    NextTrack,
    PreviousTrack,
    SeekTime(u32),
    SetRepeat(crate::RepeatMode),
    SetShuffle(bool),
    TakeSnapshot,
    ClearQueue,
    ApplySnapshot(Snapshot),
    GetQueue
}
use ZoneAction::*;
// macro_rules! action {
//     ($data:ident.$method:ident($payload:ident: $self:ident.$letmethod:ident(&$name:ident)) -> $tx:ident: $res:ident$(($returnval:ident))? ) => {
//         {  
//             if let Some($payload) = $self.$letmethod(&$name) {
//                 log::debug!("Attempting to do {:?} with {:?} in {:?}", stringify!($method), $data, $name);
//                 match $data.$method($payload).await {
//                     Ok($($returnval)?_) => return $tx.send(Response::$res$(($returnval))?).or_else(|_| Ok(())),
//                     Err(e) => log::warn!("Error: {}", e)
//                 }
//             }
//             $tx.send(Response::NotOk).ok();
//         }
//     };
//     ($payload:ident.$method:ident: $self:ident.$letmethod:ident(&$name:ident) -> $tx:ident: $res:ident$(($returnval:ident))? ) => {
//         {  
//             if let Some($payload) = $self.$letmethod(&$name) {
//                 log::debug!("Attempting to do {:#?} in {}", stringify!($method), $name);
//                 match $payload.$method().await {
//                     Ok($($returnval)?_) => return $tx.send(Response::$res$(($returnval))?).or_else(|_| Ok(())),
//                     Err(e) => log::warn!("Error: {}", e)
//                 }
//             }
//             $tx.send(Response::NotOk).ok();
//         }
//     };
// }

impl ZoneAction {
    pub(super) async fn handle_action(&self, controller: &Controller, tx: Responder, name: String) -> Result<()> {
        macro_rules! action {
            ($data:ident.$method:ident($payload:ident: $letmethod:ident) -> $res:ident($returnval:ident) ) => {
                {  
                    if let Some($payload) = controller.$letmethod(&name) {
                        log::debug!("Attempting to do {:?} with {:?} in {:?}", stringify!($method), $data, name);
                        match $data.$method($payload).await {
                            Ok($returnval) => return tx.send(Response::$res($returnval)).or_else(|_| Ok(())),
                            Err(e) => log::warn!("Error: {}", e)
                        }
                    }
                    tx.send(Response::NotOk).ok();
                }
            };
            ($payload:ident.$method:ident: $letmethod:ident -> $res:ident($returnval:ident) ) => {
                {  
                    if let Some($payload) = controller.$letmethod(&name) {
                        log::debug!("Attempting to do {:#?} in {}", stringify!($method), name);
                        match $payload.$method().await {
                            Ok($returnval) => return tx.send(Response::$res($returnval)).or_else(|_| Ok(())),
                            Err(e) => log::warn!("Error: {}", e)
                        }
                    }
                    tx.send(Response::NotOk).ok();
                }
            };
        }

        match self {
            PlayNow(media) => action!( media.play_now(coordinatordata: get_coordinatordata_for_name) -> Ok(__) ),
            QueueAsNext(media) => action!( media.queue_as_next(coordinatordata: get_coordinatordata_for_name) -> Ok(__) ),
            ClearQueue => action!( coordinator.clear_queue: get_coordinator_for_name -> Ok(__) ),
            Exists => {
                if controller.speakerdata.iter().any(|s| s.speaker.info.name == name) {
                    tx.send(Response::Ok(())).unwrap_or(());
                } else {
                    tx.send(Response::NotOk).unwrap_or(());
                }
            }
            Pause => action!( coordinator.pause: get_coordinator_for_name -> Ok(__) ),
            TakeSnapshot => action!( coordinator.snapshot: get_coordinator_for_name -> Snapshot(snapshot) ),
            ApplySnapshot(snapshot) => action!( snapshot.apply(coordinator: get_coordinator_for_name) -> Ok(__) ),
            GetQueue => action!( coordinator.queue: get_coordinator_for_name -> Queue(queue) ),
            NextTrack => action!( coordinator.next: get_coordinator_for_name -> Ok(__) ),
            PreviousTrack => action!( coordinator.previous: get_coordinator_for_name -> Ok(__) ),
            SeekTime(_) => todo!(),
            SetRepeat(_) => todo!(),
            SetShuffle(_) => todo!(),
        }

        Ok(())
    }
}