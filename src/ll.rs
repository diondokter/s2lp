use device_driver::AsyncRegisterInterface;
use embedded_hal::spi::Operation;
use embedded_hal_async::spi::SpiDevice;

#[derive(Debug)]
pub struct DeviceInterface<Spi: SpiDevice> {
    spi: Spi,
}

device_driver::create_device!(
    device_name: Device,
    manifest: "device.yaml"
);

impl<Spi: SpiDevice> DeviceInterface<Spi> {
    /// Construct a new instance of the device.
    ///
    /// Spi mode 0, max 8 MHz
    pub(crate) const fn new(spi: Spi) -> Self {
        Self { spi }
    }
}

impl<Spi: SpiDevice> device_driver::AsyncRegisterInterface for DeviceInterface<Spi> {
    type Error = DeviceError<Spi::Error>;

    type AddressType = u8;

    async fn write_register(
        &mut self,
        address: Self::AddressType,
        _size_bits: u32,
        data: &[u8],
    ) -> Result<(), Self::Error> {
        // #[cfg(feature = "defmt-03")]
        // defmt::trace!(
        //     "Writing to register {:X} with value {:X}",
        //     R::ADDRESS,
        //     data.as_raw_slice()
        // );

        Ok(self
            .spi
            .transaction(&mut [
                Operation::Write(&[0b0000_0000, address]),
                Operation::Write(data),
            ])
            .await?)
    }

    async fn read_register(
        &mut self,
        address: Self::AddressType,
        _size_bits: u32,
        data: &mut [u8],
    ) -> Result<(), Self::Error> {
        let result = self
            .spi
            .transaction(&mut [
                Operation::Write(&[0b0000_0001, address]),
                Operation::Read(data),
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

impl<Spi: SpiDevice> device_driver::AsyncCommandInterface for DeviceInterface<Spi> {
    type Error = DeviceError<Spi::Error>;
    type AddressType = u8;

    async fn dispatch_command(
        &mut self,
        address: Self::AddressType,
        _size_bits_in: u32,
        _input: &[u8],
        _size_bits_out: u32,
        _output: &mut [u8],
    ) -> Result<(), Self::Error> {
        // #[cfg(feature = "defmt-03")]
        // defmt::trace!("Dispatching command: {:X}", id as u8);

        Ok(self
            .spi
            .transaction(&mut [Operation::Write(&[0b1000_0000, address])])
            .await?)
    }
}

impl<Spi: SpiDevice> device_driver::BufferInterfaceError for DeviceInterface<Spi> {
    type Error = DeviceError<Spi::Error>;
}

impl<Spi: SpiDevice> device_driver::AsyncBufferInterface for DeviceInterface<Spi> {
    type AddressType = u8;

    async fn write(
        &mut self,
        address: Self::AddressType,
        buf: &[u8],
    ) -> Result<usize, DeviceError<Spi::Error>> {
        let tx_free_space = loop {
            let mut tx_fifo_status = [0];
            self.read_register(0x8F, 8, &mut tx_fifo_status).await?;
            let tx_fifo_status: TxFifoStatus = tx_fifo_status.into();

            let space = 128 - tx_fifo_status.n_elem_txfifo();

            if space > 0 {
                break space;
            }
        };

        let write_len = buf.len().min(tx_free_space as usize);

        self.spi
            .transaction(&mut [
                Operation::Write(&[0b0000_0000, address]),
                Operation::Write(&buf[..write_len]),
            ])
            .await?;

        Ok(write_len)
    }

    async fn read(
        &mut self,
        address: Self::AddressType,
        buf: &mut [u8],
    ) -> Result<usize, DeviceError<Spi::Error>> {
        let rx_available_space = loop {
            let mut rx_fifo_status = [0];
            self.read_register(0x90, 8, &mut rx_fifo_status).await?;
            let rx_fifo_status: RxFifoStatus = rx_fifo_status.into();

            if rx_fifo_status.n_elem_rxfifo() > 0 {
                break rx_fifo_status.n_elem_rxfifo();
            }
        };

        let read_len = buf.len().min(rx_available_space as usize);

        self.spi
            .transaction(&mut [
                Operation::Write(&[0b0000_0001, address]),
                Operation::Read(&mut buf[..read_len]),
            ])
            .await?;

        Ok(read_len)
    }

    async fn flush(&mut self, _address: Self::AddressType) -> Result<(), Self::Error> {
        Ok(())
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
        let mut s2 = Device::new(DeviceInterface::new(&mut spi_device));

        let version = s2.device_info_0().read_async().await.unwrap().version();
        let partnum = s2.device_info_1().read_async().await.unwrap().partnum();

        println!("Version: {:X}, partnum: {:X}", version, partnum);
        assert_eq!(version, 0xC1);
        assert_eq!(partnum, 0x03);

        spi_device.done();
    }
}
