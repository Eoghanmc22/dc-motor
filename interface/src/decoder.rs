use crc::Crc;
use postcard::de_flavors::crc::from_bytes_u16;
use serde::Deserialize;

use crate::CRC;

// Modified from postcard's CobsAccumulator
pub struct PackerDecoder<const N: usize> {
    buf: [u8; N],
    idx: usize,
    crc: Crc<u16>,
}

/// The result of feeding the accumulator
pub enum FeedResult<'a, T> {
    /// Consumed all data, still pending
    Consumed,

    /// Buffer was filled. Contains remaining section of input, if any
    OverFull(&'a [u8]),

    /// Reached end of chunk, but deserialization failed. Contains remaining section of input, if any
    DeserError(&'a [u8]),

    /// Deserialization complete. Contains deserialized data and remaining section of input, if any
    Success {
        /// Deserialize data.
        data: T,

        /// Remaining data left in the buffer after deserializing
        remaining: &'a [u8],
    },
}

impl<const N: usize> Default for PackerDecoder<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> PackerDecoder<N> {
    /// Create a new accumulator.
    pub const fn new() -> Self {
        PackerDecoder {
            buf: [0; N],
            idx: 0,
            crc: CRC,
        }
    }

    /// Appends data to the internal buffer and attempts to deserialize the accumulated data into `T`
    #[inline]
    pub fn feed<'a, T>(&mut self, input: &'a [u8]) -> FeedResult<'a, T>
    where
        T: for<'de> Deserialize<'de>,
    {
        self.feed_ref(input)
    }

    /// Appends data to the internal buffer and attempts to deserialize the accumulated data into `T`
    ///
    /// This differs from feed, as it allows the `T` to reference data within the internal buffer, but
    /// mutably borrows the accumulator for the lifetime of the deserialization
    /// If `T` does not require the reference, the borrow of `self` ends at the end of the function
    pub fn feed_ref<'de, 'a, T>(&'de mut self, input: &'a [u8]) -> FeedResult<'a, T>
    where
        T: Deserialize<'de>,
    {
        if input.is_empty() {
            return FeedResult::Consumed;
        }

        let zero_pos = input.iter().position(|&i| i == 0);

        if let Some(n) = zero_pos {
            // Yes! We have an end of message here.
            // Add one to include the zero in the "take" portion
            // of the buffer, rather than in "release"
            let (take, release) = input.split_at(n + 1);

            // Does it fit?
            if (self.idx + take.len()) <= N {
                // Aw yiss - add to array
                self.extend_unchecked(take);

                let res = cobs::decode_in_place(&mut self.buf[..self.idx])
                    .map_err(|_| postcard::Error::DeserializeBadEncoding)
                    .and_then(|len| from_bytes_u16(&self.buf[..len], self.crc.digest()));
                let retval = match res {
                    Ok(t) => FeedResult::Success {
                        data: t,
                        remaining: release,
                    },
                    Err(_) => FeedResult::DeserError(release),
                };
                self.idx = 0;
                retval
            } else {
                self.idx = 0;
                FeedResult::OverFull(release)
            }
        } else {
            // Does it fit?
            if (self.idx + input.len()) > N {
                // nope
                let new_start = N - self.idx;
                self.idx = 0;
                FeedResult::OverFull(&input[new_start..])
            } else {
                // yup!
                self.extend_unchecked(input);
                FeedResult::Consumed
            }
        }
    }

    pub fn reset(&mut self) {
        self.idx = 0;
    }

    /// Extend the internal buffer with the given input
    ///
    /// # Panics
    ///
    /// Will panic if the input does not fit in the internal buffer
    fn extend_unchecked(&mut self, input: &[u8]) {
        let new_end = self.idx + input.len();
        self.buf[self.idx..new_end].copy_from_slice(input);
        self.idx = new_end;
    }
}
