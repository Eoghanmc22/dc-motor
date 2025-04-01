use defmt::{error, unwrap};
use embassy_executor::Spawner;
use embassy_rp::peripherals::{PIN_0, PIN_1, UART0};
use embassy_rp::uart::{BufferedUart, BufferedUartRx, BufferedUartTx, Config};
use embedded_io_async::{Read, Write};
use interface::decoder::{FeedResult, PackerDecoder};
use interface::encoder::encode_packet;
use interface::from_motor_controller;
use static_cell::StaticCell;

use crate::Irqs;
use crate::serial::handler::{self, stream_motor_data};

use super::handler::HandlerCtx;

static UART_CTX: HandlerCtx = HandlerCtx::new();

#[embassy_executor::task]
pub async fn start_uart(spawner: Spawner, uart: UART0, tx_pin: PIN_0, rx_pin: PIN_1) {
    static TX_BUF: StaticCell<[u8; 16]> = StaticCell::new();
    static RX_BUF: StaticCell<[u8; 16]> = StaticCell::new();

    let tx_buf = &mut TX_BUF.init([0; 16])[..];
    let rx_buf = &mut RX_BUF.init([0; 16])[..];

    let uart = BufferedUart::new(
        uart,
        Irqs,
        tx_pin,
        rx_pin,
        tx_buf,
        rx_buf,
        Config::default(),
    );

    let (tx, rx) = uart.split();

    // Run the USB device.
    unwrap!(spawner.spawn(uart_write_half(tx)));
    unwrap!(spawner.spawn(uart_read_half(rx)));
    unwrap!(spawner.spawn(stream_motor_data(&UART_CTX)));
}

#[embassy_executor::task]
async fn uart_write_half(mut sender: BufferedUartTx<'static, UART0>) {
    let mut buffer = [0; 128];

    loop {
        let packet = UART_CTX.packets.receive().await;

        let Ok(buffer) = encode_packet(&packet, &mut buffer) else {
            error!("Error encoding packet");
            continue;
        };

        let res = sender.write_all(buffer).await;
        match res {
            Ok(()) => {}
            Err(err) => {
                error!("Uart tx error: {}", err);
            }
        }
    }
}

#[embassy_executor::task]
async fn uart_read_half(mut receiver: BufferedUartRx<'static, UART0>) {
    let mut decoder = PackerDecoder::<128>::new();
    // TODO: Try to get rid of the need for an extra buffer
    let mut buf = [0; 64];

    loop {
        let res = receiver.read(&mut buf).await;
        let n = match res {
            Ok(n) => n,
            Err(err) => {
                error!("Uart rx error: {}", err);
                decoder.reset();
                continue;
            }
        };

        let mut data = &buf[..n];

        while !data.is_empty() {
            let rst = decoder.feed(data);
            match rst {
                FeedResult::Consumed => {
                    data = &[];
                }
                FeedResult::OverFull(remaining) => {
                    UART_CTX
                        .packets
                        .send(from_motor_controller::Packet::Error(
                            from_motor_controller::Error::DecodingBufferOverflow,
                        ))
                        .await;
                    data = remaining;
                }
                FeedResult::DeserError(remaining) => {
                    UART_CTX
                        .packets
                        .send(from_motor_controller::Packet::Error(
                            from_motor_controller::Error::DecodingError,
                        ))
                        .await;
                    data = remaining;
                }
                FeedResult::Success {
                    data: packet,
                    remaining,
                } => {
                    handler::handle_inbound_packet(&UART_CTX, packet).await;
                    data = remaining;
                }
            }
        }
    }
}
