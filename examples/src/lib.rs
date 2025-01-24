#![no_std]

use defmt::unwrap;
use embassy_stm32::exti::ExtiInput;
use embassy_stm32::gpio::{Input, Level, Output, Speed};
use embassy_stm32::mode::Async;
use embassy_stm32::spi::{Config, Spi, MODE_0};
use embassy_stm32::time::Hertz;
use embedded_hal_bus::spi::ExclusiveDevice;
use s2lp::states::Shutdown;
use s2lp::S2lp;
use {defmt_rtt as _, panic_probe as _};

pub fn init_board() -> Board {
    let p = init_chip();

    #[cfg(not(feature = "dk"))]
    let shutdown = Output::new(p.PA8, Level::Low, Speed::VeryHigh);
    #[cfg(feature = "dk")]
    let shutdown = Output::new(p.PD2, Level::Low, Speed::VeryHigh);
    let s2_gpio0 = ExtiInput::new(p.PA0, p.EXTI0, embassy_stm32::gpio::Pull::None);
    let s2_gpio1 = Input::new(p.PA2, embassy_stm32::gpio::Pull::None);
    let s2_gpio2 = Input::new(p.PA3, embassy_stm32::gpio::Pull::None);
    let s2_gpio3 = Input::new(p.PA5, embassy_stm32::gpio::Pull::None);

    let mut spi_config = Config::default();
    spi_config.mode = MODE_0;
    spi_config.frequency = Hertz(8_000_000);

    let spi = Spi::new(
        p.SPI1, p.PB3, p.PA7, p.PA6, p.DMA1_CH1, p.DMA1_CH2, spi_config,
    );
    let cs = Output::new(p.PA1, Level::High, Speed::VeryHigh);

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

    defmt::info!("Init done");

    Board {
        s2,
        s2_gpio1,
        s2_gpio2,
        s2_gpio3,
    }
}

pub fn init_board_lp() -> BoardLp {
    let p = init_chip();

    #[cfg(feature = "low-power")]
    {
        let rtc = embassy_stm32::rtc::Rtc::new(p.RTC, embassy_stm32::rtc::RtcConfig::default());
        static RTC: static_cell::StaticCell<embassy_stm32::rtc::Rtc> = static_cell::StaticCell::new();
        let rtc = RTC.init(rtc);
        embassy_stm32::low_power::stop_with_rtc(rtc);
    }

    #[cfg(not(feature = "dk"))]
    let shutdown = Output::new(p.PA8, Level::Low, Speed::VeryHigh);
    #[cfg(feature = "dk")]
    let shutdown = Output::new(p.PD2, Level::Low, Speed::VeryHigh);
    let s2_gpio0 = ExtiInput::new(p.PA0, p.EXTI0, embassy_stm32::gpio::Pull::Down);
    let s2_gpio1 = Input::new(p.PA2, embassy_stm32::gpio::Pull::None);
    let s2_gpio2 = Input::new(p.PA3, embassy_stm32::gpio::Pull::None);
    let s2_gpio3 = Input::new(p.PA5, embassy_stm32::gpio::Pull::None);

    let mut spi_config = Config::default();
    spi_config.mode = MODE_0;
    spi_config.frequency = Hertz(8_000_000);

    let spi = LpSpi {
        peri: p.SPI1,
        sck: p.PB3,
        miso: p.PA6,
        mosi: p.PA7,
        tx_dma: p.DMA1_CH1,
        rx_dma: p.DMA1_CH2,
        config: spi_config,
        cs: p.PA1,
    };

    defmt::info!("Init done");

    BoardLp {
        spi,
        sdn: shutdown,
        s2_gpio0,
        s2_gpio1,
        s2_gpio2,
        s2_gpio3,
    }
}

fn init_chip() -> embassy_stm32::Peripherals {
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

        if cfg!(feature = "low-power") {
            config.enable_debug_during_sleep = false;
        }
    }

    embassy_stm32::init(config)
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

pub struct BoardLp {
    pub spi: LpSpi,
    pub sdn: Output<'static>,
    pub s2_gpio0: ExtiInput<'static>,
    pub s2_gpio1: Input<'static>,
    pub s2_gpio2: Input<'static>,
    pub s2_gpio3: Input<'static>,
}

pub struct LpSpi {
    peri: embassy_stm32::peripherals::SPI1,
    sck: embassy_stm32::peripherals::PB3,
    miso: embassy_stm32::peripherals::PA6,
    mosi: embassy_stm32::peripherals::PA7,
    tx_dma: embassy_stm32::peripherals::DMA1_CH1,
    rx_dma: embassy_stm32::peripherals::DMA1_CH2,
    config: embassy_stm32::spi::Config,
    cs: embassy_stm32::peripherals::PA1,
}

impl LpSpi {
    pub fn get_spi<'s>(
        &'s mut self,
    ) -> ExclusiveDevice<Spi<'s, Async>, Output<'s>, embassy_time::Delay> {
        ExclusiveDevice::new(
            Spi::new(
                &mut self.peri,
                &mut self.sck,
                &mut self.mosi,
                &mut self.miso,
                &mut self.tx_dma,
                &mut self.rx_dma,
                self.config,
            ),
            Output::new(&mut self.cs, Level::High, Speed::VeryHigh),
            embassy_time::Delay,
        )
        .unwrap()
    }
}
