use embedded_hal::{
    digital::{InputPin, OutputPin},
    spi::SpiDevice,
};
use embedded_hal_async::{delay::DelayNs, digital::Wait};

use crate::{
    ll::{Device, DeviceInterface, GpioSelectOutput, SleepModeSel, State},
    packet_format::Uninitialized,
    states::addressable::GpioFunction,
    Error, ErrorOf, GpioNumber, S2lp,
};

use super::{Ready, Shutdown};

impl<Spi, Sdn, Gpio, Delay> S2lp<Shutdown, Spi, Sdn, Gpio, Delay>
where
    Spi: SpiDevice,
    Sdn: OutputPin,
    Gpio: InputPin + Wait,
    Delay: DelayNs,
{
    /// Create a new instance of the driver.
    ///
    /// The driver requires one of the gpio pins for interrupt management.
    /// The pin and its number are given as arguments.
    ///
    /// If gpio pin 0 is used, the init procedure will be faster since it gives
    /// a power-on-reset signal by default. If another pin is given, the worst case
    /// startup delay is used to allow the radio to boot.
    pub const fn new(
        spi: Spi,
        shutdown_pin: Sdn,
        gpio_pin: Gpio,
        gpio_number: GpioNumber,
        delay: Delay,
    ) -> Self {
        Self {
            device: Some(Device::new(DeviceInterface::new(spi))),
            shutdown_pin,
            gpio_pin,
            gpio_number,
            delay,
            state: Shutdown,
        }
    }

    /// Initialize the radio chip
    pub async fn init(
        mut self,
        config: Config,
    ) -> Result<S2lp<Ready<Uninitialized>, Spi, Sdn, Gpio, Delay>, ErrorOf<Self>> {
        if !is_frequency_band(config.base_frequency) {
            return Err(Error::BadConfig {
                reason: "Base frequency out of range",
            });
        }
        if !is_datarate(config.datarate, config.xtal_frequency) {
            return Err(Error::BadConfig {
                reason: "Datarate out of range",
            });
        }
        if !is_f_dev(config.frequency_deviation, config.xtal_frequency) {
            return Err(Error::BadConfig {
                reason: "Frequency deviation out of range",
            });
        }

        #[cfg(feature = "defmt-03")]
        defmt::debug!("Resetting the radio");

        self.shutdown_pin.set_high().map_err(Error::Sdn)?;
        self.delay.delay_us(1).await;
        self.shutdown_pin.set_low().map_err(Error::Sdn)?;

        if self.gpio_number == GpioNumber::Gpio0 {
            #[cfg(feature = "defmt-03")]
            defmt::trace!("Waiting for POR");
            self.gpio_pin.wait_for_high().await.map_err(Error::Gpio)?;
        } else {
            #[cfg(feature = "defmt-03")]
            defmt::trace!("Waiting for reset delay");
            self.delay.delay_ms(2).await;
        }

        let mut this = self.cast_state(Ready::new(0));

        #[cfg(feature = "defmt-03")]
        defmt::trace!("Checking interface works");
        let version = this.ll().device_info_0().read()?.version();
        if version != 0xC1 {
            return Err(Error::Init);
        }

        #[cfg(feature = "defmt-03")]
        defmt::trace!("Setting correct radio config");
        // Set the gpio pin to irq mode since we use IRQs in the driver
        this.set_gpio_function(
            this.gpio_number,
            GpioFunction::Output {
                high_power: false,
                select: GpioSelectOutput::Irq,
            },
        )?;

        // Datasheet 4.7 - Setting up the crystal oscillator
        // If the xtal_frequency is slow, then we can drive the chip from it directly.
        // If it is fast, we need to enable the clock divider.
        let digital_frequency = {
            let mut pd_clkdiv = this.ll().xo_rco_conf_1().read()?.pd_clkdiv();

            if (config.xtal_frequency < DIG_DOMAIN_XTAL_THRESH && !pd_clkdiv)
                || (config.xtal_frequency > DIG_DOMAIN_XTAL_THRESH && pd_clkdiv)
            {
                // Go to standby
                this.ll().standby().dispatch()?;
                while this.ll().mc_state_0().read()?.state()? != State::Standby {}

                // Invert the pd_clkdiv
                pd_clkdiv = !pd_clkdiv;
                this.ll()
                    .xo_rco_conf_1()
                    .modify(|reg| reg.set_pd_clkdiv(pd_clkdiv))?;

                // Go to ready
                this.ll().ready().dispatch()?;
                while this.ll().mc_state_0().read()?.state()? != State::Ready {}
            }

            config.xtal_frequency / if pd_clkdiv { 1 } else { 2 }
        };

        this.state.digital_frequency = digital_frequency;

        // Datasheet 5.7 part 1
        // The clock divider is now ok, so we can turn the rco calibration on.
        // Later we must check whether it succeeded.
        this.ll()
            .xo_rco_conf_0()
            .modify(|reg| reg.set_rco_calibration(true))?;

        if !is_ch_bw(config.bandwidth, digital_frequency) {
            return Err(Error::BadConfig {
                reason: "Bandwidth out of range",
            });
        }

        // Datasheet 5.5.5 - Set the Intermediate Frequency (IF) to the recommended value
        {
            const IF: u64 = 300_000;
            this.ll().if_offset_ana().write(|reg| {
                reg.set_value(((IF << 13) * 3 / config.xtal_frequency as u64 - 100) as u8)
            })?;
            this.ll().if_offset_dig().write(|reg| {
                reg.set_value(((IF << 13) * 3 / digital_frequency as u64 - 100) as u8)
            })?;
        }

        // Datasheet 5.4.5 - Configure the datarate
        // We search for the smallest exponent where our datarate fits (for highest resolution)
        {
            let mut used_exponent = 0;
            for exponent in 0..15 {
                if compute_datarate(digital_frequency, u16::MAX, exponent) > config.datarate {
                    used_exponent = exponent;
                    break;
                }
            }

            // Now calculate the best mantissa including rounding
            let used_mantissa = if used_exponent == 0 {
                let target = (config.datarate as u64) << 32;
                (target + (digital_frequency as u64 / 2)) / digital_frequency as u64
            } else {
                let target = (config.datarate as u64) << (33 - used_exponent as u64);
                (target + (digital_frequency as u64 / 2)) / digital_frequency as u64 - 65536
            } as u16;

            #[cfg(feature = "defmt-03")]
            defmt::trace!(
                "Selected datarate. Target: {}, found: {}",
                config.datarate,
                compute_datarate(digital_frequency, used_mantissa, used_exponent)
            );

            this.ll()
                .mod_4()
                .write(|reg| reg.set_value(used_mantissa))?;
            this.ll().mod_2().write(|reg| {
                reg.set_datarate_e(used_exponent);
                reg.set_modulation_type(config.modulation);
            })?;
        }

        // Datasheet 5.3.1
        {
            this.ll()
                .synt()
                .modify(|reg| reg.set_bs(is_frequency_band_middle(config.base_frequency)))?;
        }

        // Datasheet 5.4.1 - Configure the frequency modulation
        {
            let band_factor = get_band_factor(config.base_frequency);

            let refdiv = if this.ll().xo_rco_conf_0().read()?.refdiv() {
                2
            } else {
                1
            };

            // Search for the smallest exponent that our fdev fits in for the highest resolution
            let mut used_exponent = 0;
            for exponent in 0..16 {
                let fdev = compute_fdev(
                    config.xtal_frequency,
                    u8::MAX,
                    exponent,
                    band_factor,
                    refdiv,
                );

                if fdev > config.frequency_deviation {
                    used_exponent = exponent;
                    break;
                }
            }

            let mut used_mantissa = u8::MAX;
            let mut prev_fdev = 0;
            for mantissa in (0..=u8::MAX).rev() {
                let fdev = compute_fdev(
                    config.xtal_frequency,
                    mantissa,
                    used_exponent,
                    band_factor,
                    refdiv,
                );

                if fdev < config.frequency_deviation {
                    used_mantissa = if config.frequency_deviation.abs_diff(fdev)
                        < config.frequency_deviation.abs_diff(prev_fdev)
                    {
                        #[cfg(feature = "defmt-03")]
                        defmt::trace!(
                            "Selected frequency deviation. Target: {}, found: {}",
                            config.frequency_deviation,
                            fdev
                        );
                        mantissa
                    } else {
                        #[cfg(feature = "defmt-03")]
                        defmt::trace!(
                            "Selected frequency deviation. Target: {}, found: {}",
                            config.frequency_deviation,
                            prev_fdev
                        );
                        mantissa + 1
                    };
                    break;
                } else {
                    prev_fdev = fdev;
                }
            }

            this.ll()
                .mod_1()
                .modify(|reg| reg.set_fdev_e(used_exponent))?;
            this.ll()
                .mod_0()
                .write(|reg| reg.set_fdev_m(used_mantissa))?;
        }

        // Set the bandwidth
        this.ll().ch_flt().write(|reg| {
            *reg = search_channel_filter_bandwidth(config.bandwidth, digital_frequency);
        })?;

        // Set the OOK smoothing
        let is_ook = matches!(config.modulation, ModulationType::AskOok);
        this.ll()
            .pa_power_0()
            .modify(|reg| reg.set_dig_smooth_en(is_ook))?;
        this.ll()
            .pa_config_1()
            .modify(|reg| reg.set_fir_en(is_ook))?;

        this.ll().pa_config_0().modify(|reg| {
            reg.set_pa_fc(match config.datarate {
                ..16000 => crate::ll::PaFc::Khz12P5,
                16000..32000 => crate::ll::PaFc::Khz25,
                32000..62500 => crate::ll::PaFc::Khz50,
                62500.. => crate::ll::PaFc::Khz100,
            })
        })?;

        // Enable AFC freeze on SYNC
        this.ll()
            .afc_2()
            .modify(|reg| reg.set_afc_freeze_on_sync(true))?;

        // Set the synt word (base frequency) and charge pump
        {
            let band_factor = get_band_factor(config.base_frequency);

            let refdiv = if this.ll().xo_rco_conf_0().read()?.refdiv() {
                2
            } else {
                1
            };

            let synt_target =
                ((config.base_frequency as u64) << 20) * (band_factor / 2) as u64 * refdiv as u64;
            let synt = ((synt_target + config.xtal_frequency as u64 / 2)
                / config.xtal_frequency as u64) as u32;

            let vco_freq = config.base_frequency as u64 * band_factor as u64;
            let f_ref = config.xtal_frequency / refdiv;

            let (cp_isel, pfd_split) = match (vco_freq, f_ref) {
                (VCO_CENTER_FREQ.., DIG_DOMAIN_XTAL_THRESH..) => (0x02, false),
                (VCO_CENTER_FREQ.., ..DIG_DOMAIN_XTAL_THRESH) => (0x01, true),
                (..VCO_CENTER_FREQ, DIG_DOMAIN_XTAL_THRESH..) => (0x03, false),
                (..VCO_CENTER_FREQ, ..DIG_DOMAIN_XTAL_THRESH) => (0x02, true),
            };

            this.ll()
                .synth_config_2()
                .modify(|reg| reg.set_pll_pfd_split_en(pfd_split))?;
            this.ll().synt().modify(|reg| {
                reg.set_synt(synt);
                reg.set_pll_cp_isel(cp_isel);
            })?;
        }

        // Datasheet 5.7 part 2
        loop {
            // Wait for the RCO calibration to finish
            let mc_state_1 = this.ll().mc_state_1().read()?;
            if mc_state_1.rco_cal_ok() {
                break;
            } else if mc_state_1.error_lock() {
                return Err(Error::RcoLockError);
            }
        }

        // Retain fifo on sleep. Required for CSMA/CA to work
        this.ll()
            .pm_conf_0()
            .write(|reg| reg.set_sleep_mode_sel(SleepModeSel::WithFifoRetention))?;

        #[cfg(feature = "defmt-03")]
        defmt::debug!("Init done!");

        Ok(this)
    }
}

