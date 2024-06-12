use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal_async::{delay::DelayNs, digital::Wait, spi::SpiDevice};

use crate::S2lp;

use super::Ready;

impl<Spi, Sdn, Gpio, Delay> S2lp<Ready, Spi, Sdn, Gpio, Delay>
where
    Spi: SpiDevice,
    Sdn: OutputPin,
    Gpio: InputPin + Wait,
    Delay: DelayNs,
{
}
