use std::time::Duration;

use sonor::{manager::{self, MediaSource::*}, Manager};
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), manager::Error> {
    simple_logger::init_with_level(log::Level::Debug).unwrap();

    let manager = Manager::new().await?;

    let zone = manager.get_zone("Living Room").await?;
    // let snapshot = zone.take_snapshot().await?;
    // zone.clear_queue().await?;
    // zone.play_now(Apple("librarytrack:a.1442979904".into())).await?;
    // zone.play_now(Apple("track:1025212410".into())).await?;
    // zone.play_now(Spotify("track:4LI1ykYGFCcXPWkrpcU7hn".into())).await?;
    // zone.play_now(Spotify("album:4hW2wvP51Myt7UIVTgSp4f".into())).await?;
    // zone.play_now(Spotify("user:spotify:playlist:32O0SSXDNWDrMievPkV0Im".into())).await?;
    // zone.play_now(Apple("album:1025210938".into())).await?;
    zone.play_now(SonosFavorite("New York Rhapsody".into())).await?;
    sleep(Duration::from_secs(10)).await;
    // zone.play_now(SonosPlaylist("Cars 1, 2, 3".into())).await?;
    // zone.pause().await?;
    // zone.apply_snapshot(snapshot).await?;
    Ok(())
}
