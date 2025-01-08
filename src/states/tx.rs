use embassy_futures::select::{select, Either};
use embedded_hal::{
    digital::{InputPin, OutputPin},
    spi::SpiDevice,
};
use embedded_hal_async::{delay::DelayNs, digital::Wait};

use crate::{ll::State, Error, ErrorOf, S2lp};

use super::{Ready, Tx};

impl<'buffer, Spi, Sdn, Gpio, Delay, PF> S2lp<Tx<'buffer, PF>, Spi, Sdn, Gpio, Delay>
where
    Spi: SpiDevice,
    Sdn: OutputPin,
    Gpio: InputPin + Wait,
    Delay: DelayNs,
{
    /// Wait for the transmission to be done including waiting for CSMA/CA and retries.
    ///
    /// After this is done, call [Self::abort] to get back the radio in the ready state.
    pub async fn wait(&mut self) -> Result<TxResult, ErrorOf<Self>> {
        if self.state.tx_done {
            return Ok(TxResult::TxAlreadyDone);
        }

        loop {
            // Wait for the interrupt
            match select(self.gpio_pin.wait_for_low(), self.delay.delay_ms(1000)).await {
                Either::First(res) => res.map_err(Error::Gpio)?,
                Either::Second(()) => {
                    // Timeout. Check for bad state
                    let state = self.ll().mc_state_0().read()?.state();
                    #[cfg(feature = "defmt-03")]
                    defmt::error!("TX wait timeout out in state: {}", state);
                    match state {
                        Ok(State::Lockst) | Err(_) => return Err(Error::BadState),
                        _ => {}
                    }
                }
            }

            // Figure out what's up
            let irq_status = self.ll().irq_status().read()?;

            #[cfg(feature = "defmt-03")]
            defmt::trace!("TX wait interrupt: {}", irq_status);

            if irq_status.tx_fifo_error() {
                self.ll().abort().dispatch()?;
                self.ll().flush_tx_fifo().dispatch()?;

                break Ok(TxResult::FifoError);
            }

            if irq_status.tx_fifo_almost_empty() {
                // Refill the fifo
                let written = self.device.fifo().write(self.state.tx_buffer)?;
                self.state.tx_buffer = &self.state.tx_buffer[written..];

                continue;
            }

            let tx_result = if irq_status.tx_data_sent() {
                TxResult::Ok
            } else if irq_status.max_re_tx_reach() {
                TxResult::MaxReTxReached
            } else if irq_status.max_bo_cca_reach() {
                TxResult::MaxBackoffReached
            } else {
                unreachable!();
            };

            self.state.tx_done = true;
            break Ok(tx_result);
        }
    }

    /// Aborts the transmission immediately
    pub fn abort(mut self) -> Result<S2lp<Ready<PF>, Spi, Sdn, Gpio, Delay>, ErrorOf<Self>> {
        self.ll().abort().dispatch()?;
        self.ll().flush_tx_fifo().dispatch()?;

        let digital_frequency = self.state.digital_frequency;
        Ok(self.cast_state(Ready::new(digital_frequency)))
    }

    /// Finish the transmission. This only returns ok when the [Self::wait] function has returned.
    /// If you need to stop the transmission before it's done, call [Self::abort].
    pub fn finish(self) -> Result<S2lp<Ready<PF>, Spi, Sdn, Gpio, Delay>, Self> {
        if self.state.tx_done {
            let digital_frequency = self.state.digital_frequency;
            Ok(self.cast_state(Ready::new(digital_frequency)))
        } else {
            Err(self)
        }
    }
}

/// The result of the TX operation
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt-03", derive(defmt::Format))]
pub enum TxResult {
    /// All went fine and the packet is sent
    Ok,
    /// There was trouble keeping the fifo full.
    /// This may be a performance issue where polling isn't happening fast enough.
    ///
    /// The transmission has been aborted.
    FifoError,
    /// The tx retries have reached their maximum. The packet has been sent, but no ack was received.
    MaxReTxReached,
    /// The Csma/ca engine did not find a good time to send the packet. The packet has not been sent.
    MaxBackoffReached,
    /// The transmission was already done previously
    TxAlreadyDone,
}
