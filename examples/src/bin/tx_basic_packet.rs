#![no_std]
#![no_main]

use embassy_executor::Spawner;
use s2lp::{ll::CrcMode, states::ready::PreamblePattern};
use stm32u0_examples::{init_board, Board};
use {defmt_rtt as _, panic_probe as _};

#[embassy_executor::main]
async fn main(_spawner: Spawner) -> ! {
    let Board { s2, .. } = init_board().await;

    let mut s2 = s2.init().await.unwrap();

    let mut s2 = s2
        .set_basic_format(
            128,
            PreamblePattern::Pattern0,
            32,
            0x12345678,
            false,
            0,
            CrcMode::NoCrc,
        )
        .await
        .unwrap();

    let mut tx_s2 = s2.send_packet(0xAA, b"Hello from S2!!").await.unwrap();

    loop {
        cortex_m::asm::bkpt();
    }
}