pub use crate::ll::ModulationType;

/// The radio configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    /// The frequency of the crystal oscillator
    pub xtal_frequency: u32,
    /// Specifies the carrier frequency of channel 0 in Hz.
    ///
    /// Possible values:
    /// - High band (860 MHz - 940 MHz)
    /// - Middle band (430 MHz - 470 MHz)
    pub base_frequency: u32,
    /// The modulation the radio will use
    pub modulation: ModulationType,
    /// The datarate used in bps (100 bps - 500 kbps)
    pub datarate: u32,
    /// Frequency deviation in Hz. This is used for (G)FSK.
    ///
    /// - Min: `F_Xo * 8 / 0x40000`
    /// - Max: `F_Xo * 7680 / 0x40000 `
    pub frequency_deviation: u32,
    /// Channel (filter) bandwidth in Hz between 1100 Hz - 800100 Hz
    pub bandwidth: u32,
    // TODO:
    // pub pa_info: PaInfo,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            xtal_frequency: 50_000_000,
            base_frequency: 868_000_000,
            modulation: ModulationType::Fsk2,
            datarate: 38_400,
            frequency_deviation: 20_000,
            bandwidth: 100_000,
        }
    }
}

const fn is_frequency_band(base_frequency: u32) -> bool {
    is_frequency_band_high(base_frequency) || is_frequency_band_middle(base_frequency)
}

