use core::marker::PhantomData;

pub mod addressable;
pub mod ready;
pub mod shutdown;
pub mod tx;
pub mod rx;

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
    tx_done: bool,
    _p: PhantomData<PF>,
}

impl<'buffer, PF> Tx<'buffer, PF> {
    pub fn new(tx_buffer: &'buffer [u8]) -> Self {
        Self { tx_buffer, tx_done: false, _p: PhantomData }
    }
}

pub struct Rx<'buffer, PF> {
    rx_buffer: &'buffer mut [u8],
    written: usize,
    rx_done: bool,
    _p: PhantomData<PF>,
}

impl<'buffer, PF> Rx<'buffer, PF> {
    pub fn new(rx_buffer: &'buffer mut [u8]) -> Self {
        Self { rx_buffer, written: 0, rx_done: false, _p: PhantomData }
    }
}

/// Implemented if the state allows for spi communication
pub(crate) trait Addressable {}

impl Addressable for Standby {}
impl<PF> Addressable for Ready<PF> {}
impl<'buffer, PF> Addressable for Tx<'buffer, PF> {}
impl<'buffer, PF> Addressable for Rx<'buffer, PF> {}
