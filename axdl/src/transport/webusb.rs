use std::time::Duration;

use webusb_web;

use crate::AxdlError;

use super::{Device, Transport};

pub const VENDOR_ID: u16 = 0x32c9;
pub const PRODUCT_ID: u16 = 0x1000;
pub const ENDPOINT_OUT: u8 = 0x01;
pub const ENDPOINT_IN: u8 = 0x81;

static USB: std::sync::OnceLock<webusb_web::Usb> = std::sync::OnceLock::new();

/// Transport implementation to use the USB device directly via WebUSB.
pub struct WebUsbTransport;

/// Device path for USB devices.
#[derive(Debug, Clone, PartialEq)]
pub struct WebUsbDevicePath {
    manufacturer_name: Option<String>,
    product_name: Option<String>,
    serial_number: Option<String>,
}

impl std::fmt::Display for UsbDevicePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(manufacturer_name) = &self.manufacturer_name {
            write!(f, "{} ", manufacturer_name)?;
        }
        if let Some(product_name) = &self.product_name {
            write!(f, "{} ", product_name)?;
        }
        if let Some(serial_number) = &self.serial_number {
            write!(f, "{}", serial_number)?;
        }
        Ok(())
    }
}

impl Transport for WebUsbTransport {
    type DevicePath = WebUsbDevicePath;
    type DeviceType = WebUsbDevice;

    fn list_devices() -> Result<Vec<Self::DevicePath>, AxdlError> {
        let usb = USB.get_or_init(|| webusb_web::Usb::new());
        usb.devices()
        Ok(list)
    }
    fn open_device(path: &Self::DevicePath) -> Result<Self::DeviceType, AxdlError> {
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
