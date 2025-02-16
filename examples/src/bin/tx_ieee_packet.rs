#![no_std]
#![no_main]

use defmt::unwrap;
use embassy_executor::Spawner;
use s2lp::{
    ll::CrcMode,
    packet_format::{Ieee802154G, Ieee802154GConfig, Ieee802154GTxMetaData, PreamblePattern},
    states::shutdown::Config,
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

    // Optional CSMA/CA (default is off)
    unwrap!(s2.set_csma_ca(s2lp::states::ready::CsmaCaMode::Backoff {
        cca_period: s2lp::ll::CcaPeriod::Bits64,
        num_cca_periods: 2,
        max_backoffs: 7,
        backoff_prescaler: 2,
        custom_prng_seed: None,
    }));

    loop {
        let mut tx_s2 = unwrap!(s2.send_packet(&Ieee802154GTxMetaData, b"\0\0Hello from Rust!!"));
        let tx_result = unwrap!(tx_s2.wait().await);
        s2 = unwrap!(tx_s2.finish().ok());

        defmt::info!("Packet has been sent! ({})", tx_result);

        embassy_time::Timer::after_millis(2000).await;
    }
}
