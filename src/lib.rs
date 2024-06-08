pub mod ll;

pub enum Error<S> {
    Spi(S),
}

pub enum GpioNumber {
    Gpio0,
    Gpio1,
    Gpio2,
    Gpio3,
}
