# DC Motor Controller requirements

Expose the functional of the motor controllers over USB, UART, and SPI

- Set speed
- Report fault status
- Report current draw

## SPI

Command Format:
Command id
Request body
CRC
Status
Response body
CRC

Commands:

- Set Speed (0):
  - Request:
    - Motor id (1 byte)
    - Motor speed (2 bytes)
  - Response:
    - Current Draw (2 bytes)
- Read Motor (1):
  - Request
    - Motor id (1 byte)
  - Response
    - Motor speed (2 bytes)
    - Current Draw (2 bytes)
- Arm
  - Enable for millis (2 byte)
- Software Reset (3)

Status:
OK (0)
Bad Message (1)
Motor Fault (2)

## Serial

Postcard with COBS and CRC

### To Motor Controller

#### StartStream

Payload:

- Bit set of motor ids (u8)
- Interval millis (u16)

After receiving this message the Motor controller will start sending `MotorState` messages

Streaming can be stopped by setting interval to zero

#### SetSpeed

Payload:

- Motor id (u8)
- Motor speed (u16)

#### Ping

Payload:

- Ping id (u8)

Motor controller replies with a `Pong` of the same id

#### SetArmed

Payload:

- enum:
  - Enabled
    - Deadline Millis (NonZeroU16)
  - Disabled

Enables motor outputs for the specified number of milliseconds

#### SoftwareReset

### From Motor Controller

#### Motor State

Payload:

- Motor id (u8)
- Last Speed (u16)
- Current draw (u16)
- Fault status (u8)

#### Pong

Payload:

- Ping id (u8)

#### Error

- Motor id (u8)
- Fault (enum)

Motor id is 0xFF when the error is not related to a motor
