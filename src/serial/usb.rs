use defmt::{error, info, unwrap};
use embassy_executor::Spawner;
use embassy_rp::peripherals::USB;
use embassy_rp::usb::Driver;
use embassy_usb::UsbDevice;
use embassy_usb::class::cdc_acm::{CdcAcmClass, Receiver, Sender, State};
use interface::decoder::PackerDecoder;
use interface::encoder::encode_packet;
use static_cell::StaticCell;

use crate::Irqs;
use crate::serial::handler::{HandlerCtx, feed_all_and_handle, stream_motor_data};

static USB_CTX: HandlerCtx = HandlerCtx::new();

#[embassy_executor::task]
pub async fn start_usb(spawner: Spawner, usb: USB) {
    let driver = Driver::new(usb, Irqs);

    // Create embassy-usb Config
    let config = {
        let mut config = embassy_usb::Config::new(0xC0DE, 0xCAFE);
        config.manufacturer = Some("Night Owls");
        config.product = Some("DC Motor Controller");
        config.serial_number = None;
        config.max_power = 0;
        config.max_packet_size_0 = 64;
        config
    };

    // Create embassy-usb DeviceBuilder using the driver and config.
    // It needs some buffers for building the descriptors.
    let mut builder = {
        static CONFIG_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
        static BOS_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
        static CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();

        let builder = embassy_usb::Builder::new(
            driver,
            config,
            CONFIG_DESCRIPTOR.init([0; 256]),
            BOS_DESCRIPTOR.init([0; 256]),
            &mut [], // no msos descriptors
            CONTROL_BUF.init([0; 64]),
        );
        builder
    };

    // Create classes on the builder.
    let class = {
        static STATE: StaticCell<State> = StaticCell::new();
        let state = STATE.init(State::new());
        CdcAcmClass::new(&mut builder, state, 64)
    };

    // Build the builder.
    let usb = builder.build();

    let (tx, rx) = class.split();

    // Run the USB device.
    unwrap!(spawner.spawn(usb_task(usb)));
    unwrap!(spawner.spawn(usb_write_half(tx)));
    unwrap!(spawner.spawn(usb_read_half(rx)));
    unwrap!(spawner.spawn(stream_motor_data(&USB_CTX)));
}

type MyUsbDriver = Driver<'static, USB>;
type MyUsbDevice = UsbDevice<'static, MyUsbDriver>;

#[embassy_executor::task]
async fn usb_task(mut usb: MyUsbDevice) -> ! {
    usb.run().await
}

#[embassy_executor::task]
async fn usb_write_half(mut sender: Sender<'static, MyUsbDriver>) {
    let mut buffer = [0; 128];

    loop {
        sender.wait_connection().await;
        USB_CTX.packets.clear();

        info!("USB write half connected");

        'connection: loop {
            let packet = USB_CTX.packets.receive().await;

            let Ok(mut buffer) = encode_packet(&packet, &mut buffer) else {
                error!("Error encoding packet");
                continue;
            };

            let max_packet_size = sender.max_packet_size();

            while !buffer.is_empty() {
                let to_send = buffer.len().min(max_packet_size as usize);

                let Ok(()) = sender.write_packet(&buffer[..to_send]).await else {
                    error!("Error writing packet");
                    break 'connection;
                };

                buffer = &mut buffer[to_send..];
            }
        }
    }
}

#[embassy_executor::task]
async fn usb_read_half(mut receiver: Receiver<'static, MyUsbDriver>) {
    let mut decoder = PackerDecoder::<128>::new();

    loop {
        receiver.wait_connection().await;

        info!("USB read half connected");

        // TODO: Try to get rid of the need for an extra buffer
        let mut buf = [0; 64];
        loop {
            let Ok(n) = receiver.read_packet(&mut buf).await else {
                error!("Read packer error");
                decoder.reset();
                break;
            };

            feed_all_and_handle(&buf[..n], &mut decoder, &USB_CTX).await;
        }
    }
}
