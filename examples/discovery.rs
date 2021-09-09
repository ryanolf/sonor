use futures::prelude::*;
use sonor::{discover, Error, Speaker};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let devices = discover(Duration::from_secs(5))
        .await?
        .try_collect::<Vec<Speaker>>()
        .await?;

    for device in devices.iter() {
        println!("- {}", device.name());
    }
    // while let Some(device) = devices.try_next().await? {
    //     println!("- {}", device.name());
    // }

    Ok(())
}
