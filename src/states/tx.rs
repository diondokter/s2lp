use embassy_futures::select::{select, Either};
use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal_async::{delay::DelayNs, digital::Wait, spi::SpiDevice};

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
    /// After this is done, call [Self::stop] to get back the radio in the ready state.
    pub async fn wait(&mut self) -> Result<TxResult, ErrorOf<Self>> {
        if self.state.tx_done {
            return Ok(TxResult::TxAlreadyDone);
        }

        loop {
            // Wait for the interrupt
            match select(self.gpio0.wait_for_low(), self.delay.delay_ms(1000)).await {
                Either::First(res) => res.map_err(Error::Gpio)?,
                Either::Second(()) => {
                    // Timeout. Check for bad state
                    let state = self.ll().mc_state_0().read_async().await?.state();
                    #[cfg(feature = "defmt-03")]
                    defmt::error!(
                        "TX wait timeout out in state: {}",
                        defmt::Debug2Format(&state)
                    );
                    match state {
                        Ok(State::Lockst) | Err(_) => return Err(Error::BadState),
                        _ => {}
                    }
                }
            }

            // Figure out what's up
            let irq_status = self.ll().irq_status().read_async().await?;

            #[cfg(feature = "defmt-03")]
            defmt::trace!("TX wait interrupt: {}", defmt::Debug2Format(&irq_status));

            if irq_status.tx_fifo_error() {
                self.ll().abort().dispatch_async().await?;
                self.ll().flush_tx_fifo().dispatch_async().await?;

                break Ok(TxResult::FifoError);
            }

            if irq_status.tx_fifo_almost_empty() {
                // Refill the fifo
                let written = self.device.fifo().write_async(self.state.tx_buffer).await?;
                self.state.tx_buffer = &self.state.tx_buffer[written..];

                continue;
            }

            let tx_result = if irq_status.tx_data_sent() {
                TxResult::Ok
            } else if irq_status.max_re_tx_reach() {
                TxResult::MaxReTxReached
            } else if irq_status.max_bo_cca_reach() {
                TxResult::CcaBackoffReached
            } else {
                unreachable!();
            };

            self.state.tx_done = true;
            break Ok(tx_result);
        }
    }

    /// Aborts the transmission immediately
    pub async fn abort(mut self) -> Result<S2lp<Ready<PF>, Spi, Sdn, Gpio, Delay>, ErrorOf<Self>> {
        self.ll().abort().dispatch_async().await?;
        self.ll().flush_tx_fifo().dispatch_async().await?;

        Ok(self.cast_state(Ready::new()))
    }

    /// Finish the transmission. This only returns ok when the [Self::wait] function has returned.
    /// If you need to stop the transmission before it's done, call [Self::abort].
    pub async fn finish(self) -> Result<S2lp<Ready<PF>, Spi, Sdn, Gpio, Delay>, Self> {
        if self.state.tx_done {
            Ok(self.cast_state(Ready::new()))
        } else {
            Err(self)
        }
    }
}

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
    /// The Cca engine did not find a good time to send the packet. The packet has not been sent.
    CcaBackoffReached,
    /// The transmission was already done previously
    TxAlreadyDone,
}
