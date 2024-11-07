#![no_std]
#![no_main]

use defmt::unwrap;
use embassy_executor::Spawner;
use s2lp::{
    ll::{CrcMode, LenWid},
    states::{
        ready::{PacketFilteringOptions, PreamblePattern},
        rx::RxResult,
        shutdown::Config,
    },
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
            PacketFilteringOptions {
                source_address: Some(0xAA),
                ..Default::default()
            },
        )
        .await
    );

    let mut index = 0;

    loop {
        let mut buf = [0; 128];
        let mut rx_s2 = unwrap!(s2.start_receive(&mut buf).await);
        let rx_result = unwrap!(rx_s2.wait().await);
        s2 = unwrap!(rx_s2.finish().await.ok());

        defmt::info!("Packet {} has been Received! ({:a})", index, rx_result);
        index += 1;

        if let RxResult::Ok { packet_size, .. } = rx_result {
            defmt::info!("Received: {:a}", &buf[..packet_size])
        }
    }
}
