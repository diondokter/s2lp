#![no_std]
#![no_main]

use defmt::unwrap;
use embassy_executor::Spawner;
use s2lp::states::shutdown::Config;
use stm32u0_examples::{init_board, Board};
use {defmt_rtt as _, panic_probe as _};

#[embassy_executor::main]
async fn main(_spawner: Spawner) -> ! {
    let Board { s2, .. } = init_board();

    let mut s2 = unwrap!(s2.init(Config::default()).await);

    let version = unwrap!(s2.ll().device_info_0().read()).version();
    let partnum = unwrap!(s2.ll().device_info_1().read()).partnum();

    defmt::info!("Version: {:X}, partnum: {:X}", version, partnum);
    defmt::assert_eq!(version, 0xC1);
    defmt::assert_eq!(partnum, 0x03);

    loop {
        cortex_m::asm::bkpt();
    }
}
