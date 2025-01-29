use device_driver::RegisterInterface;
use embedded_hal::{
    digital::{InputPin, OutputPin},
    spi::SpiDevice,
};
use embedded_hal_async::{delay::DelayNs, digital::Wait};

use crate::{
    ll::Device,
    packet_format::{PacketFormat, RxMetaData},
    Error, ErrorOf, S2lp,
};

use super::{Ready, Rx};

impl<Spi, Sdn, Gpio, Delay, PF: PacketFormat> S2lp<Rx<'_, PF>, Spi, Sdn, Gpio, Delay>
where
    Sdn: OutputPin,
    Gpio: InputPin + Wait,
    Delay: DelayNs,
{
    /// Just waits for the interrupt without acting on it. This is cancel-safe.
    pub async fn wait_for_irq(&mut self) -> Result<(), Error<(), Sdn::Error, Gpio::Error>> {
        self.gpio_pin.wait_for_low().await.map_err(Error::Gpio)?;
        Ok(())
    }
}

impl<Spi, Sdn, Gpio, Delay, PF: PacketFormat> S2lp<Rx<'_, PF>, Spi, Sdn, Gpio, Delay>
where
    Spi: SpiDevice,
    Sdn: OutputPin,
    Gpio: InputPin + Wait,
    Delay: DelayNs,
{
    /// Wait for the receive to be done.
    ///
    /// After this is done, call [Self::abort] to get back the radio in the ready state.
    pub async fn wait(&mut self) -> Result<RxResult<PF::RxMetaData>, ErrorOf<Self>> {
        if self.state.rx_done {
            return Ok(RxResult::RxAlreadyDone);
        }

        loop {
            // Wait for the interrupt
            self.gpio_pin.wait_for_low().await.map_err(Error::Gpio)?;

            // Figure out what's up
            let irq_status = self.ll().irq_status().read()?;

            #[cfg(feature = "defmt-03")]
            defmt::trace!("RX wait interrupt: {}", irq_status);

            if irq_status.rx_data_disc()
                || irq_status.rx_fifo_error()
                || self.state.written == self.state.rx_buffer.len()
            {
                self.ll().abort().dispatch()?;
                self.ll().flush_rx_fifo().dispatch()?;
                self.state.rx_done = true;

                if self.state.written == self.state.rx_buffer.len() {
                    return Ok(RxResult::TooBigForBuffer);
                } else if irq_status.rx_fifo_error() {
                    return Ok(RxResult::Fifo);
                } else if irq_status.crc_error() {
                    return Ok(RxResult::CrcError);
                } else if irq_status.rx_timeout() {
                    return Ok(RxResult::Timeout);
                } else if irq_status.rx_data_disc() {
                    return Ok(RxResult::Discarded);
                } else {
                    unreachable!()
                }
            }

            if irq_status.rx_data_ready() || irq_status.rx_fifo_almost_full() {
                let received = self
                    .device
                    .as_mut()
                    .unwrap()
                    .fifo()
                    .read(&mut self.state.rx_buffer[self.state.written..])?;
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
                    rssi_value: self.ll().rssi_level().read()?.value() as i16 - 146,
                    meta_data: PF::RxMetaData::read_from_device(self.ll())?,
                });
            }
        }
    }

    /// Aborts the transmission immediately
    pub fn abort(mut self) -> Result<S2lp<Ready<PF>, Spi, Sdn, Gpio, Delay>, ErrorOf<Self>> {
        self.ll().abort().dispatch()?;
        self.ll().flush_rx_fifo().dispatch()?;

        let digital_frequency = self.state.digital_frequency;
        Ok(self.cast_state(Ready::new(digital_frequency)))
    }

    /// Finish the transmission. This only returns ok when the [Self::wait] function has returned.
    /// If you need to stop the transmission before it's done, call [Self::abort].
    pub fn finish(self) -> Result<S2lp<Ready<PF>, Spi, Sdn, Gpio, Delay>, Self> {
        if self.state.rx_done {
            let digital_frequency = self.state.digital_frequency;
            Ok(self.cast_state(Ready::new(digital_frequency)))
        } else {
            Err(self)
        }
    }
}

