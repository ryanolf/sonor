use super::{metadata::*, Error::{self, *}, Result};
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
            _ => None,
        }
    }

    /// Add the media to the end of the queue.
    pub async fn add_to_queue(&self, coordinator: &Speaker) -> Result<()> {
        let (uri, metadata) = self.get_uri_and_metadata(coordinator).await.ok_or(ContentNotFound)?;
        coordinator.queue_end(&uri, &metadata).await.map_err(Error::from)
    }
    /// Replace what is playing with this
    pub async fn play_now(&self, coordinator: &Speaker) -> Result<()> {
        let (uri, metadata) = self.get_uri_and_metadata(coordinator).await.ok_or(ContentNotFound)?;
        coordinator.clear_queue().await?;
        coordinator.queue_next(&uri, &metadata, Some(0)).await?;
        // Turn on queue mode
        let queue_uri = format!("x-rincon-queue:{}#0", coordinator.uuid());
        coordinator.set_transport_uri(&queue_uri, "").await?;
        coordinator.play().await.map_err(Error::from)
    }
}