const fn is_frequency_band_high(base_frequency: u32) -> bool {
    base_frequency >= HIGH_BAND_LOWER_LIMIT && base_frequency <= HIGH_BAND_UPPER_LIMIT
}

const fn is_frequency_band_middle(base_frequency: u32) -> bool {
    base_frequency >= MIDDLE_BAND_LOWER_LIMIT && base_frequency <= MIDDLE_BAND_UPPER_LIMIT
}

const fn get_band_factor(base_frequency: u32) -> u32 {
    if is_frequency_band_high(base_frequency) {
        HIGH_BAND_FACTOR
    } else {
        MIDDLE_BAND_FACTOR
    }
}

const fn is_datarate(datarate: u32, xtal_freq: u32) -> bool {
    datarate >= MINIMUM_DATARATE
        && datarate <= (MAXIMUM_DATARATE * xtal_freq as u64 / 1000000 / 26) as u32
}

const fn is_f_dev(fdev: u32, xtal_freq: u32) -> bool {
    fdev >= (xtal_freq >> 22) && fdev <= ((787109u64 * xtal_freq as u64 / 1000000) / 26) as u32
}

const fn is_ch_bw(bandwidth: u32, dig_freq: u32) -> bool {
    bandwidth >= ((1100u64 * dig_freq as u64 / 1000000) / 26) as u32
        && bandwidth <= ((800100u64 * dig_freq as u64 / 1000000) / 26) as u32
}

