use bytes_buf::{Buf, BufMut};
use defmt::{error, info, unwrap, warn};
use embassy_executor::Spawner;
use embassy_rp::{
    i2c_slave::{Command, Config, I2cSlave},
    peripherals::{I2C1, PIN_18, PIN_19},
};
use num_enum::FromPrimitive;

use crate::{Irqs, motor_controller};

use super::handler::{HandlerCtx, handle_inbound_packet};
use interface::{CurrentDraw, Interval, Motors, Speed, h2c};

#[embassy_executor::task]
pub async fn start_i2c(spawner: Spawner, i2c: I2C1, sda: PIN_19, scl: PIN_18) {
    let mut config = Config::default();
    config.addr = 0x42;
    config.general_call = false;

    let dev = I2cSlave::new(i2c, sda, scl, Irqs, config);
    unwrap!(spawner.spawn(device_task(dev)));
}

#[embassy_executor::task]
async fn device_task(mut dev: I2cSlave<'static, I2C1>) -> ! {
    info!("Start i2c interface");

    loop {
        let mut buf_in = [0u8; 128];
        let mut buf_out = [0u8; 128];
        match dev.listen(&mut buf_in).await {
            Ok(Command::WriteRead(len)) => {
                let remaining = handle_message(&buf_in[..len], &mut buf_out[..])
                    .await
                    .remaining_mut();

                let response_len = buf_out.len() - remaining;
                let res = dev.respond_and_fill(&buf_out[..response_len], 0).await;

                match res {
                    Ok(_) => {}
                    Err(err) => {
                        warn!("I2c error while responding: {}", err);
                    }
                }
            }
            Ok(_) => {
                warn!("Received unsupported i2c command");
                dev.reset();
            }
            Err(err) => error!("I2c error while listening: {}", err),
        }
    }
}

#[derive(Debug, Eq, PartialEq, FromPrimitive)]
#[repr(u8)]
pub enum PacketsI2c {
    SetSpeed = 0,
    ReadMotor = 1,
    Arm = 2,
    #[num_enum(catch_all)]
    Unknown(u8),
}

async fn handle_message<Out: BufMut>(mut msg: impl Buf, mut response: Out) -> Out {
    let cmd = msg.get_u8();

    match PacketsI2c::from(cmd) {
        PacketsI2c::SetSpeed => {
            let motors = Motors::from_bits_truncate(msg.get_u8());
            let speed = Speed(msg.get_i16());

            handle_inbound_packet(&HandlerCtx::new(), h2c::SetSpeed { motors, speed }).await;

            let mut motor_controllers = motor_controller::MOTOR_CONTROLLERS.lock().await;
            if let Some(motor_controllers) = &mut *motor_controllers {
                response.put_u8(motors.bits().count_ones() as u8);

                for (_, motor_id) in motors.iter_names() {
                    let motor_id = motor_id.bits().trailing_zeros();
                    let motor = &mut motor_controllers[motor_id as usize];

                    response.put_u8(motor_id as u8);
                    response.put_u16(CurrentDraw::from_f32_amps(motor.current_draw()).0);
                    response.put_u8(motor.is_fault() as u8);
                }
            } else {
                // Motor controllers are not initialized, write length of 0
                response.put_u8(0);
            }
        }
        PacketsI2c::ReadMotor => {
            let motors = Motors::from_bits_truncate(msg.get_u8());

            let mut motor_controllers = motor_controller::MOTOR_CONTROLLERS.lock().await;
            if let Some(motor_controllers) = &mut *motor_controllers {
                response.put_u8(motors.bits().count_ones() as u8);

                for (_, motor_id) in motors.iter_names() {
                    let motor_id = motor_id.bits().trailing_zeros();
                    let motor = &mut motor_controllers[motor_id as usize];

                    response.put_u8(motor_id as u8);
                    response.put_i16(Speed::from_f32(motor.last_speed()).0);
                    response.put_u16(CurrentDraw::from_f32_amps(motor.current_draw()).0);
                    response.put_u8(motor.is_fault() as u8);
                }
            } else {
                // Motor controllers are not initialized, write length of 0
                response.put_u8(0);
            }
        }
        PacketsI2c::Arm => {
            let duration = Interval(msg.get_u16());

            if duration.0 > 0 {
                handle_inbound_packet(&HandlerCtx::new(), h2c::SetArmed::Armed { duration }).await;
            } else {
                handle_inbound_packet(&HandlerCtx::new(), h2c::SetArmed::Disarmed).await;
            }
        }
        PacketsI2c::Unknown(id) => {
            error!("Received unknown i2c packet id: {}", id);
        }
    }

    response
}
