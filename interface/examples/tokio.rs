use std::time::Duration;

use anyhow::Context;
use interface::{
    Interval, Motors, Speed, h2c,
    implementation_tokio::{DcMotorController, DcMotorControllerHandle},
};
use tokio::sync::{broadcast, mpsc};
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();

    let motor_controller = DcMotorController::open(DcMotorControllerHandle::FirstAvaible)
        .context("Get motor controller interface")?;

    let (tx_out, rx_out) = mpsc::channel(10);
    let (tx_in, mut rx_in) = broadcast::channel(10);

    let join_handle = tokio::spawn(motor_controller.start(tx_in, rx_out));

    tokio::spawn(async move {
        while let Ok(packet) = rx_in.recv().await {
            info!("Got packet: {packet:?}");
        }
    });

    tx_out
        .send(
            h2c::StartStream {
                motors: Motors::Mot0,
                interval: Interval::from_duration(Duration::from_millis(500)),
            }
            .into(),
        )
        .await
        .unwrap();

    tx_out.send(h2c::Ping { id: 42 }.into()).await.unwrap();
    tx_out
        .send(h2c::PacketH2C::ReadProtocolVersion)
        .await
        .unwrap();
    // tx_out.send(h2c::PacketH2C::ResetToUsbBoot).await.unwrap();

    tx_out
        .send(
            h2c::SetArmed::Armed {
                duration: Interval::from_duration(Duration::from_millis(1000)),
            }
            .into(),
        )
        .await
        .unwrap();

    tx_out
        .send(
            h2c::SetSpeed {
                motors: Motors::all(),
                speed: Speed::from_f32(0.5),
            }
            .into(),
        )
        .await
        .unwrap();

    join_handle.await.context("Motor Controller server")
}
