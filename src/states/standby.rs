use core::marker::PhantomData;

use embedded_hal::{
    digital::{InputPin, OutputPin},
    spi::SpiDevice,
};
use embedded_hal_async::{delay::DelayNs, digital::Wait};

use crate::{ErrorOf, S2lp};

use super::{Ready, Standby};

impl<Spi, Sdn, Gpio, Delay, PF> S2lp<Standby<PF>, Spi, Sdn, Gpio, Delay>
where
    Spi: SpiDevice,
    Sdn: OutputPin,
    Gpio: InputPin + Wait,
    Delay: DelayNs,
{
    /// Wake up the device and go back to ready mode
    pub fn wake_up(mut self) -> Result<S2lp<Ready<PF>, Spi, Sdn, Gpio, Delay>, ErrorOf<Self>> {
        self.ll().ready().dispatch()?;
        let digital_frequency = self.state.digital_frequency;
        Ok(self.cast_state(Ready {
            digital_frequency,
            _p: PhantomData,
        }))
    }
}
