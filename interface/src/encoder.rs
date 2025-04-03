use postcard::ser_flavors::{Cobs, Slice, crc::CrcModifier};
use serde::Serialize;

use crate::CRC;

pub fn encode_packet<'a, T: Serialize>(
    value: &T,
    buffer: &'a mut [u8],
) -> postcard::Result<&'a mut [u8]> {
    postcard::serialize_with_flavor(
        value,
        CrcModifier::new(Cobs::try_new(Slice::new(buffer))?, CRC.digest()),
    )
}