/// The result of an RX operation. This tells the reason why the operation stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt-03", derive(defmt::Format))]
pub enum RxResult<MetaData> {
    /// All went fine and the packet is received
    Ok {
        /// The size of the received packet in bytes
        packet_size: usize,
        /// The RSSI value in dB
        rssi_value: i16,
        /// Format-specific metadata like addresses
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
    /// The RX timeout was reached
    Timeout,
}

/// The mode of receiving
#[derive(Debug)]
#[cfg_attr(feature = "defmt-03", derive(defmt::Format))]
pub enum RxMode {
    /// Normal, default, receiving where the receiver will just be on
    Normal {
        /// If some, the receiving will stop after the configured time.
        /// If none, the receiver will stay on until a packet has been received or the operation is aborted.
        timeout: Option<RxTimeout>,
    },
    LowDutyCycle {
        timeout: RxTimeout,
    },
    Sniff {
        timeout: RxTimeout,
    },
}

impl Default for RxMode {
    fn default() -> Self {
        RxMode::Normal { timeout: None }
    }
}

impl RxMode {
    pub(crate) fn write_to_device<I: RegisterInterface<AddressType = u8>>(
        &self,
        device: &mut Device<I>,
        digital_frequency: u32,
    ) -> Result<(), I::Error> {
        match self {
            RxMode::Normal {
                timeout: Some(timeout),
            } => {
                timeout.write_to_device(device, digital_frequency)?;
            }
            RxMode::Normal { timeout: None } => {
                RxTimeout {
                    timeout_us: 0,
                    mask: RxTimeoutMask::_NoTimeout,
                }
                .write_to_device(device, digital_frequency)?;
            }
            RxMode::LowDutyCycle { timeout: _ } => todo!(),
            RxMode::Sniff { timeout: _ } => todo!(),
        }

        Ok(())
    }
}

/// Timeout settings for the receiver
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt-03", derive(defmt::Format))]
pub struct RxTimeout {
    /// The amount of time after which the RX timer timeout happens
    pub timeout_us: u32,
    /// A mask to prevent the timout from aborting the RX
    pub mask: RxTimeoutMask,
}

impl RxTimeout {
    fn write_to_device<I: RegisterInterface<AddressType = u8>>(
        &self,
        device: &mut Device<I>,
        digital_frequency: u32,
    ) -> Result<(), I::Error> {
        device
            .pckt_flt_options()
            .modify(|reg| reg.set_rx_timeout_and_or_sel((self.mask as u8 & 0b1000) > 0))?;

        device.protocol_2().modify(|reg| {
            reg.set_cs_timeout_mask((self.mask as u8 & 0b0100) > 0);
            reg.set_sqi_timeout_mask((self.mask as u8 & 0b0010) > 0);
            reg.set_pqi_timeout_mask((self.mask as u8 & 0b0001) > 0);
        })?;

        let (prescaler, counter, overflow) =
            find_rx_timer_prescaler_and_counter(self.timeout_us, digital_frequency);

        if overflow {
            #[cfg(feature = "defmt-03")]
            defmt::warn!(
                "RX timeout ({=u32}) is longer than is supported. Max value is used (~3s)",
                self.timeout_us
            );
        }

        device
            .timers_5()
            .write(|reg| reg.set_rx_timer_cntr(counter))?;
        device
            .timers_4()
            .write(|reg| reg.set_rx_timer_presc(prescaler))?;

        Ok(())
    }
}

/// The mask for the RX timer. It can prevent the timer from expiring in situations where it's not desired.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "defmt-03", derive(defmt::Format))]
#[repr(u8)]
pub enum RxTimeoutMask {
    /// INTERNAL API:
    /// Disable the timeout fully. The RX will continue continuesly
    #[doc(hidden)]
    _NoTimeout = 0b0000,
    /// The RX timeout cannot be stopped. It
    /// starts at the RX state and at the end
    /// expires even when a packet is actively
    /// being received
    None = 0b1000,
    /// RSSI above threshold
    Rssi = 0b0100,
    /// SQI above threshold (default)
    #[default]
    Sqi = 0b0010,
    /// PQI above threshold
    Pqi = 0b0001,
    /// Both RSSI AND SQI above threshold
    RssiAndSqi = 0b0110,
    /// Both RSSI AND PQI above threshold
    RssiAndPqi = 0b0101,
    /// Both SQI AND PQI above threshold
    SqiAndPqi = 0b0011,
    /// ALL above threshold
    All = 0b0111,
    /// RSSI OR SQI above threshold
    RssiOrSqi = 0b1110,
    /// RSSI OR PQI above threshold
    RssiOrPqi = 0b1101,
    /// QI OR PQI above threshold
    SqiOrPqi = 0b1011,
    /// ANY above threshold
    Any = 0b1111,
}

fn find_rx_timer_prescaler_and_counter(
    time_microseconds: u32,
    digital_frequency: u32,
) -> (u8, u8, bool) {
    let t_scaled: u64 = time_microseconds as u64 * digital_frequency as u64 / 1210;

    // Avoid division by 1_000_000 prematurely to improve accuracy
    const SCALE: u64 = 1_000_000;
    const MAX_COUNTER: u64 = 255;

    // Calculate the smallest prescaler
    let mut prescaler = t_scaled
        .div_ceil(MAX_COUNTER * SCALE)
        .saturating_sub(1)
        .max(1);

    // Calculate the corresponding counter
    let mut counter = t_scaled.div_ceil((prescaler + 1) * SCALE) + 1;

    if counter > u8::MAX as _ {
        prescaler += 1;
        counter = t_scaled.div_ceil((prescaler + 1) * SCALE) + 1;
    }

    (
        prescaler.try_into().unwrap_or(u8::MAX),
        counter.try_into().unwrap_or(u8::MAX),
        prescaler > 255,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calculate_rx_timeout(prescaler: u8, counter: u8, digital_frequency: f64) -> f64 {
        (prescaler as f64 + 1.0) * (counter as f64 - 1.0) / (digital_frequency / 1210.0)
    }

    #[test]
    fn rx_timeout() {
        fn assert_find(us: u32) -> Option<f32> {
            let (prescaler, counter, overflow) =
                find_rx_timer_prescaler_and_counter(us, 26_000_000);
            let return_us = calculate_rx_timeout(prescaler, counter, 26_000_000.0) * 1_000_000.0;

            // println!("{us} -> {return_us} ({prescaler}, {counter}, {overflow})");

            if !overflow {
                assert!(
                    return_us as f32 / us as f32 > 0.9999,
                    "{us} -> {return_us} ({prescaler}, {counter}, {overflow})"
                );
                Some(return_us as f32 / us as f32)
            } else {
                None
            }
        }

        let mut max_frac = 0.0f32;
        let mut min_frac = f32::INFINITY;

        for us in 1..3_200_000 {
            let fraction = assert_find(us);

            if let Some(fraction) = fraction {
                max_frac = max_frac.max(fraction);
                min_frac = min_frac.min(fraction);
            }

            if us % 10000 == 0 {
                println!("..{us}: {max_frac:1.5}/{min_frac:1.5}");
                max_frac = 0.0;
                min_frac = f32::INFINITY;
            }
        }
    }
}
