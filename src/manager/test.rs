#[allow(unused_imports)]
use super::{controller::Controller, *};

#[tokio::test]
async fn test_controller() -> Result<(), Error> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .is_test(true)
        .init();

    let mut controller = Controller::new();
    let _tx = controller.init().await?;

    log::info!("Initialized manager with devices:");
    for device in controller.speakers().iter() {
        log::info!("     - {}", device.name());
    }

    controller._drop_speaker();

    log::info!("Now we have:");
    for device in controller.speakers().iter() {
        log::info!("     - {}", device.name());
    }

    let handle = tokio::spawn(async move {
        controller.run().await?;
        Ok(())
    });

    // Should look for handle to await and then make new manager. System may
    // get out of sync and throw an error. Rediscover in that case.

    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
    handle.abort();
    handle.await.unwrap_or(Ok(()))
}
