#![no_std]
#![no_main]

use defmt::unwrap;
use embassy_executor::Spawner;
use s2lp::{
    ll::{CrcMode, LenWid},
    packet_format::{Basic, BasicConfig, PacketFilteringOptions, PreamblePattern},
    states::{
        rx::{RxResult, RxTimeout},
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
        s2.set_format::<Basic>(&BasicConfig {
            preamble_length: 128,
            preamble_pattern: PreamblePattern::Pattern0,
            sync_length: 32,
            sync_pattern: 0x12345678,
            include_address: true,
            packet_length_encoding: LenWid::Bytes1,
            postamble_length: 0,
            crc_mode: CrcMode::CrcPoly0X1021,
            packet_filter: PacketFilteringOptions {
                source_address: Some(0xAA),
                ..Default::default()
            },
        })
        .await
    );

    let mut index = 0;

    embassy_time::Timer::after_millis(1000).await;

    loop {
        let mut buf = [0; 128];
        let mut rx_s2 = unwrap!(
            s2.start_receive(
                &mut buf,
                s2lp::states::rx::RxMode::Normal {
                    timeout: Some(RxTimeout {
                        timeout_us: 1_000_000,
                        mask: Default::default(),
                    })
                }
            )
            .await
        );
        let rx_result = unwrap!(rx_s2.wait().await);
        s2 = unwrap!(rx_s2.finish().await.ok());

        defmt::info!("{}: Wait is done: ({})", index, rx_result);
        index += 1;

        if let RxResult::Ok {
            packet_size,
            rssi_value,
            meta_data,
        } = rx_result
        {
            defmt::info!(
                "Received from {} with rssi {}: {:a}",
                meta_data.destination_address,
                rssi_value,
                &buf[..packet_size]
            )
        }
    }
}
