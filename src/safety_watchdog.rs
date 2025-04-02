use defmt::warn;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::{Duration, Instant, Timer};

use crate::motor_controller::MOTOR_CONTROLLERS;

static WATCH_DOG_DEADLINE: Signal<CriticalSectionRawMutex, Instant> = Signal::new();

#[embassy_executor::task]
pub async fn start_safety_watch_dog() {
    async fn set_all(enabled: bool) {
        let mut motor_controllers = MOTOR_CONTROLLERS.lock().await;

        if let Some(motor_controllers) = &mut *motor_controllers {
            for motor in motor_controllers {
                motor.set_armed(enabled);
            }
        }
    }

    loop {
        let deadline = WATCH_DOG_DEADLINE.wait().await;

        if deadline == Instant::MAX {
            set_all(false).await;
            continue;
        }

        if deadline > Instant::now() {
            set_all(true).await;
        }

        Timer::at(deadline).await;

        if !WATCH_DOG_DEADLINE.signaled() {
            warn!("Saftey watch dog deadline elapsed");
            set_all(false).await;
        }
    }
}

pub fn feed_safety_watch_dog(dur: Duration) {
    WATCH_DOG_DEADLINE.signal(Instant::now() + dur);
}

pub fn disable_motors() {
    WATCH_DOG_DEADLINE.signal(Instant::MAX);
}
