use embassy_futures::select::{Either, select};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, signal::Signal,
};
use embassy_time::{Duration, Instant, Timer};

use crate::{motor_controller, safety_watchdog};

use interface::{
    CurrentDraw, Motors, Speed,
    c2h::{self, PacketC2H},
    decoder::{FeedResult, PackerDecoder},
    h2c::{self, PacketH2C},
};

pub struct HandlerCtx {
    pub packets: Channel<CriticalSectionRawMutex, PacketC2H, 8>,
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

pub async fn feed_all_and_handle<const N: usize>(
    mut data: &[u8],
    decoder: &mut PackerDecoder<N>,
    ctx: &HandlerCtx,
) {
    while !data.is_empty() {
        let rst = decoder.feed::<PacketH2C>(data);
        match rst {
            FeedResult::Consumed => {
                data = &[];
            }
            FeedResult::OverFull(remaining) => {
                ctx.packets
                    .send(c2h::Error::DecodingBufferOverflow.into())
                    .await;
                data = remaining;
            }
            FeedResult::DeserError(remaining) => {
                ctx.packets.send(c2h::Error::DecodingError.into()).await;
                data = remaining;
            }
            FeedResult::Success {
                data: packet,
                remaining,
            } => {
                handle_inbound_packet(ctx, packet).await;
                data = remaining;
            }
        }
    }
}

pub async fn handle_inbound_packet(ctx: &HandlerCtx, packet: impl Into<PacketH2C>) {
    match packet.into() {
        PacketH2C::StartStream(start_stream) => {
            ctx.streams
                .signal((start_stream.motors, start_stream.interval.as_duration()));
        }
        PacketH2C::SetSpeed(set_speed) => {
            let mut motor_controllers = motor_controller::MOTOR_CONTROLLERS.lock().await;
            let Some(motor_controllers) = &mut *motor_controllers else {
                return;
            };

            for (_, motor_id) in set_speed.motors.iter_names() {
                let motor_id = motor_id.bits().trailing_zeros();
                let motor = &mut motor_controllers[motor_id as usize];

                motor.set_speed(set_speed.speed.as_f32());
            }
        }
        PacketH2C::Ping(ping) => {
            let pong = c2h::Pong { id: ping.id };
            ctx.packets.send(pong.into()).await;
        }
        PacketH2C::SetArmed(set_armed) => match set_armed {
            h2c::SetArmed::Armed { duration } => {
                safety_watchdog::feed_safety_watch_dog(duration.as_duration())
            }
            h2c::SetArmed::Disarmed => safety_watchdog::disable_motors(),
        },
        PacketH2C::ResetToUsbBoot => {
            embassy_rp::rom_data::reset_to_usb_boot(0, 0);
        }
        PacketH2C::ReadProtocolVersion => {
            ctx.packets
                .send(
                    c2h::ProtocolVersionResponse {
                        version: interface::PROTOCOL_VERSION,
                    }
                    .into(),
                )
                .await;
        }
        PacketH2C::ReadSoftwareData => {
            ctx.packets.send(c2h::Error::Unimplemented.into()).await;
        }
    }
}

#[embassy_executor::task(pool_size = 2)]
pub async fn stream_motor_data(ctx: &'static HandlerCtx) {
    let mut config = (Motors::empty(), Duration::MAX);

    loop {
        let new_config_fut = ctx.streams.wait();
        // let interval_fut = Timer::after(config.1);
        let interval_fut = Timer::at(Instant::now().checked_add(config.1).unwrap_or(Instant::MAX));

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
            .send(
                c2h::MotorState {
                    motor_id,
                    last_speed: Speed::from_f32(motor.last_speed()),
                    current_draw: CurrentDraw::from_f32_amps(motor.current_draw()),
                    is_fault: motor.is_fault(),
                    is_enabled: motor.is_armed(),
                }
                .into(),
            )
            .await;
    }
}
