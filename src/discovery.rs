use crate::{
    speaker::{Speaker, EXTRA_DEVICE_FIELDS},
    urns::SONOS_URN,
    Error, Result,
};
use futures_util::stream::{FuturesUnordered, Stream, TryStreamExt};
use rupnp::Device;
use std::time::Duration;

// 1,408ms +/- 169ms for two devices in network
/*pub(crate) async fn discover_simple(
    timeout: Duration,
) -> Result<impl Stream<Item = Result<Speaker>>> {
    let stream = rupnp::discover(&SONOS_URN.into(), timeout)
        .await?
        .map_ok(Speaker::from_device)
        .map_ok(|device| device.expect("searched for sonos urn but got something else"));

    Ok(stream)
}*/
async fn discover_simple(timeout: Duration) -> Result<impl Stream<Item = Result<Speaker>>> {
    let stream = rupnp::discover_with_properties(&SONOS_URN.into(), timeout, EXTRA_DEVICE_FIELDS)
        .await?
        .try_filter_map(|d| async { Ok(Speaker::from_device(d)) })
        .map_err(|e| e.into());
    Ok(stream)
}

// 292ms +/- 191ms for two devices in network
/// Discover sonos players on the network.
///
/// # Example Usage
///
/// ```rust,no_run
/// # use futures::prelude::*;
/// # use std::time::Duration;
/// # async fn f() -> Result<(), sonor::Error> {
/// let mut devices = sonor::discover(Duration::from_secs(2)).await?;
///
/// while let Some(device) = devices.try_next().await? {
///     let name = device.name();
///     println!("- {}", name);
/// }
/// # Ok(())
/// # };
pub async fn discover(timeout: Duration) -> Result<impl Stream<Item = Result<Speaker>>> {
    // this method searches for devices, but when it finds the first one it
    // uses its `.zone_group_state` to find the other devices in the network.

    let devices = rupnp::discover(&SONOS_URN.into(), timeout)
        .await?
        .try_filter_map(|dev| async move { Ok(Speaker::from_device(dev)) });
    futures_util::pin_mut!(devices);

    let mut devices_iter = None;

    if let Some(device) = devices.try_next().await? {
        let iter = device
            .zone_group_state()
            .await?
            .into_iter()
            .flat_map(|(_, speakers)| speakers)
            .map(|speaker_info| {
                let url = speaker_info.location().parse();
                async {
                    let device = Device::from_url_and_properties(url?, EXTRA_DEVICE_FIELDS).await?;
                    let speaker = Speaker::from_device(device);
                    speaker.ok_or(Error::GetZoneGroupStateReturnedNonSonos)
                }
            });
        devices_iter = Some(iter);
    };

    Ok(devices_iter
        .into_iter()
        .flatten()
        .collect::<FuturesUnordered<_>>())
}

/// Discover one sonos player on the network
pub async fn discover_one(timeout: Duration) -> Result<Speaker> {
    // this method searches for devices, and returns first one it finds
    let devices =
        rupnp::discover_with_properties(&SONOS_URN.into(), timeout, EXTRA_DEVICE_FIELDS).await?;
    futures_util::pin_mut!(devices);

    while let Some(device) = devices.try_next().await? {
        if let Some(speaker) = Speaker::from_device(device) {
            return Ok(speaker);
        }
    }
    Err(Error::NoSpeakersDetected)
}

/// Search for a sonos speaker by its name. Will work even with multiple systems on same network.
///
/// # Example Usage
///
/// ```rust
/// # use futures::prelude::*;
/// # use std::time::Duration;
/// # #[tokio::main]
/// # async fn main() -> Result<(), sonor::Error> {
/// let mut speaker = sonor::find("Living Room", Duration::from_secs(1)).await?
///     .expect("player exists");
/// assert_eq!(speaker.name(), "Living Room");
/// assert_eq!(speaker.update_name().await?, "Living Room");
/// # Ok(())
/// # }
pub async fn find(roomname: &str, timeout: Duration) -> Result<Option<Speaker>> {
    let speakers = discover_simple(timeout).await?;
    futures_util::pin_mut!(speakers);

    while let Some(speaker) = speakers.try_next().await? {
        if speaker.name().eq_ignore_ascii_case(roomname) {
            return Ok(Some(speaker));
        }
    }

    Ok(None)
}
