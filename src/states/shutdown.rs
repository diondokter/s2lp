use core::marker::PhantomData;

use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal_async::{delay::DelayNs, digital::Wait, spi::SpiDevice};

use crate::{ll::Device, Error, S2lp};

use super::{Ready, Shutdown};

impl<Spi, Sdn, Gpio, Delay> S2lp<Shutdown, Spi, Sdn, Gpio, Delay>
where
    Spi: SpiDevice,
    Sdn: OutputPin,
    Gpio: InputPin + Wait,
    Delay: DelayNs,
{
    pub fn new(spi: Spi, shutdown_pin: Sdn, gpio0: Gpio, delay: Delay) -> Self {
        Self {
            device: Device::new(spi),
            shutdown_pin,
            gpio0,
            delay,
            _phantom: PhantomData,
        }
    }

    /// Initialize the radio chip
    pub async fn init(
        mut self,
    ) -> Result<S2lp<Ready, Spi, Sdn, Gpio, Delay>, Error<Spi::Error, Sdn::Error, Gpio::Error>>
    {
        #[cfg(feature = "defmt-03")]
        defmt::debug!("Resetting the radio");

        self.shutdown_pin.set_high().map_err(Error::Sdn)?;
        self.delay.delay_us(1).await;
        self.shutdown_pin.set_low().map_err(Error::Sdn)?;

        #[cfg(feature = "defmt-03")]
        defmt::trace!("Waiting for POR");

        self.gpio0.wait_for_high().await.map_err(Error::Gpio)?;

        #[cfg(feature = "defmt-03")]
        defmt::trace!("Checking interface works");
        let version = self.device.device_info_0().read_async().await?.version();
        if version != 0xC1 {
            return Err(Error::Init);
        }

        #[cfg(feature = "defmt-03")]
        defmt::debug!("Init done!");

        Ok(S2lp {
            device: self.device,
            shutdown_pin: self.shutdown_pin,
            gpio0: self.gpio0,
            delay: self.delay,
            _phantom: PhantomData,
        })
    }
}
