use anyhow::Context;
use futures_util::{SinkExt, StreamExt};
use postcard::de_flavors::crc::from_bytes_u16;
use tokio::{
    select,
    sync::{broadcast, mpsc},
};
use tokio_serial::{SerialPortType, SerialStream, UsbPortInfo};
use tokio_util::{
    bytes::BytesMut,
    codec::{Decoder, Encoder, Framed},
};
use tracing::{error, info, warn};

use crate::{CRC, c2h, encoder, h2c};

pub struct DcMotorController {
    inner: Framed<SerialStream, DcMotorControllerCodec>,
}

impl DcMotorController {
    pub fn enumerate() -> anyhow::Result<impl Iterator<Item = String>> {
        Ok(tokio_serial::available_ports()?
            .into_iter()
            .filter(|port| {
                matches!(
                    port.port_type,
                    SerialPortType::UsbPort(UsbPortInfo {
                        vid: 0xc0de,
                        pid: 0xcafe,
                        manufacturer: Some(ref manufacturer),
                        product: Some(ref product),
                        ..
                    }) if manufacturer == "Night Owls" && product == "DC Motor Controller"
                )
            })
            .map(|it| it.port_name))
    }

    pub fn open(stratagy: DcMotorControllerHandle) -> anyhow::Result<Self> {
        let name = match stratagy {
            DcMotorControllerHandle::FirstAvaible => Self::enumerate()?
                .next()
                .context("No motor controller was found")?,
            DcMotorControllerHandle::Name(name) => name,
        };

        let serial = SerialStream::open(&tokio_serial::new(name, 115200))?;
        Ok(Self {
            inner: DcMotorControllerCodec.framed(serial),
        })
    }

    pub async fn start(
        self,
        inbound: broadcast::Sender<c2h::PacketC2H>,
        mut outbound: mpsc::Receiver<h2c::PacketH2C>,
    ) {
        let mut motor_controller = self.into_inner();

        loop {
            select! {
                inbound_frame = motor_controller.next() => {
                    if let Some(inbound_frame) = inbound_frame {
                        match inbound_frame {
                            Ok(inbound_frame) => {
                                let res = inbound.send(inbound_frame);
                                if res.is_err() {
                                    info!("in channel disconnected");
                                    break;
                                }
                            },
                            Err(err) => {
                                warn!("Error decoding packet: {err:?}");
                            },
                        }
                    } else {
                        info!("end of motor controller stream");
                        break;
                    }
                }
                outbound_frame = outbound.recv() => {
                    if let Some(outbound_frame) = outbound_frame {
                        let res = motor_controller.send(&outbound_frame).await;
                        if let Err(err) = res {
                            error!("Error sending message: {err:?}");
                        }
                    } else {
                        info!("out channel disconnected");
                        break;
                    }
                }
            }
        }
    }

    pub fn into_inner(self) -> Framed<SerialStream, DcMotorControllerCodec> {
        self.inner
    }
}

pub enum DcMotorControllerHandle {
    FirstAvaible,
    Name(String),
}

// FIXME: This type is implemented inefficiently
pub struct DcMotorControllerCodec;

impl Decoder for DcMotorControllerCodec {
    type Item = c2h::PacketC2H;

    type Error = anyhow::Error;

    fn decode(&mut self, src: &mut BytesMut) -> anyhow::Result<Option<Self::Item>> {
        let mut buf = [0; 128];

        let null_byte = src.as_ref().iter().position(|b| *b == 0);
        if let Some(n) = null_byte {
            let msg = src.split_to(n + 1);
            let n = cobs::decode(&msg, &mut buf).context("COBS Decode")?;
            let val = from_bytes_u16(&buf[..n], CRC.digest()).context("Parse packet")?;
            Ok(Some(val))
        } else {
            Ok(None)
        }
    }
}

impl Encoder<&h2c::PacketH2C> for DcMotorControllerCodec {
    type Error = anyhow::Error;

    fn encode(&mut self, item: &h2c::PacketH2C, dst: &mut BytesMut) -> anyhow::Result<()> {
        let mut buf = [0; 128];

        let packet = encoder::encode_packet(item, &mut buf).context("Encode packet")?;
        dst.extend_from_slice(packet);

        Ok(())
    }
}
