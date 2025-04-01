use embassy_futures::select::{Either, select};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, signal::Signal,
};
use embassy_time::{Duration, Timer};

use crate::{motor_controller, safety_watchdog};

use super::{
    CurrentDraw, Motors, Speed,
    from_motor_controller::{self, MotorState, Pong},
    to_motor_controller,
};

pub struct HandlerCtx {
    pub packets: Channel<CriticalSectionRawMutex, from_motor_controller::Packet, 8>,
    pub streams: Signal<CriticalSectionRawMutex, (Motors, Duration)>,
}

impl HandlerCtx {
    pub const fn new() -> Self {
        Self {
            packets: Channel::new(),
            streams: Signal::new(),
        }
    }
}

impl Default for HandlerCtx {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn handle_inbound_packet(ctx: &HandlerCtx, packet: to_motor_controller::Packet) {
    match packet {
        to_motor_controller::Packet::StartStream(start_stream) => {
            ctx.streams
                .signal((start_stream.motors, start_stream.interval.as_duration()));
        }
        to_motor_controller::Packet::SetSpeed(set_speed) => {
            let mut motor_controllers = motor_controller::MOTOR_CONTROLLERS.lock().await;
            let Some(motor_controllers) = &mut *motor_controllers else {
                return;
            };

            for (_, motor_id) in set_speed.motors.iter_names() {
                let motor_id = motor_id.bits().trailing_zeros();
                let motor = &mut motor_controllers[motor_id as usize];

                motor.set_speed(set_speed.speed.as_f32_pct());
            }
        }
        to_motor_controller::Packet::Ping(ping) => {
            let pong = Pong { id: ping.id };

            ctx.packets
                .send(from_motor_controller::Packet::Pong(pong))
                .await;
        }
        to_motor_controller::Packet::SetArmed(set_armed) => match set_armed {
            to_motor_controller::SetArmed::Armed { duration } => {
                safety_watchdog::feed_safety_watch_dog(duration.as_duration())
            }
            to_motor_controller::SetArmed::Disarmed => safety_watchdog::disable_motors(),
        },
    }
}

#[embassy_executor::task]
pub async fn stream_motor_data(ctx: &'static HandlerCtx) {
    let mut config = (Motors::empty(), Duration::MAX);

    loop {
        let new_config_fut = ctx.streams.wait();
        let interval_fut = Timer::after(config.1);

        let select = select(new_config_fut, interval_fut).await;
        match select {
            Either::First(new_config) => config = new_config,
            Either::Second(()) => send_motor_stream(ctx, config.0).await,
        }
    }
}

async fn send_motor_stream(ctx: &HandlerCtx, motors: Motors) {
    let mut motor_controllers = motor_controller::MOTOR_CONTROLLERS.lock().await;
    let Some(motor_controllers) = &mut *motor_controllers else {
        return;
    };

    for (_, motor_id) in motors.iter_names() {
        let motor_id = motor_id.bits().trailing_zeros() as u8;
        let motor = &mut motor_controllers[motor_id as usize];

        ctx.packets
            .send(from_motor_controller::Packet::MotorState(MotorState {
                motor_id,
                last_speed: Speed::from_f32_pct(motor.last_speed()),
                current_draw: CurrentDraw::from_f32_amps(motor.current_draw()),
                is_fault: motor.is_fault(),
            }))
            .await;
    }
}
