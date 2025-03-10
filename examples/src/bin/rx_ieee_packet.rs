#![no_std]
#![no_main]

use defmt::unwrap;
use embassy_executor::Spawner;
use s2lp::{
    ll::CrcMode,
    packet_format::{Ieee802154G, Ieee802154GConfig, PreamblePattern},
    states::{rx::RxResult, shutdown::Config},
};
use stm32u0_examples::{init_board, Board};
use {defmt_rtt as _, panic_probe as _};

#[embassy_executor::main]
async fn main(_spawner: Spawner) -> ! {
    let Board { s2, .. } = init_board();

    let s2 = unwrap!(s2.init(Config::default()).await);

    let mut s2 = unwrap!(s2.set_format::<Ieee802154G>(&Ieee802154GConfig {
        preamble_length: 128,
        preamble_pattern: PreamblePattern::Pattern0,
        sync_length: 32,
        sync_pattern: 0x12345678,
        crc_mode: CrcMode::CrcPoly0X04C011Bb7,
        data_whitening: true,
    }));

    let mut index = 0;

    loop {
        let mut buf = [0; 128];
        let mut rx_s2 = unwrap!(s2.start_receive(&mut buf, Default::default()));
        let rx_result = unwrap!(rx_s2.wait().await);
        s2 = unwrap!(rx_s2.finish().ok());

        defmt::info!("{}: Wait is done: ({})", index, rx_result);
        index += 1;

        if let RxResult::Ok {
            packet_size,
            rssi_value,
            meta_data: _,
        } = rx_result
        {
            defmt::info!(
                "Received with rssi {}: {:a}",
                rssi_value,
                &buf[..packet_size]
            )
        }
    }
}
