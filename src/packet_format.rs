use core::fmt::Debug;

use device_driver::AsyncRegisterInterface;

use crate::ll::Device;

pub struct Uninitialized;

pub trait PacketFormat {
    type RxMetaData: RxMetaData;
}

#[allow(async_fn_in_trait)]
pub trait RxMetaData: Debug + Clone {
    async fn read_from_device<I: AsyncRegisterInterface<AddressType = u8>>(
        device: &mut Device<I>,
    ) -> Result<Self, I::Error>
    where
        Self: Sized;
}

// Basic impl

pub struct Basic;

impl PacketFormat for Basic {
    type RxMetaData = BasicRxMetaData;
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt-03", derive(defmt::Format))]
pub struct BasicRxMetaData {
    /// The received packet destination address (if any)
    pub destination_address: Option<u8>,
}

impl RxMetaData for BasicRxMetaData {
    async fn read_from_device<I: AsyncRegisterInterface<AddressType = u8>>(
        device: &mut Device<I>,
    ) -> Result<Self, I::Error>
    where
        Self: Sized,
    {
        let destination_address = if device.pckt_ctrl_4().read_async().await?.address_len() {
            Some(device.rx_addre_field_0().read_async().await?.value())
        } else {
            None
        };

        Ok(Self {
            destination_address,
        })
    }
}