/// VCO center frequency in Hz
const VCO_CENTER_FREQ: u64 = 3600000000;

/// Band select factor for high band. Factor B in the equation 2
const HIGH_BAND_FACTOR: u32 = 4;
/// Band select factor for middle band. Factor B in the equation 2
const MIDDLE_BAND_FACTOR: u32 = 8;

/// Lower limit of the high band: 860 MHz (S2-LPQTR)
const HIGH_BAND_LOWER_LIMIT: u32 = 825900000;
/// Upper limit of the high band: 940 MHz (S2-LPCBQTR)
const HIGH_BAND_UPPER_LIMIT: u32 = 1056000000;
/// Lower limit of the middle band: 430 MHz (S2-LPQTR)
const MIDDLE_BAND_LOWER_LIMIT: u32 = 412900000;
/// Upper limit of the middle band: 470 MHz (S2-LPCBQTR)
const MIDDLE_BAND_UPPER_LIMIT: u32 = 527100000;

/// Minimum datarate supported by S2LP 100 bps
const MINIMUM_DATARATE: u32 = 100;
/// Maximum datarate supported by S2LP 250 ksps
const MAXIMUM_DATARATE: u64 = 250000;

/// Digital domain logic threshold for XTAL in MHz
const DIG_DOMAIN_XTAL_THRESH: u32 = 30000000;

