//! Low power RX

#![no_std]
#![no_main]

use defmt::unwrap;
use embassy_executor::Spawner;
use embassy_stm32::low_power::Executor;
use s2lp::S2lp;
use s2lp::{
    ll::{CrcMode, LenWid},
    packet_format::{Basic, BasicConfig, PacketFilteringOptions, PreamblePattern},
    states::{rx::RxResult, shutdown::Config},
};
use stm32u0_examples::{init_board_lp, BoardLp};
use {defmt_rtt as _, panic_probe as _};

#[cortex_m_rt::entry]
fn main() -> ! {
    Executor::take().run(|spawner| {
        unwrap!(spawner.spawn(async_main(spawner)));
    });
}

#[embassy_executor::task]
async fn async_main(_spawner: Spawner) -> ! {
    let BoardLp {
        mut spi,
        sdn,
        s2_gpio0,
        ..
    } = init_board_lp();

    let mut s2_shutdown = S2lp::new(
        spi.get_spi(),
        sdn,
        s2_gpio0,
        s2lp::GpioNumber::Gpio0,
        embassy_time::Delay,
    );
    loop {
        let s2 = unwrap!(s2_shutdown.init(Config::default()).await);

        let mut s2 = unwrap!(s2.set_format::<Basic>(&BasicConfig {
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
        }));

        let mut buf = [0; 128];
        let rx_s2 = unwrap!(s2.start_receive(&mut buf, Default::default()));

        let (mut rx_s2_no_spi, _) = rx_s2.take_spi();
        unwrap!(rx_s2_no_spi.wait_for_irq().await);
        let mut rx_s2 = rx_s2_no_spi.give_spi(spi.get_spi());

        let rx_result = unwrap!(rx_s2.wait().await);
        s2 = unwrap!(rx_s2.finish().ok());

        defmt::info!("Wait is done: ({})", rx_result);

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
            );
        }

        let s2 = s2.shutdown().unwrap();
        let (s2_no_spi, _) = s2.take_spi();
        embassy_time::Timer::after_secs(7).await;
        s2_shutdown = s2_no_spi.give_spi(spi.get_spi());
    }
}
