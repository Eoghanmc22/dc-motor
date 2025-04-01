pub mod decoder;
pub mod encoder;
pub mod handler;
pub mod i2c;
pub mod uart;
pub mod usb;

use bitflags::bitflags;
use embassy_time::Duration;
use serde::{Deserialize, Serialize};

bitflags! {
    #[derive(Debug, Copy, Clone, Serialize, Deserialize)]
    pub struct Motors: u8 {
        const Mot0 = 0b00000001;
        const Mot1 = 0b00000010;
        const Mot2 = 0b00000100;
        const Mot3 = 0b00001000;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Speed(pub i16);

impl Speed {
    pub fn from_f32_pct(pct: f32) -> Self {
        Self((pct.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
    }

    pub fn as_f32_pct(&self) -> f32 {
        self.0 as f32 / i16::MAX as f32
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Interval(pub u16);

impl Interval {
    pub fn from_duration(dur: Duration) -> Self {
        Self(dur.as_millis() as u16)
    }

    pub fn as_duration(&self) -> Duration {
        Duration::from_millis(self.0 as u64)
    }
}

pub mod to_motor_controller {
    use serde::{Deserialize, Serialize};

    use super::{Interval, Motors, Speed};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum Packet {
        StartStream(StartStream),
        SetSpeed(SetSpeed),
        Ping(Ping),
        SetArmed(SetArmed),
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct StartStream {
        pub motors: Motors,
        pub interval: Interval,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SetSpeed {
        pub motors: Motors,
        pub speed: Speed,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Ping {
        pub id: u8,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum SetArmed {
        Armed {
            // millis
            duration: Interval,
        },
        Disarmed,
    }
}

pub mod from_motor_controller {
    use serde::{Deserialize, Serialize};

    use super::{CurrentDraw, Speed};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum Packet {
        MotorState(MotorState),
        Pong(Pong),
        Error(Error),
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct MotorState {
        pub motor_id: u8,
        pub last_speed: Speed,
        pub current_draw: CurrentDraw,
        pub is_fault: bool,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Pong {
        pub id: u8,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum Error {
        DecodingError,
        DecodingBufferOverflow,
    }
}
