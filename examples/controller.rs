#[allow(unused_imports)]
use sonor::{Command, Controller, Error};
use tokio::time::{sleep, Duration};
use log;

#[tokio::main]
async fn main() -> Result<(), Error> {
    simple_logger::init_with_level(log::Level::Debug).unwrap();
    let mut controller = Controller::new();
    let tx = controller.init().await?;

    println!("Initialized manager with devices:");
    for device in controller.speakers().iter() {
        println!("- {}", device.name());
    }

    controller.drop_speaker();

    println!("Now we have:");
    for device in controller.speakers().iter() {
        println!("- {}", device.name());
    }

    let handle = tokio::spawn(async move {
        controller.run().await?;
        Ok(())
    });

    // Should look for handle to await and then make new manager. System may
    // get out of sync and throw an error. Rediscover in that case.

    sleep(Duration::from_millis(10000)).await;
    tx.send(Command::Break).await.unwrap();
    handle.await.unwrap()
}
