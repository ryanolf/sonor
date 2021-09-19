use async_trait::async_trait;

use super::Controller;
use crate::{RepeatMode, Snapshot, manager::{MediaSource, Error,  Result, types::{Response, Responder}}};

#[derive(Debug)]
pub enum ZoneAction {
    Exists,
    PlayNow(MediaSource),
    QueueAsNext(MediaSource),
    Pause,
    NextTrack,
    PreviousTrack,
    SeekTime(u32),
    SeekTrack(u32),
    SeekRelTrack(u32),
    SetRepeat(RepeatMode),
    SetShuffle(bool),
    ClearQueue,
    GetQueue,
    TakeSnapshot,
    ApplySnapshot(Snapshot),
}
use ZoneAction::*;

impl ZoneAction {
    pub(super) async fn handle_action(
        self,
        controller: &Controller,
        tx: Responder,
        name: String,
    ) -> Result<()> {
        macro_rules! action {
            ($data:ident.$method:ident($payload:ident: $letmethod:ident) -> $res:ident($returnval:ident) ) => {{
                if let Some($payload) = controller.$letmethod(&name) {
                    log::debug!(
                        "Attempting to {:?} with {:?} in {:?}",
                        stringify!($method),
                        $data,
                        name
                    );
                    match $data.$method($payload).await {
                        Ok($returnval) => {
                            return tx.send(Response::$res($returnval)).or_else(|_| Ok(()))
                        }
                        Err(e) => log::warn!("Error: {}", e),
                    }
                }
                tx.send(Response::NotOk).ok();
            }};
            ($payload:ident.$method:ident: $letmethod:ident -> $res:ident($returnval:ident) ) => {{
                if let Some($payload) = controller.$letmethod(&name) {
                    log::debug!("Attempting to {:#?} in {}", stringify!($method), name);
                    match $payload.$method().await {
                        Ok($returnval) => {
                            return tx.send(Response::$res($returnval)).or_else(|_| Ok(()))
                        }
                        Err(e) => log::warn!("Error: {}", e),
                    }
                }
                tx.send(Response::NotOk).ok();
            }};
        }

        match self {
            PlayNow(media) => {
                action!( media.play_now(coordinatordata: get_coordinatordata_for_name) -> Ok(__) )
            }
            QueueAsNext(media) => {
                action!( media.queue_as_next(coordinatordata: get_coordinatordata_for_name) -> Ok(__) )
            }
            Pause => action!( coordinator.pause: get_coordinator_for_name -> Ok(__) ),
            NextTrack => action!( coordinator.next: get_coordinator_for_name -> Ok(__) ),
            PreviousTrack => action!( coordinator.previous: get_coordinator_for_name -> Ok(__) ),
            SeekTime(seconds) => {
                action!( seconds.skip_to(coordinator: get_coordinator_for_name) -> Ok(__) )
            }
            SeekTrack(number) => {
                action!( number.seek_track(coordinator: get_coordinator_for_name) -> Ok(__) )
            }
            SeekRelTrack(number) => {
                action!( number.seek_rel_track(coordinatordata: get_coordinatordata_for_name) -> Ok(__) )
            }
            SetRepeat(mode) => action!( mode.set(coordinator: get_coordinator_for_name) -> Ok(__) ),
            SetShuffle(state) => action!( state.set_crossfade(coordinator: get_coordinator_for_name) -> Ok(__) ),
            ClearQueue => action!( coordinator.clear_queue: get_coordinator_for_name -> Ok(__) ),
            GetQueue => action!( coordinator.queue: get_coordinator_for_name -> Queue(queue) ),
            ApplySnapshot(snapshot) => {
                action!( snapshot.apply(coordinator: get_coordinator_for_name) -> Ok(__) )
            }
            TakeSnapshot => {
                action!( coordinator.snapshot: get_coordinator_for_name -> Snapshot(snapshot) )
            }
            Exists => {
                if controller
                    .speakerdata
                    .iter()
                    .any(|s| s.speaker.info.name == name)
                {
                    tx.send(Response::Ok(())).unwrap_or(());
                } else {
                    tx.send(Response::NotOk).unwrap_or(());
                }
            }
        }

        Ok(())
    }
}


#[async_trait]
trait ZoneActionBoolExt {
    async fn set_crossfade(self, speaker: &crate::Speaker) -> Result<()>;
}

#[async_trait]
impl ZoneActionBoolExt for bool {
    async fn set_crossfade(self, speaker: &crate::Speaker) -> Result<()> {
        speaker.set_crossfade(self).await.map_err(Error::from)
    }
}

#[async_trait]
trait ZoneActionNumbersExt {
    async fn skip_to(self, speaker: &crate::Speaker) -> Result<()>;
    async fn seek_track(self, speaker: &crate::Speaker) -> Result<()>;
    async fn seek_rel_track(self, speaker: &super::SpeakerData) -> Result<()>;
}

#[async_trait]
impl ZoneActionNumbersExt for u32 {
    async fn skip_to(self, speaker: &crate::Speaker) -> Result<()> {
        speaker.skip_to(self).await.map_err(Error::from)
    }
    async fn seek_track(self, speaker: &crate::Speaker) -> Result<()> {
        speaker.seek_track(self).await.map_err(Error::from)
    }
    async fn seek_rel_track(self, speakerdata: &super::SpeakerData) -> Result<()> {
        let cur_track_no = speakerdata.get_current_track_no().await?;
        let target = cur_track_no + self;
        if target < 1 {
            speakerdata.speaker.seek_track(1).await.map_err(Error::from)
        } else {
        speakerdata.speaker.seek_track(self).await.map_err(Error::from)
        }
    }
}
