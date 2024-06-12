pub mod shutdown;
pub mod ready;
pub mod addressable;

pub struct Shutdown;
pub struct Standby;
pub struct Ready;
pub struct Tx;
pub struct Rx;

/// Implemented if the state allows for spi communication
pub(crate) trait Addressable {

}

impl Addressable for Standby {}
impl Addressable for Ready {}
impl Addressable for Tx {}
impl Addressable for Rx {}
