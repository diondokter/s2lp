use embedded_hal::{
    digital::{InputPin, OutputPin},
    spi::SpiDevice,
};
use embedded_hal_async::{delay::DelayNs, digital::Wait};

use crate::{
    ll::CcaPeriod,
    packet_format::{Basic, PacketFormat, Uninitialized},
    ErrorOf, S2lp,
};

use super::{rx::RxMode, Ready, Rx, Tx};

impl<Spi, Sdn, Gpio, Delay, PF> S2lp<Ready<PF>, Spi, Sdn, Gpio, Delay>
where
    Spi: SpiDevice,
    Sdn: OutputPin,
    Gpio: InputPin + Wait,
    Delay: DelayNs,
{
    pub fn set_csma_ca(mode: CsmaCaMode) -> Result<(), ErrorOf<Self>> {
        todo!()
    }
}

pub enum CsmaCaMode {
    /// No Csma is done
    Off,
    /// Csma is done without backoff. The radio will keep scanning the channel until it's free and then send the message.
    /// This is only aborted if the transmission is aborted.
    Persistent {
        /// The length of a cca period
        cca_period: CcaPeriod,
        /// The number of consecutive cca periods that must be free for the channel to be deemed free.
        /// 
        /// Range: 1..15
        num_cca_periods: u8,
    },
    /// Csma is done with backoffs. When a channel is busy, the radio will go to sleep until it will try again.
    ///
    /// Each backoff time is random between 0 and a max value based on the backoff prescaler and the number of backoffs already done.
    /// For each backoff, the maximum value doubles.
    ///
    /// When the number of backoffs reaches the maximum,
    /// the transmission is aborted with a [TxResult::MaxBackoffReached](crate::states::tx::TxResult::MaxBackoffReached).
    Backoff {
        /// The length of a cca period
        cca_period: CcaPeriod,
        /// The number of consecutive cca periods that must be free for the channel to be deemed free.
        /// 
        /// Range: 1..15
        num_cca_periods: u8,
        /// The number of backoffs done before the csma/ca engine gives up and aborts the transmmission.
        max_backoffs: u8,
        /// The backoff time is based on the RCO clock (32-34.66khz depending on crystal used) divided by the prescaler.
        /// 
        /// Range: 2..=64
        backoff_prescaler: u8,
        /// The backoff time is based on a prng. This prng is automatically seeded, unless this custom seed is given.
        custom_prng_seed: Option<u16>,
    },
}

impl<Spi, Sdn, Gpio, Delay> S2lp<Ready<Uninitialized>, Spi, Sdn, Gpio, Delay>
where
    Spi: SpiDevice,
    Sdn: OutputPin,
    Gpio: InputPin + Wait,
    Delay: DelayNs,
{
    /// Configure the packet format the radio is going to use.
    ///
    /// The format itself is given as a generic type.
    /// The config parameters are given through a struct as a parameter of the function.
    /// The type of the config struct depends on the used packet format.
    pub fn set_format<Format: PacketFormat>(
        mut self,
        format_config: &Format::Config,
    ) -> Result<S2lp<Ready<Format>, Spi, Sdn, Gpio, Delay>, ErrorOf<Self>> {
        // Set up the format specific configs
        Format::use_config(&mut self, format_config)?;

        self.ll()
            .ant_select_conf()
            .modify(|reg| reg.set_cs_blanking(true))?;

        self.ll().pckt_ctrl_3().write(|reg| {
            reg.set_rx_mode(crate::ll::RxMode::Normal);
            reg.set_byte_swap(false);
            reg.set_fsk_4_sym_swap(false);
        })?;

        self.ll().pckt_ctrl_1().write(|reg| {
            reg.set_fec_en(false);
            reg.set_second_sync_sel(false);
            reg.set_tx_source(crate::ll::TxSource::Normal);
            reg.set_whit_en(true);
        })?;

        // Set the tx fifo almost empty to the default
        self.ll().fifo_config_0().write(|_| ())?;
        // Set the rx fifo almost full to the default
        self.ll().fifo_config_3().write(|_| ())?;

        self.ll()
            .pm_conf_1()
            .modify(|reg| reg.set_smps_lvl_mode(true))?;

        self.ll().rssi_flt().modify(|reg| {
            reg.set_cs_mode(crate::ll::CsMode::StaticCs);
            reg.set_rssi_flt(14)
        })?;
        self.ll().rssi_th().write(|reg| reg.set_value(65))?; // -85 dB

        #[cfg(feature = "defmt-03")]
        defmt::debug!("Packet type has been configured");

        let digital_frequency = self.state.digital_frequency;
        Ok(self.cast_state(Ready::new(digital_frequency)))
    }
}

impl<Format, Spi, Sdn, Gpio, Delay> S2lp<Ready<Format>, Spi, Sdn, Gpio, Delay>
where
    Format: PacketFormat,
    Spi: SpiDevice,
    Sdn: OutputPin,
    Gpio: InputPin + Wait,
    Delay: DelayNs,
{
    /// Start a transmission and send a packet
    pub fn send_packet<'b>(
        mut self,
        tx_meta_data: &Format::TxMetaData,
        payload: &'b [u8],
    ) -> Result<S2lp<Tx<'b, Format>, Spi, Sdn, Gpio, Delay>, ErrorOf<Self>> {
        Format::setup_packet_send(&mut self, tx_meta_data, payload.len())?;

        // Clear out anything that might still be in the tx fifo
        self.ll().flush_tx_fifo().dispatch()?;

        // Read the irq status to clear it
        self.ll().irq_status().read()?;
        // Set the irq mask for all the irqs we need
        self.ll().irq_mask().write(|reg| {
            reg.set_tx_fifo_almost_empty(true);
            reg.set_tx_data_sent(true);
            reg.set_max_re_tx_reach(true);
            reg.set_tx_fifo_error(true);
            reg.set_max_bo_cca_reach(true);
        })?;

        // Write all we can of the payload into the fifo now
        let initial_len = self.ll().fifo().write(payload)?;

        #[cfg(feature = "defmt-03")]
        defmt::debug!("Sending packet with len: {}", payload.len());

        // Start the tx process
        self.ll().tx().dispatch()?;

        let digital_frequency = self.state.digital_frequency;
        Ok(self.cast_state(Tx::new(digital_frequency, &payload[initial_len..])))
    }

    /// Start the reception to try and receive a packet
    pub fn start_receive(
        mut self,
        buffer: &mut [u8],
        mode: RxMode,
    ) -> Result<S2lp<Rx<Basic>, Spi, Sdn, Gpio, Delay>, ErrorOf<Self>> {
        let digital_frequency = self.state.digital_frequency;
        mode.write_to_device(self.ll(), digital_frequency)?;

        // Clear out anything that might still be in the rx fifo
        self.ll().flush_rx_fifo().dispatch()?;

        // Set the irq mask for all the irqs we need
        self.ll().irq_mask().write(|reg| {
            reg.set_rx_data_ready(true);
            reg.set_rx_fifo_almost_full(true);
            reg.set_rx_fifo_error(true);
            reg.set_rx_timeout(true);
            reg.set_rx_data_disc(true);
            reg.set_crc_error(true);
            reg.set_rx_sniff_timeout(true);
        })?;
        // Read the irq status to clear it
        self.ll().irq_status().read()?;

        #[cfg(feature = "defmt-03")]
        defmt::trace!("Starting receiver");

        // Start the rx process
        self.ll().rx().dispatch()?;

        let digital_frequency = self.state.digital_frequency;
        Ok(self.cast_state(Rx::new(digital_frequency, buffer)))
    }
}
