#![no_std]
#![no_main]

use defmt::unwrap;
use embassy_executor::Spawner;
use s2lp::{
    ll::CrcMode,
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
            false,
            0,
            CrcMode::NoCrc,
            Default::default(),
        )
        .await
    );

    let mut buf = [0; 128];
    let mut rx_s2 = unwrap!(s2.start_receive(&mut buf).await);
    let rx_result = unwrap!(rx_s2.wait().await);
    s2 = unwrap!(rx_s2.finish().await.ok());

    defmt::info!("Packet has been Received! ({:a})", rx_result);

    loop {
        cortex_m::asm::bkpt();
    }
}
