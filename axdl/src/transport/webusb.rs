use std::time::Duration;

use webusb_web;

use crate::AxdlError;

use super::AsyncDevice;

pub const VENDOR_ID: u16 = 0x32c9;
pub const PRODUCT_ID: u16 = 0x1000;
pub const ENDPOINT_OUT: u8 = 0x01;
pub const ENDPOINT_IN: u8 = 0x01;

/// Returns a device filter for Axera devices.
pub fn axdl_device_filter() -> webusb_web::UsbDeviceFilter {
    webusb_web::UsbDeviceFilter::new()
        .with_vendor_id(VENDOR_ID)
        .with_product_id(PRODUCT_ID)
}

impl AsyncDevice for webusb_web::OpenUsbDevice {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, AxdlError> {
        let result = self
            .transfer_in(ENDPOINT_IN, buf.len() as u32)
            .await
            .map_err(AxdlError::WebUsbError)?;
        let bytes_to_copy = result.len().min(buf.len());

        buf[..bytes_to_copy].copy_from_slice(&result[..bytes_to_copy]);
        Ok(bytes_to_copy)
    }

    async fn write(&mut self, buf: &[u8]) -> Result<usize, AxdlError> {
        let bytes_written = self
            .transfer_out(ENDPOINT_OUT, buf)
            .await
            .map_err(AxdlError::WebUsbError)?;
        Ok(bytes_written as usize)
    }
}
