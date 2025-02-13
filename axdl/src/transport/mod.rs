use std::time::Duration;

use crate::AxdlError;

#[cfg(feature = "usb")]
pub mod usb;
#[cfg(feature = "serial")]
pub mod serial;
//#[cfg(feature = "webusb")]
//pub mod webusb;

/// Device trait for reading and writing data.
pub trait Device {
    fn read_timeout(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize, AxdlError>;
    fn write_timeout(&mut self, buf: &[u8], timeout: Duration) -> Result<usize, AxdlError>;
}

/// Transport trait for listing devices and opening devices.
pub trait Transport {
    type DevicePath;
    type DeviceType: Device;
    fn list_devices() -> Result<Vec<Self::DevicePath>, AxdlError>;
    fn open_device(path: &Self::DevicePath) -> Result<Self::DeviceType, AxdlError>;
}

pub type DynDevice = Box<dyn Device>;
