#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_stm32::exti::ExtiInput;
use embassy_stm32::gpio::{Input, Level, Output, Speed};
use embassy_stm32::spi::{Config, Spi, MODE_0};
use embassy_stm32::time::Hertz;
use {defmt_rtt as _, panic_probe as _};

#[embassy_executor::main]
async fn main(_spawner: Spawner) -> ! {
    info!("Hello World!");

    // Init the chip
    let mut config = embassy_stm32::Config::default();
    {
        use embassy_stm32::rcc::*;
        config.rcc.hsi = true;
        config.rcc.pll = Some(Pll {
            source: PllSource::HSI, // 16 MHz
            prediv: PllPreDiv::DIV1,
            mul: PllMul::MUL7, // 16 * 7 = 112 MHz
            divp: None,
            divq: None,
            divr: Some(PllRDiv::DIV2), // 112 / 2 = 56 MHz
        });
        config.rcc.sys = Sysclk::PLL1_R;
    }
    let p = embassy_stm32::init(config);

    // Init the spi
    let mut spi_config = Config::default();
    spi_config.mode = MODE_0;
    spi_config.frequency = Hertz(8_000_000);

    let spi = Spi::new(
        p.SPI1, p.PB3, p.PA7, p.PA6, p.DMA1_CH1, p.DMA1_CH2, spi_config,
    );
    let cs = Output::new(p.PA1, Level::High, Speed::VeryHigh);
    let shutdown = Output::new(p.PA8, Level::Low, Speed::VeryHigh);
    let gpio0 = ExtiInput::new(p.PA0, p.EXTI0, embassy_stm32::gpio::Pull::None);
    let _gpio1 = Input::new(p.PA2, embassy_stm32::gpio::Pull::None);
    let _gpio2 = Input::new(p.PA3, embassy_stm32::gpio::Pull::None);
    let _gpio3 = Input::new(p.PA5, embassy_stm32::gpio::Pull::None);

    let spi_device = unwrap!(embedded_hal_bus::spi::ExclusiveDevice::new(
        spi,
        cs,
        embassy_time::Delay
    ));

    // Init the radio
    let mut s2 = s2lp::S2lp::init(spi_device, shutdown, gpio0, embassy_time::Delay)
        .await
        .unwrap();
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
