#![cfg_attr(not(test), no_std)]

use core::marker::PhantomData;

use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal_async::{delay::DelayNs, digital::Wait, spi::SpiDevice};
use ll::{Device, DeviceError};

pub mod ll;
pub mod states;
pub mod packet_format;

pub struct S2lp<State, Spi: SpiDevice, Sdn: OutputPin, Gpio: InputPin + Wait, Delay: DelayNs> {
    device: Device<Spi>,
    shutdown_pin: Sdn,
    gpio0: Gpio,
    delay: Delay,
    _phantom: PhantomData<State>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Error<Spi: SpiDevice, Sdn: OutputPin, Gpio: InputPin + Wait> {
    Device(DeviceError<Spi::Error>),
    Sdn(Sdn::Error),
    Gpio(Gpio::Error),
    /// The chip could not be initialized
    Init,
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
