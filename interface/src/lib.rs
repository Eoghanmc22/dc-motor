#![cfg_attr(not(feature = "std"), no_std)]

pub mod decoder;
pub mod encoder;
#[cfg(all(feature = "std", feature = "implementation_tokio"))]
pub mod implementation_tokio;

use bitflags::bitflags;

use crc::{Crc, Table};
#[cfg(not(feature = "std"))]
use embassy_time::Duration;
use postcard::experimental::max_size::MaxSize;
#[cfg(feature = "std")]
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u16 = 1;

pub const CRC: Crc<u16, Table<1>> = Crc::<u16>::new(&crc::CRC_16_USB);

bitflags! {
    #[derive(Debug, Copy, Clone, Serialize, Deserialize)]
    pub struct Motors: u8 {
        const Mot0 = 0b00000001;
        const Mot1 = 0b00000010;
        const Mot2 = 0b00000100;
        const Mot3 = 0b00001000;
    }
}

impl MaxSize for Motors {
    const POSTCARD_MAX_SIZE: usize = 1;
}

#[derive(Debug, Clone, Serialize, Deserialize, MaxSize)]
pub struct Speed(pub i16);

impl Speed {
    pub fn from_f32(pct: f32) -> Self {
        Self((pct.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
    }

    pub fn as_f32(&self) -> f32 {
        self.0 as f32 / i16::MAX as f32
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, MaxSize)]
pub struct CurrentDraw(pub u16);

impl CurrentDraw {
    pub fn from_f32_amps(amps: f32) -> Self {
        if amps < 0.0 {
            return Self(u16::MAX);
        }

        Self((amps.clamp(0.0, 3.0) / 3.0 * (u16::MAX - 1) as f32) as u16)
    }

    pub fn as_f32_amps(&self) -> f32 {
        if self.0 == u16::MAX {
            return -1.0;
        }

        3.0 * self.0 as f32 / (u16::MAX - 1) as f32
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, MaxSize)]
pub struct Interval(pub u16);

impl Interval {
    pub fn from_duration(dur: Duration) -> Self {
        Self(dur.as_millis() as u16)
    }

    pub fn as_duration(&self) -> Duration {
        Duration::from_millis(self.0 as u64)
    }
}

/// Host -> Motor controller
pub mod h2c {
    use postcard::experimental::max_size::MaxSize;
    use serde::{Deserialize, Serialize};

    use super::{Interval, Motors, Speed};

    #[derive(Debug, Clone, Serialize, Deserialize, MaxSize)]
    pub enum PacketH2C {
        // Stable packets
        ResetToUsbBoot,
        ReadProtocolVersion,
        Ping(Ping),

        // Unstable packets
        ReadSoftwareData,
        StartStream(StartStream),
        SetSpeed(SetSpeed),
        SetArmed(SetArmed),
    }

    #[derive(Debug, Clone, Serialize, Deserialize, MaxSize)]
    pub struct StartStream {
        pub motors: Motors,
        pub interval: Interval,
    }

    impl From<StartStream> for PacketH2C {
        fn from(value: StartStream) -> Self {
            PacketH2C::StartStream(value)
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize, MaxSize)]
    pub struct SetSpeed {
        pub motors: Motors,
        pub speed: Speed,
    }

    impl From<SetSpeed> for PacketH2C {
        fn from(value: SetSpeed) -> Self {
            PacketH2C::SetSpeed(value)
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize, MaxSize)]
    pub struct Ping {
        pub id: u8,
    }

    impl From<Ping> for PacketH2C {
        fn from(value: Ping) -> Self {
            PacketH2C::Ping(value)
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize, MaxSize)]
    pub enum SetArmed {
        Armed {
            // millis
            duration: Interval,
        },
        Disarmed,
    }

    impl From<SetArmed> for PacketH2C {
        fn from(value: SetArmed) -> Self {
            PacketH2C::SetArmed(value)
        }
    }
}

/// Motor controller -> Host
pub mod c2h {
    use postcard::experimental::max_size::MaxSize;
    use serde::{Deserialize, Serialize};

    use super::{CurrentDraw, Speed};

    #[derive(Debug, Clone, Serialize, Deserialize, MaxSize)]
    pub enum PacketC2H {
        // Stable packets
        ProtocolVersionResponse(ProtocolVersionResponse),
        Error(Error),
        Pong(Pong),

        // Unstable packets
        SoftwareDataResponse(SoftwareDataResponse),
        MotorState(MotorState),
    }

    #[derive(Debug, Clone, Serialize, Deserialize, MaxSize)]
    pub struct MotorState {
        pub motor_id: u8,
        pub last_speed: Speed,
        pub current_draw: CurrentDraw,
        pub is_fault: bool,
        pub is_enabled: bool,
    }

    impl From<MotorState> for PacketC2H {
        fn from(value: MotorState) -> Self {
            PacketC2H::MotorState(value)
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize, MaxSize)]
    pub struct Pong {
        pub id: u8,
    }

    impl From<Pong> for PacketC2H {
        fn from(value: Pong) -> Self {
            PacketC2H::Pong(value)
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize, MaxSize)]
    pub struct ProtocolVersionResponse {
        pub version: u16,
    }

    impl From<ProtocolVersionResponse> for PacketC2H {
        fn from(value: ProtocolVersionResponse) -> Self {
            PacketC2H::ProtocolVersionResponse(value)
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize, MaxSize)]
    pub struct SoftwareDataResponse {
        pub version: u16,
    }

    impl From<SoftwareDataResponse> for PacketC2H {
        fn from(value: SoftwareDataResponse) -> Self {
            PacketC2H::SoftwareDataResponse(value)
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize, MaxSize)]
    pub enum Error {
        DecodingError,
        DecodingBufferOverflow,
        Unimplemented,

        #[serde(other)]
        Unknown,
    }

    impl From<Error> for PacketC2H {
        fn from(value: Error) -> Self {
            PacketC2H::Error(value)
        }
    }
}
