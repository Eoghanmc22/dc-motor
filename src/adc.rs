use defmt::{info, unwrap};
use embassy_executor::Spawner;
use embassy_rp::{
    adc::{Adc, Channel, Config},
    gpio::Pull,
    peripherals::*,
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, watch::Watch};
use embassy_time::{Duration, Ticker};

use crate::Irqs;

#[expect(
    clippy::declare_interior_mutable_const,
    reason = "Used as template to init array of statics"
)]
const NEW_WATCH: Watch<CriticalSectionRawMutex, f32, 4> = Watch::new();
pub static ADC_WATCHES: [Watch<CriticalSectionRawMutex, f32, 4>; 4] = [NEW_WATCH; 4];

#[embassy_executor::task]
pub async fn start_adc_dma(
    spawner: Spawner,
    adc: ADC,
    mut dma: DMA_CH0,
    pin_26: PIN_26,
    pin_27: PIN_27,
    pin_28: PIN_28,
    pin_29: PIN_29,
) {
    let mut adc = Adc::new(adc, Irqs, Config::default());
    // let mut pins = [
    //     Channel::new_pin(pin_26, Pull::None),
    //     Channel::new_pin(pin_27, Pull::None),
    //     Channel::new_pin(pin_28, Pull::None),
    //     Channel::new_pin(pin_29, Pull::None),
    // ];

    // TODO: Gate this order behind feature flag
    let mut pins = [
        Channel::new_pin(pin_28, Pull::None),
        Channel::new_pin(pin_26, Pull::None),
        Channel::new_pin(pin_29, Pull::None),
        Channel::new_pin(pin_27, Pull::None),
    ];

    const NUM_CHANNELS: usize = 4;
    const FREQUENCY: usize = 1000;

    unwrap!(spawner.spawn(log_adc_readings()));

    loop {
        let mut buf = [0_u16; NUM_CHANNELS];
        let div = (48_000_000.0 / (FREQUENCY * NUM_CHANNELS) as f64) as u16; // 100kHz sample rate (48Mhz / (100kHz * 4ch) - 1)

        adc.read_many_multichannel(&mut pins, &mut buf, div, &mut dma)
            .await
            .unwrap();

        for (idx, watch) in ADC_WATCHES.iter().enumerate() {
            let voltage = buf[idx] as f32 / 4095.0 * 3.0;
            let amperage = voltage / 2.2e3 / 4.5e-4;

            watch.sender().send(amperage);
        }
    }
}

#[embassy_executor::task]
async fn log_adc_readings() {
    let mut ticker = Ticker::every(Duration::from_secs(1));

    loop {
        let mot_0 = ADC_WATCHES[0].try_get().unwrap_or(-1.0);
        let mot_1 = ADC_WATCHES[1].try_get().unwrap_or(-1.0);
        let mot_2 = ADC_WATCHES[2].try_get().unwrap_or(-1.0);
        let mot_3 = ADC_WATCHES[3].try_get().unwrap_or(-1.0);

        info!(
            "Motor Amperages: {}A, {}A, {}A, {}A",
            mot_0, mot_1, mot_2, mot_3
        );

        ticker.next().await;
    }
}
