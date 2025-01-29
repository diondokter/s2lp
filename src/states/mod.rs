//! Definition of the various type states

use core::marker::PhantomData;

pub mod addressable;
pub mod ready;
pub mod rx;
pub mod shutdown;
pub mod standby;
pub mod tx;

/// The radio is in shutdown mode. This is the lowest power state and the radio is effectively turned off.
pub struct Shutdown;
/// The radio is in standby mode. This is the lowest power state where the radio is still active.
pub struct Standby<PF: ?Sized> {
    /// The internal `fdig` of the radio
    digital_frequency: u32,
    _p: PhantomData<PF>,
}
/// The radio is in ready mode. From here the radio can start sending and receiving packets.
pub struct Ready<PF: ?Sized> {
    /// The internal `fdig` of the radio
    digital_frequency: u32,
    _p: PhantomData<PF>,
}

impl<PF> Ready<PF> {
    pub(crate) fn new(digital_frequency: u32) -> Self {
        Self {
            digital_frequency,
            _p: PhantomData,
        }
    }
}

/// The radio is in send mode. A packet is being sent or has just been sent
pub struct Tx<'buffer, PF> {
    /// The internal `fdig` of the radio
    digital_frequency: u32,
    tx_buffer: &'buffer [u8],
    tx_done: bool,
    _p: PhantomData<PF>,
}

impl<'buffer, PF> Tx<'buffer, PF> {
    fn new(digital_frequency: u32, tx_buffer: &'buffer [u8]) -> Self {
        Self {
            digital_frequency,
            tx_buffer,
            tx_done: false,
            _p: PhantomData,
        }
    }
}

/// The radio is in receive mode. The receiver is currently on, or a packet is has been received and is ready to be read out
pub struct Rx<'buffer, PF> {
    /// The internal `fdig` of the radio
    digital_frequency: u32,
    rx_buffer: &'buffer mut [u8],
    written: usize,
    rx_done: bool,
    _p: PhantomData<PF>,
}

impl<'buffer, PF> Rx<'buffer, PF> {
    fn new(digital_frequency: u32, rx_buffer: &'buffer mut [u8]) -> Self {
        Self {
            digital_frequency,
            rx_buffer,
            written: 0,
            rx_done: false,
            _p: PhantomData,
        }
    }
}

/// Implemented if the state allows for spi communication
pub(crate) trait Addressable {}

impl<PF> Addressable for Standby<PF> {}
impl<PF> Addressable for Ready<PF> {}
impl<PF> Addressable for Tx<'_, PF> {}
impl<PF> Addressable for Rx<'_, PF> {}
