//! This example shows how to use USB (Universal Serial Bus) in the RP2040 chip.
//!
//! This creates a USB serial port that echos.

#![no_std]
#![no_main]

pub mod current;
pub mod motor_controller;
pub mod safety_watchdog;
pub mod serial;

use defmt::{info, unwrap};
use embassy_executor::Spawner;
use embassy_rp::peripherals::{I2C1, UART0, USB};
use embassy_rp::{adc, uart, usb};
use embassy_rp::{bind_interrupts, i2c};
use motor_controller::Drv8874;
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => usb::InterruptHandler<USB>;
    UART0_IRQ => uart::BufferedInterruptHandler<UART0>;
    ADC_IRQ_FIFO => adc::InterruptHandler;
    I2C1_IRQ => i2c::InterruptHandler<I2C1>;


});

// TODO:
// Look into what mutex impl we should be using
// Make buffer sizes more appropriate

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("Starting DC Motor controller");

    let p = embassy_rp::init(Default::default());

    // Configure global for motor controllers
    {
        let mut motor_controllers = motor_controller::MOTOR_CONTROLLERS.lock().await;

        // TODO: Gate this order behind feature flag
        *motor_controllers = Some([
            Drv8874::new(0, p.PWM_SLICE3, p.PIN_6, p.PIN_7, p.PIN_8, p.PIN_9),
            Drv8874::new(1, p.PWM_SLICE7, p.PIN_14, p.PIN_15, p.PIN_16, p.PIN_17),
            Drv8874::new(2, p.PWM_SLICE1, p.PIN_2, p.PIN_3, p.PIN_4, p.PIN_5),
            Drv8874::new(3, p.PWM_SLICE5, p.PIN_10, p.PIN_11, p.PIN_12, p.PIN_13),
        ]);
    }

    unwrap!(spawner.spawn(safety_watchdog::start_safety_watch_dog()));
    unwrap!(spawner.spawn(serial::usb::start_usb(spawner, p.USB)));
    unwrap!(spawner.spawn(serial::uart::start_uart(spawner, p.UART0, p.PIN_0, p.PIN_1)));
    unwrap!(spawner.spawn(serial::i2c::start_i2c(spawner, p.I2C1, p.PIN_19, p.PIN_18)));
    unwrap!(spawner.spawn(current::start_adc_dma(
        spawner, p.ADC, p.DMA_CH0, p.PIN_26, p.PIN_27, p.PIN_28, p.PIN_29
    )));
}
