use crc::Crc;
use postcard::ser_flavors::{Cobs, Slice, crc::CrcModifier};
use serde::Serialize;

pub fn encode_packet<'a, T: Serialize>(
    value: &T,
    buffer: &'a mut [u8],
) -> postcard::Result<&'a mut [u8]> {
    let crc = Crc::<u16>::new(&crc::CRC_16_USB);
    postcard::serialize_with_flavor(
        value,
        CrcModifier::new(Cobs::try_new(Slice::new(buffer))?, crc.digest()),
    )
}
