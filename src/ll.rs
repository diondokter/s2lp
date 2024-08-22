use device_driver::embedded_io::ErrorKind;
use embedded_hal::spi::Operation;
use embedded_hal_async::spi::SpiDevice;

#[derive(Debug)]
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
    type Error = DeviceError<Spi::Error>;

    type AddressType = u8;

    async fn write_register<R, const SIZE_BYTES: usize>(
        &mut self,
        data: &device_driver::bitvec::prelude::BitArray<[u8; SIZE_BYTES]>,
    ) -> Result<(), Self::Error>
    where
        R: device_driver::Register<SIZE_BYTES, AddressType = Self::AddressType>,
    {
        // #[cfg(feature = "defmt-03")]
        // defmt::trace!(
        //     "Writing to register {:X} with value {:X}",
        //     R::ADDRESS,
        //     data.as_raw_slice()
        // );

        Ok(self
            .spi
            .transaction(&mut [
                Operation::Write(&[0b0000_0000, R::ADDRESS]),
                Operation::Write(data.as_raw_slice()),
            ])
            .await?)
    }

    async fn read_register<R, const SIZE_BYTES: usize>(
        &mut self,
        data: &mut device_driver::bitvec::prelude::BitArray<[u8; SIZE_BYTES]>,
    ) -> Result<(), Self::Error>
    where
        R: device_driver::Register<SIZE_BYTES, AddressType = Self::AddressType>,
    {
        let result = self
            .spi
            .transaction(&mut [
                Operation::Write(&[0b0000_0001, R::ADDRESS]),
                Operation::Read(data.as_raw_mut_slice()),
            ])
            .await?;

        // #[cfg(feature = "defmt-03")]
        // defmt::trace!(
        //     "Reading from register {:X}, value {:X}",
        //     R::ADDRESS,
        //     data.as_raw_slice()
        // );

        Ok(result)
    }
}

impl<Spi: SpiDevice> device_driver::AsyncCommandDevice for Device<Spi> {
    type Error = DeviceError<Spi::Error>;

    async fn dispatch_command(&mut self, id: u32) -> Result<(), Self::Error> {
        // #[cfg(feature = "defmt-03")]
        // defmt::trace!("Dispatching command: {:X}", id as u8);

        Ok(self
            .spi
            .transaction(&mut [Operation::Write(&[0b1000_0000, id as u8])])
            .await?)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "defmt-03", derive(defmt::Format))]
pub struct DeviceError<Spi>(pub Spi);

impl<Spi> From<Spi> for DeviceError<Spi> {
    fn from(value: Spi) -> Self {
        Self(value)
    }
}

impl<Spi> core::ops::Deref for DeviceError<Spi> {
    type Target = Spi;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<Spi> core::ops::DerefMut for DeviceError<Spi> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_hal_mock::eh1::spi;
    use futures_test::test;

    #[test]
    async fn read_chip_id() {
        let mut spi_device = spi::Mock::new(&[
            spi::Transaction::transaction_start(),
            spi::Transaction::write_vec(vec![0x01, 0xF1]),
            spi::Transaction::read(0xC1),
            spi::Transaction::transaction_end(),
            spi::Transaction::transaction_start(),
            spi::Transaction::write_vec(vec![0x01, 0xF0]),
            spi::Transaction::read(0x03),
            spi::Transaction::transaction_end(),
        ]);
        let mut s2 = Device::new(&mut spi_device);

        let version = s2.device_info_0().read_async().await.unwrap().version();
        let partnum = s2.device_info_1().read_async().await.unwrap().partnum();

        println!("Version: {:X}, partnum: {:X}", version, partnum);
        assert_eq!(version, 0xC1);
        assert_eq!(partnum, 0x03);

        spi_device.done();
    }
}
