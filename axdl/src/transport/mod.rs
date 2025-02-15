use std::time::Duration;

use crate::AxdlError;

#[cfg(feature = "serial")]
pub mod serial;
#[cfg(feature = "usb")]
pub mod usb;
#[cfg(feature = "webserial")]
pub mod webserial;
#[cfg(feature = "webusb")]
pub mod webusb;

/// Device trait for reading and writing data.
pub trait Device {
    fn read_timeout(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize, AxdlError>;
    fn write_timeout(&mut self, buf: &[u8], timeout: Duration) -> Result<usize, AxdlError>;
}

/// Transport trait for listing devices and opening devices.
pub trait Transport {
    type DeviceId;
    type DeviceType: Device;
    fn list_devices() -> Result<Vec<Self::DeviceId>, AxdlError>;
    fn open_device(path: &Self::DeviceId) -> Result<Self::DeviceType, AxdlError>;
}

pub type DynDevice = Box<dyn Device>;

#[cfg(feature = "webusb")]
mod async_transport {
    use crate::AxdlError;

    pub trait AsyncDevice {
        fn read(
            &mut self,
            buf: &mut [u8],
        ) -> impl std::future::Future<Output = Result<usize, AxdlError>>;
        fn write(
            &mut self,
            buf: &[u8],
        ) -> impl std::future::Future<Output = Result<usize, AxdlError>>;
    }

    pub trait AsyncTransport {
        type DeviceId;
        type DeviceType: AsyncDevice;
        fn list_devices(
        ) -> impl std::future::Future<Output = Result<Vec<Self::DeviceId>, AxdlError>>;
        fn open_device(
            path: &Self::DeviceId,
        ) -> impl std::future::Future<Output = Result<Self::DeviceType, AxdlError>>;
    }
}

#[cfg(feature = "webusb")]
pub use async_transport::*;
