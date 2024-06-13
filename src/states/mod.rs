use core::marker::PhantomData;

pub mod addressable;
pub mod ready;
pub mod shutdown;

pub struct Shutdown;
pub struct Standby;
pub struct Ready<PF> {
    _p: PhantomData<PF>,
}
pub struct Tx<PF> {
    _p: PhantomData<PF>,
}
pub struct Rx<PF> {
    _p: PhantomData<PF>,
}

/// Implemented if the state allows for spi communication
pub(crate) trait Addressable {}

impl Addressable for Standby {}
impl<PF> Addressable for Ready<PF> {}
impl<PF> Addressable for Tx<PF> {}
impl<PF> Addressable for Rx<PF> {}
