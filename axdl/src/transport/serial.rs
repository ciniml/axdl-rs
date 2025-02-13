use std::time::Duration;
use crate::AxdlError;

use super::{Device, Transport};

pub const VENDOR_ID: u16 = 0x32c9;
pub const PRODUCT_ID: u16 = 0x1000;

/// Transport implementation for serial ports
pub struct SerialTransport;

/// Device path for serial ports.
#[derive(Debug, Clone, PartialEq)]
pub struct SerialDevicePath {
    port_name: String,
}

impl SerialDevicePath {
    pub fn is_match(&self, port_name: &str) -> bool {
        self.port_name == port_name
    }
}

impl std::fmt::Display for SerialDevicePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.port_name)
    }
}

impl Transport for SerialTransport {
    type DevicePath = SerialDevicePath;
    type DeviceType = SerialDevice;

    fn list_devices() -> Result<Vec<Self::DevicePath>, AxdlError> {
        let list = serialport::available_ports()
            .map_err(AxdlError::SerialError)?
            .iter()
            .filter_map(|port_info| {
                match &port_info.port_type {
                    serialport::SerialPortType::UsbPort(usb) => {
                        if usb.vid == VENDOR_ID && usb.pid == PRODUCT_ID {
                            Some(SerialDevicePath {
                                port_name: port_info.port_name.clone(),
                            })
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            })
            .collect();
        Ok(list)
    }
    fn open_device(path: &Self::DevicePath) -> Result<Self::DeviceType, AxdlError> {
        let port = serialport::new(&path.port_name, 115200)
            .open()
            .map_err(AxdlError::SerialError)?;
        Ok(SerialDevice { port })
    }
}

#[derive(Debug)]
pub struct SerialDevice {
    port: Box<dyn serialport::SerialPort>,
}

impl Device for SerialDevice {
    fn read_timeout(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize, AxdlError> {
        self.port
            .set_timeout(timeout)
            .map_err(AxdlError::SerialError)?;
        self.port
            .read(buf)
            .map_err(|e| AxdlError::IoError("read error".into(), e))
    }
    fn write_timeout(&mut self, buf: &[u8], timeout: Duration) -> Result<usize, AxdlError> {
        self.port
            .set_timeout(timeout)
            .map_err(AxdlError::SerialError)?;
        self.port
            .write(buf)
            .map_err(|e| AxdlError::IoError("write error".into(), e))
    }
}