fn compute_datarate(digital_frequency: u32, mantissa: u16, exponent: u8) -> u32 {
    match exponent {
        0 => ((digital_frequency as u64 * mantissa as u64) >> 32) as u32,
        e @ 1..15 => {
            ((digital_frequency as u64 * (65536 + mantissa as u64)) >> (33 - e) as u64) as u32
        }
        15 => digital_frequency / (8 * mantissa as u32),
        #[cfg(feature = "defmt-03")]
        _ => defmt::panic!("Illegal exponent value"),
        #[cfg(not(feature = "defmt-03"))]
        _ => panic!("Illegal exponent value"),
    }
}

fn compute_fdev(
    xtal_freq: u32,   // fXO
    mantissa: u8,     // FDEV_M
    exponent: u8,     // FDEV_E
    band_factor: u32, // B
    refdiv: u32,      // D
) -> u32 {
    // (B/8)^-1
    let band_factor_div = if band_factor == HIGH_BAND_FACTOR {
        1
    } else {
        2
    };

    match exponent {
        0 => {
            let nom = xtal_freq as u64 * refdiv as u64 * mantissa as u64;
            let denom = (1 << 19) * refdiv as u64 * band_factor as u64 * band_factor_div;
            (nom / denom) as _
        }
        e @ 1..16 => {
            let nom =
                xtal_freq as u64 * refdiv as u64 * (256 + mantissa as u64) * (1 << (e as u64 - 1));
            let denom = (1 << 19) * refdiv as u64 * band_factor as u64 * band_factor_div;
            (nom / denom) as _
        }
        #[cfg(feature = "defmt-03")]
        _ => defmt::panic!("Illegal exponent value"),
        #[cfg(not(feature = "defmt-03"))]
        _ => panic!("Illegal exponent value"),
    }
}

fn search_channel_filter_bandwidth(target_bw: u32, dig_freq: u32) -> crate::ll::field_sets::ChFlt {
    // Datasheet Table 44
    // Every unit is 100hz
    const CHANNEL_FILTER_WORDS: [u16; 90] = [
        8001, 7951, 7684, 7368, 7051, 6709, 6423, 5867, 5414, 4509, 4259, 4032, 3808, 3621, 3417,
        3254, 2945, 2703, 2247, 2124, 2015, 1900, 1807, 1706, 1624, 1471, 1350, 1123, 1062, 1005,
        950, 903, 853, 812, 735, 675, 561, 530, 502, 474, 451, 426, 406, 367, 337, 280, 265, 251,
        237, 226, 213, 203, 184, 169, 140, 133, 126, 119, 113, 106, 101, 92, 84, 70, 66, 63, 59,
        56, 53, 51, 46, 42, 35, 33, 31, 30, 28, 27, 25, 23, 21, 18, 17, 16, 15, 14, 13, 13, 12, 11,
    ];

    let word_to_bandwidth = |word: u16| (word as u64 * 100 * dig_freq as u64 / 26_000_000) as u32;

    let (best_index, _) = CHANNEL_FILTER_WORDS
        .into_iter()
        // Calculate the bandwidth we get from the table
        .map(word_to_bandwidth)
        // Calculate the difference to the target bw
        .map(|possible_bw| possible_bw.abs_diff(target_bw))
        // Run over it with the index
        .enumerate()
        .min_by_key(|(_, diff)| *diff)
        .unwrap_or_default();

    #[cfg(feature = "defmt-03")]
    defmt::trace!(
        "Selected channel bandwidth. Target: {}, found: {}",
        target_bw,
        word_to_bandwidth(CHANNEL_FILTER_WORDS[best_index])
    );

    let mut w = crate::ll::field_sets::ChFlt::new_zero();

    w.set_ch_flt_e(best_index as u8 / 9);
    w.set_ch_flt_m(best_index as u8 % 9);

    w
}
