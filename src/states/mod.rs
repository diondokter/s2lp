use core::marker::PhantomData;

pub mod addressable;
pub mod ready;
pub mod shutdown;

pub struct Shutdown;
pub struct Standby;
pub struct Ready<PF> {
    _p: PhantomData<PF>,
}

impl<PF> Ready<PF> {
    pub fn new() -> Self {
        Self { _p: PhantomData }
    }
}

pub struct Tx<'buffer, PF> {
    tx_buffer: &'buffer [u8],
    _p: PhantomData<PF>,
}

impl<'buffer, PF> Tx<'buffer, PF> {
    pub fn new(tx_buffer: &'buffer [u8]) -> Self {
        Self { tx_buffer, _p: PhantomData }
    }
}

pub struct Rx<PF> {
    _p: PhantomData<PF>,
}

impl<PF> Rx<PF> {
    pub fn new() -> Self {
        Self { _p: PhantomData }
    }
}

/// Implemented if the state allows for spi communication
pub(crate) trait Addressable {}

impl Addressable for Standby {}
impl<PF> Addressable for Ready<PF> {}
impl<'buffer, PF> Addressable for Tx<'buffer, PF> {}
impl<PF> Addressable for Rx<PF> {}
