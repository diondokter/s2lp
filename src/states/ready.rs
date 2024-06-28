use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal_async::{delay::DelayNs, digital::Wait, spi::SpiDevice};

use crate::{
    ll::LenWid,
    packet_format::{Basic, Uninitialized},
    Error, S2lp,
};

use super::{Ready, Tx};

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
    ) -> Result<S2lp<Ready<Basic>, Spi, Sdn, Gpio, Delay>, Error<Spi, Sdn, Gpio>> {
        self.ll()
            .pckt_ctrl_6()
            .write_async(|w| w.preamble_len(preamble_length).sync_len(sync_length))
            .await?;

        self.ll()
            .pckt_ctrl_4()
            .write_async(|w| w.address_len(include_address))
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
    ) -> Result<S2lp<Tx<Basic>, Spi, Sdn, Gpio, Delay>, Error<Spi, Sdn, Gpio>> {
        if payload.len() > (u16::MAX - 2) as usize {
            return Err(Error::BufferTooLarge);
        }

        // Choose if we use 1 or byte field
        self.ll()
            .pckt_ctrl_4()
            .modify_async(|w| {
                w.len_wid(if payload.len() <= 254 {
                    LenWid::Bytes1
                } else {
                    LenWid::Bytes1
                })
            })
            .await?;
        // Set the destination address
        self.ll()
            .pckt_flt_goals_3()
            .write_async(|w| w.rx_source_addr_or_dual_sync_3(destination_address))
            .await?;

        // Clear out anything that might still be in the tx fifo
        self.ll().flush_tx_fifo().dispatch_async().await?;

        // TODO: Set IRQ mask

        // Write all we can of the payload into the fifo now
        use device_driver::embedded_io_async::Write;
        let initial_len = self.ll().fifo().write(payload).await?;

        // Start the tx process
        self.ll().tx().dispatch_async().await?;

        Ok(self.cast_state(Tx::new(&payload[initial_len..])))
    }
}

pub use crate::ll::CrcMode;

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
