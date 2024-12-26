use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal_async::{delay::DelayNs, digital::Wait, spi::SpiDevice};

use crate::{
    packet_format::{Basic, PacketFormat, Uninitialized},
    ErrorOf, S2lp,
};

use super::{rx::RxMode, Ready, Rx, Tx};

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
    pub async fn set_format<Format: PacketFormat>(
        mut self,
        format_config: &Format::Config,
    ) -> Result<S2lp<Ready<Format>, Spi, Sdn, Gpio, Delay>, ErrorOf<Self>> {
        // Set up the format specific configs
        Format::use_config(&mut self, format_config).await?;

        self.ll()
            .ant_select_conf()
            .modify_async(|reg| reg.set_cs_blanking(true))
            .await?;

        self.ll()
            .pckt_ctrl_3()
            .write_async(|reg| {
                reg.set_rx_mode(crate::ll::RxMode::Normal);
                reg.set_byte_swap(false);
                reg.set_fsk_4_sym_swap(false);
            })
            .await?;

        self.ll()
            .pckt_ctrl_1()
            .write_async(|reg| {
                reg.set_fec_en(false);
                reg.set_second_sync_sel(false);
                reg.set_tx_source(crate::ll::TxSource::Normal);
                reg.set_whit_en(true);
            })
            .await?;

        // Set the tx fifo almost empty to the default
        self.ll().fifo_config_0().write_async(|_| ()).await?;
        // Set the rx fifo almost full to the default
        self.ll().fifo_config_3().write_async(|_| ()).await?;

        self.ll()
            .pm_conf_1()
            .modify_async(|reg| reg.set_smps_lvl_mode(true))
            .await?;

        self.ll()
            .rssi_flt()
            .modify_async(|reg| {
                reg.set_cs_mode(crate::ll::CsMode::StaticCs);
                reg.set_rssi_flt(14)
            })
            .await?;
        self.ll()
            .rssi_th()
            .write_async(|reg| reg.set_value(65))
            .await?; // -85 dB

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
    pub async fn send_packet<'b>(
        mut self,
        tx_meta_data: &Format::TxMetaData,
        payload: &'b [u8],
    ) -> Result<S2lp<Tx<'b, Format>, Spi, Sdn, Gpio, Delay>, ErrorOf<Self>> {
        Format::setup_packet_send(&mut self, tx_meta_data, payload.len()).await?;

        // Clear out anything that might still be in the tx fifo
        self.ll().flush_tx_fifo().dispatch_async().await?;

        // Read the irq status to clear it
        self.ll().irq_status().read_async().await?;
        // Set the irq mask for all the irqs we need
        self.ll()
            .irq_mask()
            .write_async(|reg| {
                reg.set_tx_fifo_almost_empty(true);
                reg.set_tx_data_sent(true);
                reg.set_max_re_tx_reach(true);
                reg.set_tx_fifo_error(true);
                reg.set_max_bo_cca_reach(true);
            })
            .await?;

        // Write all we can of the payload into the fifo now
        let initial_len = self.ll().fifo().write_async(payload).await?;

        #[cfg(feature = "defmt-03")]
        defmt::debug!("Sending packet with len: {}", payload.len());

        // Start the tx process
        self.ll().tx().dispatch_async().await?;

        let digital_frequency = self.state.digital_frequency;
        Ok(self.cast_state(Tx::new(digital_frequency, &payload[initial_len..])))
    }

    /// Start the reception to try and receive a packet
    pub async fn start_receive(
        mut self,
        buffer: &mut [u8],
        mode: RxMode,
    ) -> Result<S2lp<Rx<Basic>, Spi, Sdn, Gpio, Delay>, ErrorOf<Self>> {
        let digital_frequency = self.state.digital_frequency;
        mode.write_to_device(self.ll(), digital_frequency).await?;

        // Clear out anything that might still be in the rx fifo
        self.ll().flush_rx_fifo().dispatch_async().await?;

        // Set the irq mask for all the irqs we need
        self.ll()
            .irq_mask()
            .write_async(|reg| {
                reg.set_rx_data_ready(true);
                reg.set_rx_fifo_almost_full(true);
                reg.set_rx_fifo_error(true);
                reg.set_rx_timeout(true);
                reg.set_rx_data_disc(true);
                reg.set_crc_error(true);
                reg.set_rx_sniff_timeout(true);
            })
            .await?;
        // Read the irq status to clear it
        self.ll().irq_status().read_async().await?;

        #[cfg(feature = "defmt-03")]
        defmt::trace!("Starting receiver");

        // Start the rx process
        self.ll().rx().dispatch_async().await?;

        let digital_frequency = self.state.digital_frequency;
        Ok(self.cast_state(Rx::new(digital_frequency, buffer)))
    }
}
