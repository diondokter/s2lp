use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal_async::{delay::DelayNs, digital::Wait, spi::SpiDevice};

use crate::{
    packet_format::{PacketFormat, RxMetaData},
    Error, ErrorOf, S2lp,
};

use super::{Ready, Rx};

impl<'buffer, Spi, Sdn, Gpio, Delay, PF: PacketFormat> S2lp<Rx<'buffer, PF>, Spi, Sdn, Gpio, Delay>
where
    Spi: SpiDevice,
    Sdn: OutputPin,
    Gpio: InputPin + Wait,
    Delay: DelayNs,
{
    /// Wait for the receive to be done.
    ///
    /// After this is done, call [Self::stop] to get back the radio in the ready state.
    pub async fn wait(&mut self) -> Result<RxResult<PF::RxMetaData>, ErrorOf<Self>> {
        if self.state.rx_done {
            return Ok(RxResult::RxAlreadyDone);
        }

        loop {
            // Wait for the interrupt
            self.gpio0.wait_for_low().await.map_err(Error::Gpio)?;

            // Figure out what's up
            let irq_status = self.ll().irq_status().read_async().await?;

            #[cfg(feature = "defmt-03")]
            defmt::trace!("RX wait interrupt: {}", irq_status);

            if irq_status.rx_data_disc()
                || irq_status.rx_fifo_error()
                || self.state.written == self.state.rx_buffer.len()
            {
                self.device.abort().dispatch_async().await?;
                self.device.flush_rx_fifo().dispatch_async().await?;
                self.state.rx_done = true;

                if self.state.written == self.state.rx_buffer.len() {
                    return Ok(RxResult::TooBigForBuffer);
                } else if irq_status.rx_fifo_error() {
                    return Ok(RxResult::Fifo);
                } else if irq_status.crc_error() {
                    return Ok(RxResult::CrcError);
                } else if irq_status.rx_data_disc() {
                    return Ok(RxResult::Discarded);
                } else {
                    unreachable!()
                }
            }

            if irq_status.rx_data_ready() || irq_status.rx_fifo_almost_full() {
                let received = self
                    .device
                    .fifo()
                    .read_async(&mut self.state.rx_buffer[self.state.written..])
                    .await?;
                self.state.written += received;

                #[cfg(feature = "defmt-03")]
                defmt::trace!(
                    "Received {} bytes (total = {}) {:X}",
                    received,
                    self.state.written,
                    &self.state.rx_buffer[..self.state.written]
                );
            }

            if irq_status.rx_data_ready() {
                self.state.rx_done = true;
                return Ok(RxResult::Ok {
                    packet_size: self.state.written,
                    rssi_value: self.device.rssi_level().read_async().await?.value() as i16 - 146,
                    meta_data: PF::RxMetaData::read_from_device(&mut self.device).await?,
                });
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
pub enum RxResult<MetaData> {
    /// All went fine and the packet is received
    Ok {
        packet_size: usize,
        rssi_value: i16,
        meta_data: MetaData,
    },
    /// The reception was already done previously
    RxAlreadyDone,
    /// The RX fifo filled up too fast and we couldn't keep up
    Fifo,
    /// While receiving the packet, it got filtered out
    Discarded,
    /// The received packet has a bad CRC
    CrcError,
    /// The received message was bigger than the given buffer
    TooBigForBuffer,
}
