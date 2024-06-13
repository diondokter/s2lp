use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal_async::{delay::DelayNs, digital::Wait, spi::SpiDevice};

use crate::{
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
        preamble_length: u32,
        preamble_pattern: PreamblePattern,
        sync_length: u8,
        sync_pattern: u32,
        postamble_length: u32,
        crc_mode: CrcMode,
    ) -> Result<S2lp<Ready<Basic>, Spi, Sdn, Gpio, Delay>, Error<Spi, Sdn, Gpio>> {
        todo!()
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
        todo!()
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
