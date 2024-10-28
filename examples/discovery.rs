#![allow(unused_imports)]

use futures::prelude::*;
use sonor::{discover, find, Error, Speaker};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .init();
    // let device = find("Sonos Roam", Duration::from_secs(5)).await?;
    // println!("Found device {:?}", device.as_ref().map(|d| d.name()));

    let devices = discover(Duration::from_secs(5))
        .await?
        .try_collect::<Vec<Speaker>>()
        .await?;

    for device in devices.iter() {
        println!("- {} at {}", device.name(), device.device().url());
    }
    // while let Some(device) = devices.try_next().await? {
    //     println!("- {}", device.name());
    // }

    Ok(())
}
