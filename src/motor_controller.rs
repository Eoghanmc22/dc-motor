use embassy_rp::{
    gpio::{AnyPin, Input, Level, Output, Pull},
    pwm::{ChannelAPin, Config, Pwm, SetDutyCycle, Slice},
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};

use crate::adc;

pub static MOTOR_CONTROLLERS: Mutex<CriticalSectionRawMutex, Option<[Drv8874; 4]>> =
    Mutex::new(None);

pub struct Drv8874 {
    motor_id: u8,
    pwm: Pwm<'static>,
    phase: Output<'static>,
    enable: Output<'static>,
    fault: Input<'static>,

    armed: bool,
}

impl Drv8874 {
    pub fn new<T: Slice>(
        motor_id: u8,
        slice: T,
        pwm: impl ChannelAPin<T>,
        phase: impl Into<AnyPin>,
        enable: impl Into<AnyPin>,
        fault: impl Into<AnyPin>,
    ) -> Self {
        Self {
            motor_id,
            // TODO: is the default config appropriate?
            pwm: Pwm::new_output_a(slice, pwm, Config::default()),
            phase: Output::new(phase.into(), Level::Low),
            enable: Output::new(enable.into(), Level::Low),
            fault: Input::new(fault.into(), Pull::None),
            armed: false,
        }
    }

    pub fn set_speed(&mut self, speed: f32) {
        if !self.armed {
            let _ = self.pwm.set_duty_cycle_fully_off();
        }
        self.set_armed(self.armed);

        let duty = speed.abs() * self.pwm.max_duty_cycle() as f32;

        self.phase.set_level((speed >= 0.0).into());
        let _ = self.pwm.set_duty_cycle(duty as u16);
    }

    pub fn set_armed(&mut self, armed: bool) {
        self.enable.set_level(armed.into());
        self.armed = armed;
    }

    pub fn is_armed(&self) -> bool {
        self.armed
    }

    pub fn is_fault(&self) -> bool {
        // Fault pin is active low
        self.fault.is_low()
    }

    pub fn motor_id(&self) -> u8 {
        self.motor_id
    }

    pub fn current_draw(&self) -> f32 {
        adc::ADC_WATCHES[self.motor_id as usize]
            .try_get()
            .unwrap_or(-1.0)
    }
}
