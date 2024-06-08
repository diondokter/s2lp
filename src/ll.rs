use device_driver::embedded_io::ErrorKind;
use embedded_hal::spi::Operation;
use embedded_hal_async::spi::SpiDevice;

pub struct Device<Spi: SpiDevice> {
    spi: Spi,
}

#[device_driver::implement_device_from_file(yaml = "device.yaml")]
impl<Spi: SpiDevice> Device<Spi> {}

impl<Spi: SpiDevice> Device<Spi> {
    /// Construct a new instance of the device.
    ///
    /// Spi mode 0, max 8 MHz
    pub(crate) const fn new(spi: Spi) -> Self {
        Self { spi }
    }
}

impl<Spi: SpiDevice> device_driver::AsyncRegisterDevice for Device<Spi> {
    type Error = Spi::Error;

    type AddressType = u8;

    async fn write_register<R, const SIZE_BYTES: usize>(
        &mut self,
        data: &device_driver::bitvec::prelude::BitArray<[u8; SIZE_BYTES]>,
    ) -> Result<(), Self::Error>
    where
        R: device_driver::Register<SIZE_BYTES, AddressType = Self::AddressType>,
    {
        self.spi
            .transaction(&mut [
                Operation::Write(&[0b0000_0000, R::ADDRESS]),
                Operation::Write(data.as_raw_slice()),
            ])
            .await
    }

    async fn read_register<R, const SIZE_BYTES: usize>(
        &mut self,
        data: &mut device_driver::bitvec::prelude::BitArray<[u8; SIZE_BYTES]>,
    ) -> Result<(), Self::Error>
    where
        R: device_driver::Register<SIZE_BYTES, AddressType = Self::AddressType>,
    {
        self.spi
            .transaction(&mut [
                Operation::Write(&[0b0000_0001, R::ADDRESS]),
                Operation::Read(data.as_raw_mut_slice()),
            ])
            .await
    }
}

impl<Spi: SpiDevice> device_driver::AsyncCommandDevice for Device<Spi> {
    type Error = Spi::Error;

    async fn dispatch_command(&mut self, id: u32) -> Result<(), Self::Error> {
        self.spi
            .transaction(&mut [Operation::Write(&[0b1000_0000, id as u8])])
            .await
    }
}

impl<Spi: SpiDevice> device_driver::AsyncBufferDevice for Device<Spi> {
    async fn write(&mut self, id: u32, buf: &[u8]) -> Result<usize, ErrorKind> {
        let tx_free_space = loop {
            let space = 128
                - self
                    .tx_fifo_status()
                    .read_async()
                    .await
                    .map_err(|_| ErrorKind::Other)?
                    .n_elem_txfifo();

            if space > 0 {
                break space;
            }
        };

        let write_len = buf.len().min(tx_free_space as usize);

        self.spi
            .transaction(&mut [
                Operation::Write(&[0b0000_0000, id as u8]),
                Operation::Write(&buf[..write_len]),
            ])
            .await
            .map_err(|_| ErrorKind::Other)?;

        Ok(write_len)
    }

    async fn read(&mut self, id: u32, buf: &mut [u8]) -> Result<usize, ErrorKind> {
        let rx_available_space = loop {
            let space = self
                .rx_fifo_status()
                .read_async()
                .await
                .map_err(|_| ErrorKind::Other)?
                .n_elem_rxfifo();

            if space > 0 {
                break space;
            }
        };

        let read_len = buf.len().min(rx_available_space as usize);

        self.spi
            .transaction(&mut [
                Operation::Write(&[0b0000_0001, id as u8]),
                Operation::Read(&mut buf[..read_len]),
            ])
            .await
            .map_err(|_| ErrorKind::Other)?;

        Ok(read_len)
    }
}
