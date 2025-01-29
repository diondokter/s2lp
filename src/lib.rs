#![cfg_attr(not(test), no_std)]
#![allow(clippy::type_complexity)] // Ugh, I know

//! Driver for the S2-LP radio chip from ST.
//! Built fully in Rust, uses [embedded_hal] and [device_driver].

use device_driver::embedded_io::ErrorKind;
use embedded_hal::{
    digital::{InputPin, OutputPin},
    spi::SpiDevice,
};
use embedded_hal_async::{delay::DelayNs, digital::Wait};
use ll::{Device, DeviceError, DeviceInterface};

pub mod ll;
pub mod packet_format;
pub mod states;

/// The main driver struct of the crate representing the S2-LP radio
#[derive(Debug)]
pub struct S2lp<State, Spi, Sdn: OutputPin, Gpio: InputPin + Wait, Delay: DelayNs> {
    device: Option<Device<DeviceInterface<Spi>>>,
    shutdown_pin: Sdn,
    gpio_pin: Gpio,
    gpio_number: GpioNumber,
    delay: Delay,
    state: State,
}

impl<State, Spi: SpiDevice, Sdn: OutputPin, Gpio: InputPin + Wait, Delay: DelayNs>
    S2lp<State, Spi, Sdn, Gpio, Delay>
{
    fn cast_state<NextState>(
        self,
        next_state: NextState,
    ) -> S2lp<NextState, Spi, Sdn, Gpio, Delay> {
        S2lp {
            device: self.device,
            shutdown_pin: self.shutdown_pin,
            gpio_pin: self.gpio_pin,
            gpio_number: self.gpio_number,
            delay: self.delay,
            state: next_state,
        }
    }
}

impl<State, Spi: SpiDevice, Sdn: OutputPin, Gpio: InputPin + Wait, Delay: DelayNs>
    S2lp<State, Spi, Sdn, Gpio, Delay>
{
    pub fn take_spi(self) -> (S2lp<State, (), Sdn, Gpio, Delay>, Spi) {
        (
            S2lp {
                device: None,
                shutdown_pin: self.shutdown_pin,
                gpio_pin: self.gpio_pin,
                gpio_number: self.gpio_number,
                delay: self.delay,
                state: self.state,
            },
            self.device.unwrap().interface.spi,
        )
    }
}

impl<State, Sdn: OutputPin, Gpio: InputPin + Wait, Delay: DelayNs>
    S2lp<State, (), Sdn, Gpio, Delay>
{
    pub fn give_spi<Spi: SpiDevice>(self, spi: Spi) -> S2lp<State, Spi, Sdn, Gpio, Delay> {
        S2lp {
            device: Some(Device::new(DeviceInterface::new(spi))),
            shutdown_pin: self.shutdown_pin,
            gpio_pin: self.gpio_pin,
            gpio_number: self.gpio_number,
            delay: self.delay,
            state: self.state,
        }
    }
}

pub(crate) type ErrorOf<S> = <S as ErrorType>::ErrorType;

pub trait ErrorType {
    type ErrorType;
}

impl<State, Spi: SpiDevice, Sdn: OutputPin, Gpio: InputPin + Wait, Delay: DelayNs> ErrorType
    for S2lp<State, Spi, Sdn, Gpio, Delay>
{
    type ErrorType = Error<Spi::Error, Sdn::Error, Gpio::Error>;
}

/// The main error type of the crate
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt-03", derive(defmt::Format))]
pub enum Error<SpiError, SdnError, GpioError> {
    Device(DeviceError<SpiError>),
    Sdn(SdnError),
    Gpio(GpioError),
    FifoError(ErrorKind),
    /// The chip could not be initialized
    Init,
    BadConfig {
        reason: &'static str,
    },
    BufferTooLarge,
    BufferTooSmall,
    ConversionError {
        name: &'static str,
    },
    BadState,
    RcoLockError,
}

impl<SpiError, SdnError, GpioError> From<ErrorKind> for Error<SpiError, SdnError, GpioError> {
    fn from(v: ErrorKind) -> Self {
        Self::FifoError(v)
    }
}

impl<SpiError, SdnError, GpioError> From<DeviceError<SpiError>>
    for Error<SpiError, SdnError, GpioError>
{
    fn from(v: DeviceError<SpiError>) -> Self {
        Self::Device(v)
    }
}

impl<SpiError, SdnError, GpioError, T> From<device_driver::ConversionError<T>>
    for Error<SpiError, SdnError, GpioError>
{
    fn from(val: device_driver::ConversionError<T>) -> Self {
        Self::ConversionError { name: val.target }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt-03", derive(defmt::Format))]
#[repr(u8)]
pub enum GpioNumber {
    Gpio0,
    Gpio1,
    Gpio2,
    Gpio3,
}
