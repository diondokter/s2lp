use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal_async::{delay::DelayNs, digital::Wait, spi::SpiDevice};

use crate::{
    ll::LenWid,
    packet_format::{Basic, Uninitialized},
    Error, ErrorOf, S2lp,
};

use super::{Ready, Rx, Tx};

impl<Spi, Sdn, Gpio, Delay> S2lp<Ready<Uninitialized>, Spi, Sdn, Gpio, Delay>
where
    Spi: SpiDevice,
    Sdn: OutputPin,
    Gpio: InputPin + Wait,
    Delay: DelayNs,
{
    pub async fn set_basic_format(
        mut self,
        preamble_length: u16, // 0-2046
        preamble_pattern: PreamblePattern,
        sync_length: u8, // 0-32
        sync_pattern: u32,
        include_address: bool,
        postamble_length: u8, // In pairs of `01`'s
        crc_mode: CrcMode,
        packet_filter: PacketFilteringOptions,
    ) -> Result<S2lp<Ready<Basic>, Spi, Sdn, Gpio, Delay>, ErrorOf<Self>> {
        self.ll()
            .pckt_ctrl_6()
            .write_async(|w| w.preamble_len(preamble_length).sync_len(sync_length))
            .await?;

        self.ll()
            .pckt_ctrl_4()
            .write_async(|w| w.address_len(include_address).len_wid(LenWid::Bytes2))
            .await?;

        self.ll()
            .pckt_ctrl_3()
            .write_async(|w| {
                w.pckt_frmt(crate::ll::PcktFrmt::Basic)
                    .preamble_sel(preamble_pattern as u8)
                    .rx_mode(crate::ll::RxMode::Normal)
                    .byte_swap(false)
                    .fsk_4_sym_swap(false)
            })
            .await?;

        self.ll()
            .pckt_ctrl_2()
            .write_async(|w| w.fix_var_len(crate::ll::FixVarLen::Variable))
            .await?;

        self.ll()
            .pckt_ctrl_1()
            .write_async(|w| {
                w.crc_mode(crc_mode)
                    .fec_en(false)
                    .second_sync_sel(false)
                    .tx_source(crate::ll::TxSource::Normal)
                    .whit_en(true)
            })
            .await?;

        self.ll()
            .sync()
            .write_async(|w| w.value(sync_pattern.to_be()))
            .await?;

        self.ll()
            .pckt_pstmbl()
            .write_async(|w| w.value(postamble_length))
            .await?;

        // Set the tx fifo almost empty to the default
        self.ll().fifo_config_0().write_async(|w| w).await?;
        // Set the rx fifo almost full to the default
        self.ll().fifo_config_3().write_async(|w| w).await?;

        self.ll()
            .pckt_flt_options()
            .modify_async(|w| {
                w.crc_flt(packet_filter.discard_bad_crc)
                    .dest_vs_broadcast_addr(packet_filter.broadcast_address.is_some())
                    .dest_vs_multicast_addr(packet_filter.multicast_address.is_some())
                    .dest_vs_source_addr(packet_filter.source_address.is_some())
            })
            .await?;

        self.ll()
            .pckt_flt_goals_4()
            .write_async(|w| w.rx_source_mask(packet_filter.source_address_mask))
            .await?;

        self.ll()
            .pckt_flt_goals_3()
            .write_async(|w| {
                w.rx_source_addr_or_dual_sync_3(packet_filter.source_address.unwrap_or_default())
            })
            .await?;

        self.ll()
            .pckt_flt_goals_2()
            .write_async(|w| {
                w.broadcast_addr_or_dual_sync_2(packet_filter.broadcast_address.unwrap_or_default())
            })
            .await?;

        self.ll()
            .pckt_flt_goals_1()
            .write_async(|w| {
                w.multicast_addr_or_dual_sync_1(packet_filter.multicast_address.unwrap_or_default())
            })
            .await?;

        self.ll()
            .protocol_1()
            .modify_async(|w| w.auto_pckt_flt(true))
            .await?;

        self.ll().mod_2().modify_async(|w| w.modulation_type(crate::ll::ModulationType::Fsk2)).await?;

        self.ll().pm_conf_1().modify_async(|w| w.smps_lvl_mode(true)).await?;

        self.ll().rssi_flt().modify_async(|w| w.cs_mode(crate::ll::CsMode::StaticCs).rssi_flt(14)).await?;
        self.ll().rssi_th().write_async(|w| w.value(65)).await?; // -85 dB

        #[cfg(feature = "defmt-03")]
        defmt::debug!("Chip configured for basic packets");

        Ok(self.cast_state(Ready::new()))
    }
}

