use std::time::Duration;

use sonor::{manager, Manager};
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), manager::Error> {
    simple_logger::init_with_level(log::Level::Debug).unwrap();

    let manager = Manager::new().await?;
    println!("got manager");
    sleep(Duration::from_millis(2000)).await;

    let uri = "x-sonos-http:librarytrack:a.1442979904.mp4?sid=204";
    let zone = manager.get_zone("Living Room").await?;
    let snapshot = zone.take_snapshot().await?;
    zone.play_now(uri).await?;

    sleep(Duration::from_secs(10)).await;
    zone.pause().await?;
    zone.apply_snapshot(snapshot).await?;

    Ok(())
}
