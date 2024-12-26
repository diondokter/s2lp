#![no_std]

use defmt::*;
use embassy_stm32::exti::ExtiInput;
use embassy_stm32::gpio::{Input, Level, Output, Speed};
use embassy_stm32::spi::{Config, Spi, MODE_0};
use embassy_stm32::time::Hertz;
use embedded_hal_bus::spi::ExclusiveDevice;
use s2lp::states::Shutdown;
use s2lp::S2lp;
use {defmt_rtt as _, panic_probe as _};

pub async fn init_board() -> Board {
    defmt::info!("Initializing microcontroller");

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
    #[cfg(not(feature = "dk"))]
    let shutdown = Output::new(p.PA8, Level::Low, Speed::VeryHigh);
    #[cfg(feature = "dk")]
    let shutdown = Output::new(p.PD2, Level::Low, Speed::VeryHigh);
    let s2_gpio0 = ExtiInput::new(p.PA0, p.EXTI0, embassy_stm32::gpio::Pull::None);
    let s2_gpio1 = Input::new(p.PA2, embassy_stm32::gpio::Pull::None);
    let s2_gpio2 = Input::new(p.PA3, embassy_stm32::gpio::Pull::None);
    let s2_gpio3 = Input::new(p.PA5, embassy_stm32::gpio::Pull::None);

    let spi_device = unwrap!(embedded_hal_bus::spi::ExclusiveDevice::new(
        spi,
        cs,
        embassy_time::Delay
    ));

    // Init the radio
    let s2 = s2lp::S2lp::new(
        spi_device,
        shutdown,
        s2_gpio0,
        s2lp::GpioNumber::Gpio0,
        embassy_time::Delay,
    );

    defmt::info!("Initializing done");

    Board {
        s2,
        s2_gpio1,
        s2_gpio2,
        s2_gpio3,
    }
}

pub struct Board {
    pub s2: S2lp<
        Shutdown,
        ExclusiveDevice<
            embassy_stm32::spi::Spi<'static, embassy_stm32::mode::Async>,
            Output<'static>,
            embassy_time::Delay,
        >,
        Output<'static>,
        ExtiInput<'static>,
        embassy_time::Delay,
    >,
    pub s2_gpio1: Input<'static>,
    pub s2_gpio2: Input<'static>,
    pub s2_gpio3: Input<'static>,
}
