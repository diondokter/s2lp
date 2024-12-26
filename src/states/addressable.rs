use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal_async::{delay::DelayNs, digital::Wait, spi::SpiDevice};

use crate::{
    ll::{Device, DeviceInterface, GpioMode, GpioSelectInput, GpioSelectOutput},
    ErrorOf, GpioNumber, S2lp,
};

use super::Addressable;

#[allow(private_bounds)]
impl<State, Spi, Sdn, Gpio, Delay> S2lp<State, Spi, Sdn, Gpio, Delay>
where
    State: Addressable,
    Spi: SpiDevice,
    Sdn: OutputPin,
    Gpio: InputPin + Wait,
    Delay: DelayNs,
{
    /// Access the registers directly.
    ///
    /// Warning: The driver makes assumptions about the state of the device.
    /// Changing registers directly may break the driver. So be careful.
    pub fn ll(&mut self) -> &mut Device<DeviceInterface<Spi>> {
        &mut self.device
    }

    /// Set the function of a gpio pin.
    ///
    /// User care should be taken because making changes here can break the driver.
    ///
    /// - The gpio pin used by the driver should not be changed as the driver assumes it never gets changed by the user.
    /// - Some input options can change the chip state. The driver assumes only it will cause state changes.
    ///
    /// Generally you're fine if:
    /// - You don't use the gpio pin the driver already uses
    /// - You only use output functionality
    ///
    /// The output can also be used as a gpio extender with the VDD and GND states.
    pub async fn set_gpio_function(
        &mut self,
        number: GpioNumber,
        function: GpioFunction,
    ) -> Result<(), ErrorOf<Self>> {
        self.ll()
            .gpio_conf(number as usize)
            .write_async(|reg| match function {
                GpioFunction::HiZ => {
                    reg.set_gpio_mode(GpioMode::HiZ);
                }
                GpioFunction::Input { select } => {
                    reg.set_gpio_mode(GpioMode::Input);
                    reg.set_gpio_select_input(select);
                }
                GpioFunction::Output { high_power, select } => {
                    reg.set_gpio_mode(if high_power {
                        GpioMode::OutputHighPower
                    } else {
                        GpioMode::OutputLowPower
                    });
                    reg.set_gpio_select_output(select);
                }
            })
            .await?;

        Ok(())
    }
}

pub enum GpioFunction {
    HiZ,
    Input {
        select: GpioSelectInput,
    },
    Output {
        high_power: bool,
        select: GpioSelectOutput,
    },
}
