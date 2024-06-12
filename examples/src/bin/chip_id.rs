#![no_std]
#![no_main]

use embassy_executor::Spawner;
use stm32u0_examples::{init_board, Board};
use {defmt_rtt as _, panic_probe as _};

#[embassy_executor::main]
async fn main(_spawner: Spawner) -> ! {
    let Board { s2, .. } = init_board().await;

    let mut s2 = s2.init().await.unwrap();

    let version = s2
        .ll()
        .device_info_0()
        .read_async()
        .await
        .unwrap()
        .version();
    let partnum = s2
        .ll()
        .device_info_1()
        .read_async()
        .await
        .unwrap()
        .partnum();

    defmt::info!("Version: {:X}, partnum: {:X}", version, partnum);
    defmt::assert_eq!(version, 0xC1);
    defmt::assert_eq!(partnum, 0x03);

    loop {
        cortex_m::asm::bkpt();
    }
}
