use std::time::Duration;

use rusb::DeviceHandle;

use crate::AxdlError;

use super::{Device, Transport};

pub const VENDOR_ID: u16 = 0x32c9;
pub const PRODUCT_ID: u16 = 0x1000;
pub const ENDPOINT_OUT: u8 = 0x01;
pub const ENDPOINT_IN: u8 = 0x81;

/// Transport implementation to use the USB device directly via libusb.
pub struct UsbTransport;

/// Device path for USB devices.
#[derive(Debug, Clone, PartialEq)]
pub struct UsbDevicePath {
    port_numbers: Vec<u8>,
}

impl std::fmt::Display for UsbDevicePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Concat port number with dot.
        for (i, port_number) in self.port_numbers.iter().enumerate() {
            if i > 0 {
                write!(f, ".")?;
            }
            write!(f, "{}", port_number)?;
        }
        Ok(())
    }
}

impl Transport for UsbTransport {
    type DeviceId = UsbDevicePath;
    type DeviceType = UsbDevice;

    fn list_devices() -> Result<Vec<Self::DeviceId>, AxdlError> {
        let list = rusb::devices()
            .map_err(AxdlError::UsbError)?
            .iter()
            .filter_map(|device| {
                if let Ok(device_desc) = device.device_descriptor() {
                    if device_desc.vendor_id() == VENDOR_ID
                        && device_desc.product_id() == PRODUCT_ID
                    {
                        device
                            .port_numbers()
                            .ok()
                            .map(|port_numbers| UsbDevicePath { port_numbers })
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();
        Ok(list)
    }
    fn open_device(path: &Self::DeviceId) -> Result<Self::DeviceType, AxdlError> {
        let device = rusb::devices()
            .map_err(AxdlError::UsbError)?
            .iter()
            .find(|device| {
                if let Ok(device_desc) = device.device_descriptor() {
                    if device_desc.vendor_id() == VENDOR_ID
                        && device_desc.product_id() == PRODUCT_ID
                    {
                        if let Ok(port_numbers) = device.port_numbers() {
                            return port_numbers == path.port_numbers;
                        }
                    }
                }
                false
            })
            .ok_or(AxdlError::DeviceNotFound)?;

        let handle = device.open().map_err(AxdlError::UsbError)?;
        handle.claim_interface(0).map_err(AxdlError::UsbError)?;
        Ok(UsbDevice { handle })
    }
}

#[derive(Debug)]
pub struct UsbDevice {
    handle: DeviceHandle<rusb::GlobalContext>,
}

impl Device for UsbDevice {
    fn read_timeout(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize, AxdlError> {
        self.handle
            .read_bulk(ENDPOINT_IN, buf, timeout)
            .map_err(AxdlError::UsbError)
    }
    fn write_timeout(&mut self, buf: &[u8], timeout: Duration) -> Result<usize, AxdlError> {
        self.handle
            .write_bulk(ENDPOINT_OUT, buf, timeout)
            .map_err(AxdlError::UsbError)
    }
}
