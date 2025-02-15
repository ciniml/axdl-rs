use std::time::Duration;

use wasm_streams::{ReadableStream, WritableStream};
use webusb_web;

use crate::AxdlError;

use super::AsyncDevice;

pub const VENDOR_ID: u16 = 0x32c9;
pub const PRODUCT_ID: u16 = 0x1000;
pub const ENDPOINT_OUT: u8 = 0x01;
pub const ENDPOINT_IN: u8 = 0x81;

pub fn new_serial() -> Result<web_sys::Serial, AxdlError> {
    web_sys::window()
        .map(|window| window.navigator().serial())
        .ok_or(AxdlError::Unsupported("WebSerial".to_string()))
}

/// Returns a device filter for Axera devices.
pub fn axdl_device_filter() -> web_sys::SerialPortFilter {
    let mut filter = web_sys::SerialPortFilter::new();
    filter.set_usb_vendor_id(VENDOR_ID);
    filter.set_usb_product_id(PRODUCT_ID);
    filter
}

pub struct WebSerialDevice {
    port: web_sys::SerialPort,
    read_buffer: Vec<u8>,
    read_position: usize,
}

impl WebSerialDevice {
    pub fn new(port: web_sys::SerialPort) -> Self {
        let read_buffer = Vec::new();
        let read_position = 0;
        Self {
            port,
            read_buffer,
            read_position,
        }
    }
}

impl AsyncDevice for WebSerialDevice {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, AxdlError> {
        if buf.len() == 0 {
            return Ok(0);
        }
        let bytes_remaining = self.read_buffer.len() - self.read_position;
        if bytes_remaining < buf.len() {
            let mut stream = ReadableStream::from_raw(self.port.readable());
            let mut reader = stream.get_reader();
            pin_utils::pin_mut!(reader);
            let result = reader.read().await;
            if let Ok(Some(chunk)) = result {
                if let Ok(buffer) = js_sys::Uint8Array::try_from(chunk) {
                    let length = buffer.length() as usize;
                    let prev_len = self.read_buffer.len();
                    self.read_buffer.resize(prev_len + length, 0);
                    buffer.copy_to(&mut self.read_buffer[prev_len..]);
                }
            }
        }

        let bytes_remaining = self.read_buffer.len() - self.read_position;
        if bytes_remaining == 0 {
            return Ok(0);
        } else if bytes_remaining < buf.len() {
            buf[..bytes_remaining].copy_from_slice(&self.read_buffer[self.read_position..]);
            self.read_position = 0;
            self.read_buffer.clear();
            Ok(bytes_remaining)
        } else {
            buf.copy_from_slice(
                &self.read_buffer[self.read_position..self.read_position + buf.len()],
            );
            self.read_position += buf.len();
            Ok(buf.len())
        }
    }

    async fn write(&mut self, buf: &[u8]) -> Result<usize, AxdlError> {
        let buffer = js_sys::Uint8Array::from(buf);
        let mut stream = WritableStream::from_raw(self.port.writable());
        let writer = stream.get_writer();
        pin_utils::pin_mut!(writer);
        writer
            .write(buffer.into())
            .await
            .map_err(AxdlError::WebSerialError)?;
        Ok(buf.len())
    }
}