impl<Spi, Sdn, Gpio, Delay> S2lp<Ready<Basic>, Spi, Sdn, Gpio, Delay>
where
    Spi: SpiDevice,
    Sdn: OutputPin,
    Gpio: InputPin + Wait,
    Delay: DelayNs,
{
    pub async fn send_packet(
        mut self,
        destination_address: u8,
        payload: &[u8],
    ) -> Result<S2lp<Tx<Basic>, Spi, Sdn, Gpio, Delay>, ErrorOf<Self>> {
        if payload.len() > (u16::MAX - 2) as usize {
            return Err(Error::BufferTooLarge);
        }

        // Set the destination address
        self.ll()
            .pckt_flt_goals_0()
            .write_async(|w| w.tx_source_addr_or_dual_sync_0(destination_address))
            .await?;

        // Clear out anything that might still be in the tx fifo
        self.ll().flush_tx_fifo().dispatch_async().await?;

        // Read the irq status to clear it
        self.ll().irq_status().read_async().await?;
        // Set the irq mask for all the irqs we need
        self.ll()
            .irq_mask()
            .write_async(|w| {
                w.tx_fifo_almost_empty(true)
                    .tx_data_sent(true)
                    .max_re_tx_reach(true)
                    .tx_fifo_error(true)
                    .max_bo_cca_reach(true)
            })
            .await?;

        // Write all we can of the payload into the fifo now
        use embedded_io_async::Write;
        let initial_len = self.ll().fifo().write(payload).await?;

        #[cfg(feature = "defmt-03")]
        defmt::debug!("Sending basic packet with len: {}", payload.len());

        // Start the tx process
        self.ll().tx().dispatch_async().await?;

        Ok(self.cast_state(Tx::new(&payload[initial_len..])))
    }

    pub async fn start_receive(
        mut self,
        buffer: &mut [u8],
    ) -> Result<S2lp<Rx<Basic>, Spi, Sdn, Gpio, Delay>, ErrorOf<Self>> {
        // Clear out anything that might still be in the rx fifo
        self.ll().flush_rx_fifo().dispatch_async().await?;

        // Set the irq mask for all the irqs we need
        self.ll()
            .irq_mask()
            .write_async(|w| {
                w.rx_data_ready(true)
                    .rx_fifo_almost_full(true)
                    .rx_fifo_error(true)
                    .rx_timeout(true)
                    .rx_data_disc(true)
                    .crc_error(true)
                    .rx_sniff_timeout(true)
                    .valid_preamble(true)
                    .valid_sync(true)
            })
            .await?;
        // Read the irq status to clear it
        self.ll().irq_status().read_async().await?;

        #[cfg(feature = "defmt-03")]
        defmt::debug!("Receiving basic packet");

        // Start the tx process
        self.ll().rx().dispatch_async().await?;

        Ok(self.cast_state(Rx::new(buffer)))
    }
}

pub use crate::ll::CrcMode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt-03", derive(defmt::Format))]
#[repr(u8)]
pub enum PreamblePattern {
    /// - `0101` for 2(G)FSK or OOK/ASK
    /// - `0010` for 4(G)FSK
    Pattern0,
    /// - `1010` for 2(G)FSK or OOK/ASK
    /// - `0111` for 4(G)FSK
    Pattern1,
    /// - `1100` for 2(G)FSK or OOK/ASK
    /// - `1101` for 4(G)FSK
    Pattern2,
    /// - `0011` for 2(G)FSK or OOK/ASK
    /// - `1000` for 4(G)FSK
    Pattern3,
}

pub struct PacketFilteringOptions {
    discard_bad_crc: bool,
    source_address: Option<u8>,
    /// Bitmask for the source address
    source_address_mask: u8,
    multicast_address: Option<u8>,
    broadcast_address: Option<u8>,
}

impl Default for PacketFilteringOptions {
    fn default() -> Self {
        Self {
            discard_bad_crc: true,
            source_address: Some(0xAA),
            source_address_mask: 0xFF,
            multicast_address: None,
            broadcast_address: Some(0xFF),
        }
    }
}
