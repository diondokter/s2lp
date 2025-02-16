use core::fmt::Debug;

use device_driver::RegisterInterface;
use embedded_hal::{
    digital::{InputPin, OutputPin},
    spi::SpiDevice,
};
use embedded_hal_async::{delay::DelayNs, digital::Wait};

use crate::{
    ll::{Device, LenWid},
    packet_format::PacketFilteringOptions,
    states::Ready,
    Error, ErrorOf, S2lp,
};

use super::{
    CrcMode, PacketFormat, PreamblePattern, RxMetaData, SealedPacketFormat, Uninitialized,
};

/// The Ieee802154G packet format
pub struct Ieee802154G;

impl SealedPacketFormat for Ieee802154G {}
impl PacketFormat for Ieee802154G {
    type Config = Ieee802154GConfig;
    type RxMetaData = Ieee802154GRxMetaData;
    type TxMetaData = Ieee802154GTxMetaData;

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
        assert!(
            matches!(
                config.crc_mode,
                CrcMode::NoCrc | CrcMode::CrcPoly0X1021 | CrcMode::CrcPoly0X04C011Bb7
            ),
            "Unsupported CRC mode selected"
        );

        device.ll().pckt_ctrl_6().write(|reg| {
            reg.set_preamble_len(config.preamble_length);
            reg.set_sync_len(config.sync_length)
        })?;

        // Frame length is always 11 bits
        device.ll().pckt_ctrl_4().write(|reg| {
            reg.set_len_wid(LenWid::Bytes2);
            reg.set_address_len(true);
        })?;

        device.ll().pckt_ctrl_3().write(|reg| {
            reg.set_pckt_frmt(crate::ll::PacketFormat::Ieee802154G);
            reg.set_preamble_sel(config.preamble_pattern as u8);
        })?;

        device
            .ll()
            .pckt_ctrl_2()
            .write(|reg| reg.set_fix_var_len(crate::ll::FixVarLen::Variable))?;

        device.ll().pckt_ctrl_1().write(|reg| {
            reg.set_crc_mode(config.crc_mode);
            reg.set_whit_en(config.data_whitening);
        })?;

        device
            .ll()
            .sync()
            .write(|reg| reg.set_value(config.sync_pattern.to_be()))?;

        PacketFilteringOptions {
            discard_bad_crc: true,
            source_address: None,
            multicast_address: None,
            broadcast_address: None,
        }
        .write_to_device(device.ll())?;

        Ok(())
    }

    fn setup_packet_send<Spi, Sdn, Gpio, Delay>(
        device: &mut S2lp<Ready<Self>, Spi, Sdn, Gpio, Delay>,
        _tx_meta_data: &Self::TxMetaData,
        payload_len: usize,
    ) -> Result<(), ErrorOf<S2lp<Ready<Self>, Spi, Sdn, Gpio, Delay>>>
    where
        Spi: SpiDevice,
        Sdn: OutputPin,
        Gpio: InputPin + Wait,
        Delay: DelayNs,
    {
        let crc_len = device.ll().pckt_ctrl_1().read()?.crc_mode()?.num_bytes();

        if payload_len + crc_len >= 2048 {
            return Err(Error::BufferTooLarge);
        }

        // Set the packet lenght
        device
            .ll()
            .pckt_len()
            .write(|reg| reg.set_value(payload_len as u16 + crc_len as u16))?;

        Ok(())
    }
}

/// Configuration for the Ieee802154G packet format
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt-03", derive(defmt::Format))]
pub struct Ieee802154GConfig {
    pub preamble_length: u16, // 0-2046
    pub preamble_pattern: PreamblePattern,
    pub sync_length: u8, // 0-32
    pub sync_pattern: u32,
    pub crc_mode: CrcMode, // Only mode 0, 3 or 5
    /// Only relevant for TX as RX reads the bit from the PHR
    pub data_whitening: bool,
}

/// Receiver metadata for the Ieee802154G packet format
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt-03", derive(defmt::Format))]
pub struct Ieee802154GRxMetaData;

impl RxMetaData for Ieee802154GRxMetaData {
    fn read_from_device<I: RegisterInterface<AddressType = u8>>(
        _device: &mut Device<I>,
    ) -> Result<Self, I::Error>
    where
        Self: Sized,
    {
        Ok(Self)
    }
}

/// Transmission metadata for the Ieee802154G packet format
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt-03", derive(defmt::Format))]
pub struct Ieee802154GTxMetaData;
