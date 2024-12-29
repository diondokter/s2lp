//! Module containing all packet format handling and setup

use core::fmt::Debug;

use device_driver::RegisterInterface;
use embedded_hal::{
    digital::{InputPin, OutputPin},
    spi::SpiDevice,
};
use embedded_hal_async::{delay::DelayNs, digital::Wait};

use crate::{
    ll::{Device, LenWid},
    states::Ready,
    Error, ErrorOf, S2lp,
};

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

/// The basic packet format
pub struct Basic;

impl SealedPacketFormat for Basic {}
impl PacketFormat for Basic {
    type Config = BasicConfig;
    type RxMetaData = BasicRxMetaData;
    type TxMetaData = BasicTxMetaData;

    fn use_config<Spi, Sdn, Gpio, Delay>(
        device: &mut S2lp<Ready<Uninitialized>, Spi, Sdn, Gpio, Delay>,
        config: &Self::Config,
    ) -> Result<(), ErrorOf<S2lp<Ready<Uninitialized>, Spi, Sdn, Gpio, Delay>>>
    where
        Spi: SpiDevice,
        Sdn: OutputPin,
        Gpio: InputPin + Wait,
        Delay: DelayNs,
    {
        device.ll().pckt_ctrl_6().write(|reg| {
            reg.set_preamble_len(config.preamble_length);
            reg.set_sync_len(config.sync_length)
        })?;

        device.ll().pckt_ctrl_4().write(|reg| {
            reg.set_address_len(config.include_address);
            reg.set_len_wid(config.packet_length_encoding);
        })?;

        device.ll().pckt_ctrl_3().write(|reg| {
            reg.set_pckt_frmt(crate::ll::PacketFormat::Basic);
            reg.set_preamble_sel(config.preamble_pattern as u8);
        })?;

        device
            .ll()
            .pckt_ctrl_2()
            .write(|reg| reg.set_fix_var_len(crate::ll::FixVarLen::Variable))?;

        device.ll().pckt_ctrl_1().write(|reg| {
            reg.set_crc_mode(config.crc_mode);
        })?;

        device
            .ll()
            .sync()
            .write(|reg| reg.set_value(config.sync_pattern.to_be()))?;

        device
            .ll()
            .pckt_pstmbl()
            .write(|reg| reg.set_value(config.postamble_length))?;

        config.packet_filter.write_to_device(device.ll())?;

        Ok(())
    }

    fn setup_packet_send<Spi, Sdn, Gpio, Delay>(
        device: &mut S2lp<Ready<Self>, Spi, Sdn, Gpio, Delay>,
        tx_meta_data: &Self::TxMetaData,
        payload_len: usize,
    ) -> Result<(), ErrorOf<S2lp<Ready<Self>, Spi, Sdn, Gpio, Delay>>>
    where
        Spi: SpiDevice,
        Sdn: OutputPin,
        Gpio: InputPin + Wait,
        Delay: DelayNs,
    {
        let pckt_ctrl_4 = device.ll().pckt_ctrl_4().read()?;
        let address_included = pckt_ctrl_4.address_len();
        let max_packet_len = match pckt_ctrl_4.len_wid() {
            LenWid::Bytes1 => u8::MAX as u16,
            LenWid::Bytes2 => u16::MAX,
        };

        if payload_len > (max_packet_len - address_included as u16) as usize {
            return Err(Error::BufferTooLarge);
        }

        if address_included != tx_meta_data.destination_address.is_some() {
            return Err(Error::BadConfig {
                reason: "Given address different from config",
            });
        }

        // Set the packet lenght
        device
            .ll()
            .pckt_len()
            .write(|reg| reg.set_value(payload_len as u16 + address_included as u16))?;

        // Set the destination address
        if let Some(destination_address) = tx_meta_data.destination_address {
            device
                .ll()
                .pckt_flt_goals_3()
                .write(|reg| reg.set_rx_source_addr_or_dual_sync_3(destination_address))?;
        }

        Ok(())
    }
}

/// Configuration for the Basic packet format
pub struct BasicConfig {
    pub preamble_length: u16, // 0-2046
    pub preamble_pattern: PreamblePattern,
    pub sync_length: u8, // 0-32
    pub sync_pattern: u32,
    pub include_address: bool,
    pub packet_length_encoding: LenWid,
    pub postamble_length: u8, // In pairs of `01`'s
    pub crc_mode: CrcMode,
    pub packet_filter: PacketFilteringOptions,
}

/// Receiver metadata for the Basic packet format
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt-03", derive(defmt::Format))]
pub struct BasicRxMetaData {
    /// The received packet destination address (if any)
    pub destination_address: Option<u8>,
}

impl RxMetaData for BasicRxMetaData {
    fn read_from_device<I: RegisterInterface<AddressType = u8>>(
        device: &mut Device<I>,
    ) -> Result<Self, I::Error>
    where
        Self: Sized,
    {
        let destination_address = if device.pckt_ctrl_4().read()?.address_len() {
            Some(device.rx_addre_field_0().read()?.value())
        } else {
            None
        };

        Ok(Self {
            destination_address,
        })
    }
}

/// Transmission metadata for the Basic packet format
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt-03", derive(defmt::Format))]
pub struct BasicTxMetaData {
    /// The destination address of the packet (if any)
    pub destination_address: Option<u8>,
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
