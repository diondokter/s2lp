#![no_std]
#![no_main]

use defmt::unwrap;
use embassy_executor::Spawner;
use s2lp::{
    ll::{CrcMode, LenWid},
    states::{ready::PreamblePattern, shutdown::Config},
};
use stm32u0_examples::{init_board, Board};
use {defmt_rtt as _, panic_probe as _};

#[embassy_executor::main]
async fn main(_spawner: Spawner) -> ! {
    let Board { s2, .. } = init_board().await;

    let s2 = unwrap!(s2.init(Config::default()).await);

    let mut s2 = unwrap!(
        s2.set_basic_format(
            128,
            PreamblePattern::Pattern0,
            32,
            0x12345678,
            true,
            LenWid::Bytes1,
            0,
            CrcMode::CrcPoly0X1021,
            Default::default(),
        )
        .await
    );

    loop {
        let mut tx_s2 = unwrap!(s2.send_packet(Some(0xAA), b"Hello from Rust!!").await);
        let tx_result = unwrap!(tx_s2.wait().await);
        s2 = unwrap!(tx_s2.finish().await.ok());

        defmt::info!("Packet has been sent! ({})", tx_result);

        embassy_time::Timer::after_millis(2000).await;
    }
}
