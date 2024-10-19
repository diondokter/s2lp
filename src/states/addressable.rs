use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal_async::{delay::DelayNs, digital::Wait, spi::SpiDevice};

use crate::{
    ll::{Device, DeviceInterface},
    S2lp,
};

use super::Addressable;

#[allow(private_bounds)]
impl<State, Spi, Sdn, Gpio, Delay> S2lp<State, Spi, Sdn, Gpio, Delay>
where
    State: Addressable,
    Spi: SpiDevice,
    Sdn: OutputPin,
    Gpio: InputPin + Wait,
    Delay: DelayNs,
{
    /// Access the registers directly.
    ///
    /// Warning: The driver makes assumptions about the state of the device.
    /// Changing registers directly may break the driver. So be careful.
    pub fn ll(&mut self) -> &mut Device<DeviceInterface<Spi>> {
        &mut self.device
    }
}
