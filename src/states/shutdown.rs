use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal_async::{delay::DelayNs, digital::Wait, spi::SpiDevice};

use crate::{
    ll::{Device, DeviceInterface, GpioMode, GpioSelectOutput, State},
    packet_format::Uninitialized,
    Error, ErrorOf, S2lp,
};

use super::{Ready, Shutdown};

impl<Spi, Sdn, Gpio, Delay> S2lp<Shutdown, Spi, Sdn, Gpio, Delay>
where
    Spi: SpiDevice,
    Sdn: OutputPin,
    Gpio: InputPin + Wait,
    Delay: DelayNs,
{
    pub const fn new(spi: Spi, shutdown_pin: Sdn, gpio0: Gpio, delay: Delay) -> Self {
        Self {
            device: Device::new(DeviceInterface::new(spi)),
            shutdown_pin,
            gpio0,
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

        #[cfg(feature = "defmt-03")]
        defmt::trace!("Waiting for POR");

        self.gpio0.wait_for_high().await.map_err(Error::Gpio)?;

        #[cfg(feature = "defmt-03")]
        defmt::trace!("Checking interface works");
        let version = self.device.device_info_0().read_async().await?.version();
        if version != 0xC1 {
            return Err(Error::Init);
        }

        let mut this = self.cast_state(Ready::new());

        #[cfg(feature = "defmt-03")]
        defmt::trace!("Setting correct radio config");
        // Set the gpio pin to irq mode since we use IRQs in the driver
        this.ll()
            .gpio_conf(0)
            .write_async(|reg| {
                reg.set_gpio_mode(GpioMode::OutputLowPower);
                reg.set_gpio_select_output(GpioSelectOutput::Irq);
            })
            .await?;

        // Datasheet 4.7 - Setting up the crystal oscillator
        // If the xtal_frequency is slow, then we can drive the chip from it directly.
        // If it is fast, we need to enable the clock divider.
        let digital_frequency = {
            let mut pd_clkdiv = this.ll().xo_rco_conf_1().read_async().await?.pd_clkdiv();

            if (config.xtal_frequency < DIG_DOMAIN_XTAL_THRESH && pd_clkdiv)
                || (config.xtal_frequency > DIG_DOMAIN_XTAL_THRESH && !pd_clkdiv)
            {
                // Go to standby
                this.ll().standby().dispatch_async().await?;
                while this.ll().mc_state_0().read_async().await?.state()? != State::Standby {}

                // Invert the pd_clkdiv
                pd_clkdiv = !pd_clkdiv;
                this.ll()
                    .xo_rco_conf_1()
                    .modify_async(|reg| reg.set_pd_clkdiv(pd_clkdiv))
                    .await?;

                // Go to ready
                this.ll().ready().dispatch_async().await?;
                while this.ll().mc_state_0().read_async().await?.state()? != State::Ready {}
            }

            config.xtal_frequency / pd_clkdiv.then_some(2).unwrap_or(1)
        };

        if !is_ch_bw(config.bandwidth, digital_frequency) {
            return Err(Error::BadConfig {
                reason: "Bandwidth out of range",
            });
        }

        // Datasheet 5.5.5 - Set the Intermediate Frequency (IF) to the recommended value
        {
            const IF: u64 = 300_000;
            this.ll()
                .if_offset_ana()
                .write_async(|reg| {
                    reg.set_value(((IF << 13) * 3 / config.xtal_frequency as u64 - 100) as u8)
                })
                .await?;
            this.ll()
                .if_offset_dig()
                .write_async(|reg| {
                    reg.set_value(((IF << 13) * 3 / digital_frequency as u64 - 100) as u8)
                })
                .await?;
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
                "Selected datarate: {}",
                compute_datarate(digital_frequency, used_mantissa, used_exponent)
            );

            this.ll()
                .mod_4()
                .write_async(|reg| reg.set_value(used_mantissa))
                .await?;
            this.ll()
                .mod_2()
                .write_async(|reg| {
                    reg.set_datarate_e(used_exponent);
                    reg.set_modulation_type(config.modulation);
                })
                .await?;
        }

        // Datasheet 5.4.1 - Configure the frequency modulation
        {
            let band_factor = if this.ll().synt().read_async().await?.bs() {
                HIGH_BAND_FACTOR
            } else {
                MIDDLE_BAND_FACTOR
            };

            let refdiv = if this.ll().xo_rco_conf_0().read_async().await?.refdiv() {
                2
            } else {
                1
            };

            // Search for the smallest exponent that our fdev fits in for the highest resolution
            let mut used_exponent = 0;
            for exponent in 0..16 {
                if compute_fdev(
                    config.xtal_frequency,
                    u8::MAX,
                    exponent,
                    band_factor,
                    refdiv,
                ) > config.frequency_deviation
                {
                    used_exponent = exponent;
                    break;
                }
            }

            let used_mantissa = if used_exponent == 0 {
                let target = (config.frequency_deviation as u64) << 22;
                (target + (config.frequency_deviation as u64 / 2))
                    / config.frequency_deviation as u64
            } else {
                let target = (config.frequency_deviation as u64) << (23 - used_exponent as u64);
                (target + (config.frequency_deviation as u64 / 2))
                    / config.frequency_deviation as u64
                    - 256
            } as u8;

            #[cfg(feature = "defmt-03")]
            defmt::trace!(
                "Selected frequency deviation: {}",
                compute_fdev(
                    config.xtal_frequency,
                    used_mantissa,
                    used_exponent,
                    band_factor,
                    refdiv,
                )
            );

            this.ll()
                .mod_1()
                .modify_async(|reg| reg.set_fdev_e(used_exponent))
                .await?;
            this.ll()
                .mod_0()
                .write_async(|reg| reg.set_fdev_m(used_mantissa))
                .await?;
        }

        // Set the bandwidth
        this.ll()
            .ch_flt()
            .write_async(|reg| {
                *reg = search_channel_filter_bandwidth(config.bandwidth, digital_frequency);
            })
            .await?;

        // Set the OOK smoothing
        let is_ook = matches!(config.modulation, ModulationType::AskOok);
        this.ll()
            .pa_power_0()
            .modify_async(|reg| reg.set_dig_smooth_en(is_ook))
            .await?;
        this.ll()
            .pa_config_1()
            .modify_async(|reg| reg.set_fir_en(is_ook))
            .await?;

        this.ll()
            .pa_config_0()
            .modify_async(|reg| {
                reg.set_pa_fc(match config.datarate {
                    ..16000 => crate::ll::PaFc::Khz12P5,
                    16000..32000 => crate::ll::PaFc::Khz25,
                    32000..62500 => crate::ll::PaFc::Khz50,
                    62500.. => crate::ll::PaFc::Khz100,
                })
            })
            .await?;

        // Enable AFC freeze on SYNC
        this.ll()
            .afc_2()
            .modify_async(|reg| reg.set_afc_freeze_on_sync(true))
            .await?;

        // Set the synt word (base frequency) and charge pump
        {
            let band_factor = if this.ll().synt().read_async().await?.bs() {
                HIGH_BAND_FACTOR
            } else {
                MIDDLE_BAND_FACTOR
            };

            let refdiv = if this.ll().xo_rco_conf_0().read_async().await?.refdiv() {
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
                .modify_async(|reg| reg.set_pll_pfd_split_en(pfd_split))
                .await?;
            this.ll()
                .synt()
                .modify_async(|reg| {
                    reg.set_synt(synt);
                    reg.set_pll_cp_isel(cp_isel)
                })
                .await?;
        }

        #[cfg(feature = "defmt-03")]
        defmt::debug!("Init done!");

        Ok(this)
    }
}

pub use crate::ll::ModulationType;

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

const fn is_frequency_band(freq: u32) -> bool {
    is_frequency_band_high(freq) || is_frequency_band_middle(freq)
}

const fn is_frequency_band_high(freq: u32) -> bool {
    freq >= HIGH_BAND_LOWER_LIMIT && freq <= HIGH_BAND_UPPER_LIMIT
}

const fn is_frequency_band_middle(freq: u32) -> bool {
    freq >= MIDDLE_BAND_LOWER_LIMIT && freq <= MIDDLE_BAND_UPPER_LIMIT
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

const fn compute_datarate(digital_frequency: u32, mantissa: u16, exponent: u8) -> u32 {
    match exponent {
        0 => ((digital_frequency as u64 * mantissa as u64) >> 32) as u32,
        e @ 1..15 => {
            ((digital_frequency as u64 * (65536 + mantissa as u64)) >> (33 - e) as u64) as u32
        }
        15 => digital_frequency / (8 * mantissa as u32),
        _ => panic!("Illegal exponent value"),
    }
}

const fn compute_fdev(
    xtal_freq: u32,
    mantissa: u8,
    exponent: u8,
    band_factor: u32,
    refdiv: u32,
) -> u32 {
    let band_factor_div = if band_factor == HIGH_BAND_FACTOR {
        1
    } else {
        2
    };

    match exponent {
        0 => {
            ((xtal_freq as u64 * refdiv as u64
                / band_factor_div
                / (refdiv as u64 * band_factor as u64))
                >> 19) as u32
        }
        e @ 1..16 => {
            (xtal_freq as u64 * refdiv as u64 * (256 + mantissa as u64)
                / band_factor_div
                / (refdiv as u64 * band_factor as u64)
                >> (20 - e)) as u32
        }
        _ => panic!("Illegal exponent value"),
    }
}

fn search_channel_filter_bandwidth(target_bw: u32, dig_freq: u32) -> crate::ll::ChFlt {
    // Datasheet Table 44
    const CHANNEL_FILTER_WORDS: [u16; 90] = [
        8001, 7951, 7684, 7368, 7051, 6709, 6423, 5867, 5414, 4509, 4259, 4032, 3808, 3621, 3417,
        3254, 2945, 2703, 2247, 2124, 2015, 1900, 1807, 1706, 1624, 1471, 1350, 1123, 1062, 1005,
        950, 903, 853, 812, 735, 675, 561, 530, 502, 474, 451, 426, 406, 367, 337, 280, 265, 251,
        237, 226, 213, 203, 184, 169, 140, 133, 126, 119, 113, 106, 101, 92, 84, 70, 66, 63, 59,
        56, 53, 51, 46, 42, 35, 33, 31, 30, 28, 27, 25, 23, 21, 18, 17, 16, 15, 14, 13, 13, 12, 11,
    ];

    let word_to_bandwidth = |word: u16| (word as u64 * dig_freq as u64 / (26_000_000 / 10)) as u32;

    let (best_index, _) = CHANNEL_FILTER_WORDS
        .into_iter()
        // Calculate the bandwidth we get from the table
        .map(word_to_bandwidth)
        // Calculate the difference to the target bw
        .map(|possible_bw| possible_bw.abs_diff(target_bw))
        // Run over it with the index
        .enumerate()
        .min_by_key(|(_, diff)| *diff)
        .unwrap();

    #[cfg(feature = "defmt-03")]
    defmt::debug!(
        "Searched channel bandwidth. Target: {}, found: {}",
        target_bw,
        word_to_bandwidth(CHANNEL_FILTER_WORDS[best_index])
    );

    let mut w = crate::ll::ChFlt::new_zero();

    w.set_ch_flt_e(best_index as u8 / 9);
    w.set_ch_flt_m(best_index as u8 % 9);

    w
}
