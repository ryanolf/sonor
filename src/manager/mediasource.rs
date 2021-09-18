use super::{metadata::*, Error::{self, *}, Result, SpeakerData};
use crate::Speaker;
use xml::escape::escape_str_pcdata;

#[derive(Debug)]
/// Definitions for media that can be played and queued.
pub enum MediaSource {
    Apple(String),
    Spotify(String),
    SonosPlaylist(String),
    SonosFavorite(String),
}

use MediaSource::*;
impl MediaSource {
    async fn get_uri_and_metadata(&self, speaker: &Speaker) -> Option<(String, String)> {
        match self {
            Apple(item) => apple_uri_and_metadata(item),
            Spotify(item) => spotify_uri_and_metadata(item),
            SonosPlaylist(item) => {
                let playlists = speaker.browse("SQ:", 0, 0).await.ok()?;
                let playlist = playlists
                    .iter()
                    .find(|&p| p.title().eq_ignore_ascii_case(item))?;
                log::debug!("Found playlist {}", playlist.title());
                Some((playlist.uri()?.into(), "".into()))
            }
            SonosFavorite(item) => {
                let favorites = speaker.browse("FV:2", 0, 0).await.ok()?;
                let favorite = favorites
                    .iter()
                    .find(|&f| f.title().eq_ignore_ascii_case(item))?;
                log::debug!("Found favorite {:?}", favorite);
                Some((favorite.uri()?.into(), escape_str_pcdata(favorite.metadata()?).into()))
            }
        }
    }

    /// Add the media to the end of the queue.
    pub(crate) async fn queue_as_next(&self, coordinator_data: &SpeakerData) -> Result<()> {
        let SpeakerData {speaker, transport_data, ..} = &coordinator_data;
        // Look for current track number in transport_data, otherwise fetch it
        let cur_track_no = match transport_data.iter().find_map(|(k, v)| {k.eq_ignore_ascii_case("CurrentTrack"); Some(v)}) {
            Some(track_no) => track_no.parse().map_err(|_| Error::ContentNotFound)?,
            None => speaker.track().await?.map(|t| t.track_no()).unwrap_or(0),
        };
        let (uri, metadata) = self.get_uri_and_metadata(speaker).await.ok_or(ContentNotFound)?;
        speaker.queue_next(&uri, &metadata, Some(cur_track_no+1)).await?;
        Ok(())
    }
    /// Replace what is playing with this
    pub(crate) async fn play_now(&self, coordinator_data: &SpeakerData) -> Result<()> {
        let coordinator = &coordinator_data.speaker;
        let (uri, metadata) = self.get_uri_and_metadata(coordinator).await.ok_or(ContentNotFound)?;
        coordinator.clear_queue().await?;
        coordinator.queue_next(&uri, &metadata, Some(0)).await?;
        // Turn on queue mode
        let queue_uri = format!("x-rincon-queue:{}#0", coordinator.uuid());
        coordinator.set_transport_uri(&queue_uri, "").await?;
        coordinator.play().await.map_err(Error::from)
    }
}
