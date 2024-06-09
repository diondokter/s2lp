#![cfg_attr(not(test), no_std)]

use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal_async::{delay::DelayNs, digital::Wait, spi::SpiDevice};
use ll::{Device, DeviceError};

pub mod ll;

pub struct S2lp<Spi: SpiDevice, Sdn: OutputPin, Gpio: InputPin + Wait, Delay: DelayNs> {
    device: Device<Spi>,
    shutdown_pin: Sdn,
    gpio0: Gpio,
    delay: Delay,
}

impl<Spi, Sdn, Gpio, Delay> S2lp<Spi, Sdn, Gpio, Delay>
where
    Spi: SpiDevice,
    Sdn: OutputPin,
    Gpio: InputPin + Wait,
    Delay: DelayNs,
{
    /// Initialize the radio chip
    pub async fn init(
        spi: Spi,
        mut shutdown_pin: Sdn,
        mut gpio0: Gpio,
        mut delay: Delay,
    ) -> Result<Self, Error<Spi::Error, Sdn::Error, Gpio::Error>> {
        #[cfg(feature = "defmt-03")]
        defmt::debug!("Resetting the radio");

        shutdown_pin.set_high().map_err(Error::Sdn)?;
        delay.delay_us(1).await;
        shutdown_pin.set_low().map_err(Error::Sdn)?;

        #[cfg(feature = "defmt-03")]
        defmt::trace!("Waiting for POR");

        gpio0.wait_for_high().await.map_err(Error::Gpio)?;

        let mut device = Device::new(spi);

        #[cfg(feature = "defmt-03")]
        defmt::trace!("Checking interface works");
        let version = device.device_info_0().read_async().await?.version();
        if version != 0xC1 {
            return Err(Error::Init);
        }

        #[cfg(feature = "defmt-03")]
        defmt::debug!("Init done!");

        Ok(Self {
            device,
            shutdown_pin,
            gpio0,
            delay,
        })
    }

    /// Access the registers directly.
    ///
    /// Warning: The driver makes assumptions about the state of the device.
    /// Changing registers directly may break the driver. So be careful.
    pub fn ll(&mut self) -> &mut Device<Spi> {
        &mut self.device
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Error<Spi, Sdn, Gpio> {
    Device(DeviceError<Spi>),
    Sdn(Sdn),
    Gpio(Gpio),
    /// The chip could not be initialized
    Init,
}

impl<Spi, Sdn, Gpio> From<DeviceError<Spi>> for Error<Spi, Sdn, Gpio> {
    fn from(v: DeviceError<Spi>) -> Self {
        Self::Device(v)
    }
}

pub enum GpioNumber {
    Gpio0,
    Gpio1,
    Gpio2,
    Gpio3,
}
