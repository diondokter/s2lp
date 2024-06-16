#![cfg_attr(not(test), no_std)]

use core::marker::PhantomData;

use device_driver::embedded_io::ErrorKind;
use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal_async::{delay::DelayNs, digital::Wait, spi::SpiDevice};
use ll::{Device, DeviceError};

pub mod ll;
pub mod packet_format;
pub mod states;

pub struct S2lp<State, Spi: SpiDevice, Sdn: OutputPin, Gpio: InputPin + Wait, Delay: DelayNs> {
    device: Device<Spi>,
    shutdown_pin: Sdn,
    gpio0: Gpio,
    delay: Delay,
    _phantom: PhantomData<State>,
}

impl<State, Spi: SpiDevice, Sdn: OutputPin, Gpio: InputPin + Wait, Delay: DelayNs>
    S2lp<State, Spi, Sdn, Gpio, Delay>
{
    fn cast_state<NextState>(self) -> S2lp<NextState, Spi, Sdn, Gpio, Delay> {
        S2lp {
            device: self.device,
            shutdown_pin: self.shutdown_pin,
            gpio0: self.gpio0,
            delay: self.delay,
            _phantom: PhantomData,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error<Spi: SpiDevice, Sdn: OutputPin, Gpio: InputPin + Wait> {
    Device(DeviceError<Spi::Error>),
    Sdn(Sdn::Error),
    Gpio(Gpio::Error),
    FifoError(ErrorKind),
    /// The chip could not be initialized
    Init,
    BufferTooLarge,
    BufferTooSmall,
}

impl<Spi: SpiDevice, Sdn: OutputPin, Gpio: InputPin + Wait> From<ErrorKind>
    for Error<Spi, Sdn, Gpio>
{
    fn from(v: ErrorKind) -> Self {
        Self::FifoError(v)
    }
}

impl<Spi: SpiDevice, Sdn: OutputPin, Gpio: InputPin + Wait> From<DeviceError<Spi::Error>>
    for Error<Spi, Sdn, Gpio>
{
    fn from(v: DeviceError<Spi::Error>) -> Self {
        Self::Device(v)
    }
}

pub enum GpioNumber {
    Gpio0,
    Gpio1,
    Gpio2,
    Gpio3,
}
