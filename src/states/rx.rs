use embassy_futures::select::{select, Either};
use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal_async::{delay::DelayNs, digital::Wait, spi::SpiDevice};

use crate::{Error, ErrorOf, S2lp};

use super::{Ready, Rx};

impl<'buffer, Spi, Sdn, Gpio, Delay, PF> S2lp<Rx<'buffer, PF>, Spi, Sdn, Gpio, Delay>
where
    Spi: SpiDevice,
    Sdn: OutputPin,
    Gpio: InputPin + Wait,
    Delay: DelayNs,
{
    /// Wait for the receive to be done.
    ///
    /// After this is done, call [Self::stop] to get back the radio in the ready state.
    pub async fn wait(&mut self) -> Result<RxResult, ErrorOf<Self>> {
        if self.state.rx_done {
            return Ok(RxResult::RxAlreadyDone);
        }

        loop {
            // Wait for the interrupt
            self.gpio0.wait_for_low().await.map_err(Error::Gpio)?;

            // Figure out what's up
            let irq_status = self.ll().irq_status().read_async().await?;

            #[cfg(feature = "defmt-03")]
            defmt::trace!("RX wait interrupt: {}", defmt::Debug2Format(&irq_status));

            if irq_status.rx_data_ready() {
                match select(
                    self.device.fifo().read_async(&mut self.state.rx_buffer),
                    self.delay.delay_ms(100),
                )
                .await
                {
                    Either::First(received) => {
                        let received = received?;
                        self.state.written += received;
                        #[cfg(feature = "defmt-03")]
                        defmt::trace!(
                            "Received {} bytes (total = {}) {:X}",
                            received,
                            self.state.written,
                            &self.state.rx_buffer[..self.state.written]
                        );
                    }
                    Either::Second(_) => {
                        #[cfg(feature = "defmt-03")]
                        defmt::error!("Timeout reading the RX fifo");
                    }
                }
            }

            if irq_status.valid_sync() {
                #[cfg(feature = "defmt-03")]
                defmt::trace!("Valid sync received");
            }

            if irq_status.valid_preamble() {
                #[cfg(feature = "defmt-03")]
                defmt::trace!("Valid preamble received");
            }
        }
    }

    /// Aborts the transmission immediately
    pub async fn abort(mut self) -> Result<S2lp<Ready<PF>, Spi, Sdn, Gpio, Delay>, ErrorOf<Self>> {
        self.ll().abort().dispatch_async().await?;
        self.ll().flush_rx_fifo().dispatch_async().await?;

        Ok(self.cast_state(Ready::new()))
    }

    /// Finish the transmission. This only returns ok when the [Self::wait] function has returned.
    /// If you need to stop the transmission before it's done, call [Self::abort].
    pub async fn finish(self) -> Result<S2lp<Ready<PF>, Spi, Sdn, Gpio, Delay>, Self> {
        if self.state.rx_done {
            Ok(self.cast_state(Ready::new()))
        } else {
            Err(self)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt-03", derive(defmt::Format))]
pub enum RxResult {
    /// All went fine and the packet is sent
    Ok(usize),
    /// The reception was already done previously
    RxAlreadyDone,
}
