#[allow(unused_imports)]
use sonor::{Command, Controller, Error};
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let mut manager = Controller::new().await?;

    let (tx, rx) = mpsc::channel(32);

    println!("Initialized manager with devices:");
    for device in manager.speakers().iter() {
        println!("- {}", device.name());
    }

    manager.drop_speaker();

    println!("Now we have:");
    for device in manager.speakers().iter() {
        println!("- {}", device.name());
    }
    let tx2 = tx.clone();
    let handle = tokio::spawn(async move {
        manager.run(tx2, rx).await?;
        Ok(())
    });

    // Should look for handle to await and then make new manager. System may
    // get out of sync and throw an error. Rediscover in that case.

    sleep(Duration::from_millis(10000)).await;
    tx.send(Command::Break).await.unwrap();
    handle.await.unwrap()
}
