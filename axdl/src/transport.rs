use std::time::Duration;

use rusb::DeviceHandle;

use crate::AxdlError;

pub const VENDOR_ID: u16 = 0x32c9;
pub const PRODUCT_ID: u16 = 0x1000;
pub const ENDPOINT_OUT: u8 = 0x01;
pub const ENDPOINT_IN: u8 = 0x81;

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
    type DevicePath = UsbDevicePath;
    type DeviceType = UsbDevice;

    fn list_devices() -> Result<Vec<Self::DevicePath>, AxdlError> {
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

pub type DynDevice = Box<dyn Device>;
