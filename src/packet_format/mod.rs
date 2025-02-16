//! Module containing all packet format handling and setup

use core::fmt::Debug;

use device_driver::RegisterInterface;
use embedded_hal::{
    digital::{InputPin, OutputPin},
    spi::SpiDevice,
};
use embedded_hal_async::{delay::DelayNs, digital::Wait};

use crate::{ll::Device, states::Ready, ErrorOf, S2lp};

mod basic;
pub use basic::*;

/// No packet format has been configured yet
pub struct Uninitialized;

trait SealedPacketFormat {}
#[allow(async_fn_in_trait, private_bounds)]
pub trait PacketFormat: SealedPacketFormat {
    /// All the configuration paramters for the format
    type Config;

    /// All reception metadata specific for the format
    type RxMetaData: RxMetaData;
    /// All transmission metada specific for the format
    type TxMetaData;

    /// Configure the device to be in the correct packet format with the given config
    fn use_config<Spi, Sdn, Gpio, Delay>(
        device: &mut S2lp<Ready<Uninitialized>, Spi, Sdn, Gpio, Delay>,
        config: &Self::Config,
    ) -> Result<(), ErrorOf<S2lp<Ready<Uninitialized>, Spi, Sdn, Gpio, Delay>>>
    where
        Spi: SpiDevice,
        Sdn: OutputPin,
        Gpio: InputPin + Wait,
        Delay: DelayNs;

    /// Write the transmission metadata to the chip together with the packet len
    fn setup_packet_send<Spi, Sdn, Gpio, Delay>(
        device: &mut S2lp<Ready<Self>, Spi, Sdn, Gpio, Delay>,
        tx_meta_data: &Self::TxMetaData,
        payload_len: usize,
    ) -> Result<(), ErrorOf<S2lp<Ready<Self>, Spi, Sdn, Gpio, Delay>>>
    where
        Spi: SpiDevice,
        Sdn: OutputPin,
        Gpio: InputPin + Wait,
        Delay: DelayNs;
}

#[allow(async_fn_in_trait)]
pub(crate) trait RxMetaData: Debug + Clone {
    /// Read the metadata from the device
    fn read_from_device<I: RegisterInterface<AddressType = u8>>(
        device: &mut Device<I>,
    ) -> Result<Self, I::Error>
    where
        Self: Sized;
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

/// Setup the filters.
///
/// If none of the address filters are set, then no filtering will be done on the address and
/// all packets will be received.
pub struct PacketFilteringOptions {
    /// If true, packets with a bad CRC will be filtered out.
    /// Ignored if no CRC is enabled.
    pub discard_bad_crc: bool,
    /// The address of *this* device.
    ///
    /// If Some, the filtering will be turned on and packets with this destination address will not be discarded.
    pub source_address: Option<u8>,
    /// The address of the multicast group this device is part of.
    ///
    /// If Some, the filtering will be turned on and packets with this destination address will not be discarded.
    pub multicast_address: Option<u8>,
    /// The broadcast address.
    ///
    /// If Some, the filtering will be turned on and packets with this destination address will not be discarded.
    pub broadcast_address: Option<u8>,
}

impl PacketFilteringOptions {
    fn write_to_device<I: RegisterInterface<AddressType = u8>>(
        &self,
        device: &mut Device<I>,
    ) -> Result<(), I::Error> {
        device.pckt_flt_options().modify(|reg| {
            reg.set_crc_flt(self.discard_bad_crc);
            reg.set_dest_vs_broadcast_addr(self.broadcast_address.is_some());
            reg.set_dest_vs_multicast_addr(self.multicast_address.is_some());
            reg.set_dest_vs_source_addr(self.source_address.is_some());
        })?;

        device.pckt_flt_goals_2().write(|reg| {
            reg.set_broadcast_addr_or_dual_sync_2(self.broadcast_address.unwrap_or_default())
        })?;

        device.pckt_flt_goals_1().write(|reg| {
            reg.set_multicast_addr_or_dual_sync_1(self.multicast_address.unwrap_or_default())
        })?;

        device.pckt_flt_goals_0().write(|reg| {
            reg.set_tx_source_addr_or_dual_sync_0(self.source_address.unwrap_or_default())
        })?;

        device
            .protocol_1()
            .modify(|reg| reg.set_auto_pckt_flt(true))?;

        Ok(())
    }
}

impl Default for PacketFilteringOptions {
    fn default() -> Self {
        Self {
            discard_bad_crc: true,
            source_address: None,
            multicast_address: None,
            broadcast_address: None,
        }
    }
}
