// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Kenta Ida
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{cell::RefCell, mem::forget, rc::Rc, time::Duration};

use axdl::{
    download_image,
    transport::{AsyncTransport, DynDevice, Transport as _},
    AxdlError, DownloadConfig, DownloadProgress,
};
use js_sys::wasm_bindgen::{self, JsCast};
use tracing_subscriber::layer::SubscriberExt;
use tracing_wasm::WASMLayerConfig;

slint::include_modules!();

struct GuiProgress {
    ui: slint::Weak<AppWindow>,
    cancelled: bool,
}

impl GuiProgress {
    fn new(ui: slint::Weak<AppWindow>) -> Self {
        Self {
            ui,
            cancelled: false,
        }
    }

    fn set_cancelled(&mut self, cancelled: bool) {
        self.cancelled = cancelled;
    }
}

impl axdl::DownloadProgress for GuiProgress {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }
    fn report_progress(&mut self, description: &str, progress: Option<f32>) {
        let ui = self.ui.clone();
        let description = description.to_string();
        let _ = slint::invoke_from_event_loop(move || {
            let ui = ui.unwrap();
            let progress = progress.unwrap_or(-1.0);
            ui.invoke_set_progress(description.into(), progress);
        });
    }
}

enum AxdlDevice {
    Serial(axdl::transport::webserial::WebSerialDevice),
    Usb(webusb_web::OpenUsbDevice),
}

impl axdl::transport::AsyncDevice for AxdlDevice {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, AxdlError> {
        match self {
            AxdlDevice::Serial(device) => device.read(buf).await,
            AxdlDevice::Usb(device) => device.read(buf).await,
        }
    }

    async fn write(&mut self, buf: &[u8]) -> Result<usize, AxdlError> {
        match self {
            AxdlDevice::Serial(device) => device.write(buf).await,
            AxdlDevice::Usb(device) => device.write(buf).await,
        }
    }
}

#[pin_project::pin_project]
struct BufReader<R: futures_io::AsyncRead + futures_io::AsyncSeek> {
    #[pin]
    reader: R,
    buf: Vec<u8>,
    pos: usize,
    filled: usize,
}

impl<R: futures_io::AsyncRead + futures_io::AsyncSeek> BufReader<R> {
    fn new(reader: R, buffer_size: usize) -> Self {
        let mut buf = Vec::with_capacity(buffer_size);
        buf.resize(buffer_size, 0);
        Self {
            reader,
            buf,
            pos: 0,
            filled: 0,
        }
    }
}

impl<R: futures_io::AsyncRead + futures_io::AsyncSeek> futures_io::AsyncRead for BufReader<R> {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut [u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let bytes_remaining = self.filled - self.pos;
        tracing::debug!(
            "poll_read size: {}, bytes_remaining: {}",
            buf.len(),
            bytes_remaining
        );

        let this = self.project();
        if bytes_remaining > 0 {
            let bytes_to_copy = bytes_remaining.min(buf.len());
            buf[..bytes_to_copy].copy_from_slice(&this.buf[*this.pos..*this.pos + bytes_to_copy]);
            *this.pos += bytes_to_copy;
            if *this.pos == *this.filled {
                *this.filled = 0;
                *this.pos = 0;
            }
            return std::task::Poll::Ready(Ok(bytes_to_copy));
        }
        this.reader.poll_read(cx, buf)
    }
}

impl<R: futures_io::AsyncRead + futures_io::AsyncSeek> futures_io::AsyncSeek for BufReader<R> {
    fn poll_seek(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        pos: std::io::SeekFrom,
    ) -> std::task::Poll<std::io::Result<u64>> {
        tracing::debug!("poll_seek: {:?}", pos);
        let this = self.project();
        *this.filled = 0;
        *this.pos = 0;

        this.reader.poll_seek(cx, pos)
    }
}

impl<R: futures_io::AsyncRead + futures_io::AsyncSeek> futures_io::AsyncBufRead for BufReader<R> {
    fn poll_fill_buf(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<&[u8]>> {
        tracing::debug!("poll_fill_buf");
        let bytes_remaining_in_buf = self.filled - self.pos;
        let mut this = self.project();
        if bytes_remaining_in_buf > 0 {
            std::task::Poll::Ready(Ok(&this.buf[*this.pos..*this.filled]))
        } else {
            *this.filled = 0;
            *this.pos = 0;

            match this.reader.poll_read(cx, &mut this.buf) {
                std::task::Poll::Ready(Ok(bytes_read)) => {
                    *this.filled = bytes_read;
                    std::task::Poll::Ready(Ok(&this.buf[..*this.filled]))
                }
                std::task::Poll::Ready(Err(e)) => std::task::Poll::Ready(Err(e)),
                std::task::Poll::Pending => std::task::Poll::Pending,
            }
        }
    }
    fn consume(self: std::pin::Pin<&mut Self>, amt: usize) {
        tracing::debug!("consume: {}", amt);
        let this = self.project();
        *this.pos += amt;
    }
}

#[pin_project::pin_project]
struct FileWrapper<'a> {
    reader: web_sys::FileReader,
    file: &'a web_sys::File,
    position: u64,
    slice: Option<web_sys::Blob>,
}

impl<'a> FileWrapper<'a> {
    fn new(file: &'a web_sys::File) -> Self {
        Self {
            reader: web_sys::FileReader::new().unwrap(),
            file,
            position: 0,
            slice: None,
        }
    }
}

impl<'a> futures_io::AsyncRead for FileWrapper<'a> {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut [u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let size = self.file.size() as u64;
        let remaining = size.saturating_sub(self.position);
        let bytes_to_read = remaining.min(buf.len() as u64);

        tracing::debug!(
            "poll_read position: {}, bytes_to_read: {}, remaining: {}",
            self.position,
            bytes_to_read,
            remaining
        );

        if bytes_to_read == 0 {
            return std::task::Poll::Ready(Ok(0));
        }

        let this = self.project();
        if this.slice.is_some() {
            match this.reader.ready_state() {
                web_sys::FileReader::LOADING => {
                    tracing::debug!("poll_read: LOADING");
                    return std::task::Poll::Pending;
                }
                web_sys::FileReader::DONE => {
                    tracing::debug!("poll_read: DONE");
                    *this.slice = None;
                    let result = this.reader.result().unwrap();
                    let array = js_sys::Uint8Array::new(&result);
                    let bytes_read = array.length() as usize;
                    let bytes_to_copy = bytes_read.min(buf.len());
                    array.copy_to(&mut buf[..bytes_to_copy]);
                    tracing::debug!(
                        "poll_read: DONE bytes_read {} bytes_to_copy {}",
                        bytes_read,
                        bytes_to_copy
                    );
                    *this.position += bytes_to_copy as u64;
                    return std::task::Poll::Ready(Ok(bytes_to_copy));
                }
                _ => unreachable!(),
            }
        }

        tracing::debug!("poll_read: EMPTY");

        let slice = this
            .file
            .slice_with_f64_and_f64(
                *this.position as f64,
                (*this.position + bytes_to_read) as f64,
            )
            .map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("failed to create slice - {:?}", e),
                )
            });
        let slice = match slice {
            Ok(slice) => slice,
            Err(e) => return std::task::Poll::Ready(Err(e)),
        };
        let waker = cx.waker().clone();
        let wake_closure = wasm_bindgen::closure::Closure::wrap(Box::new(move || {
            //tracing::debug!("poll_read: onloadend");
            waker.wake_by_ref();
        }) as Box<dyn FnMut()>);
        this.reader
            .set_onloadend(Some(wake_closure.as_ref().unchecked_ref()));
        forget(wake_closure);

        if let Err(e) = this.reader.read_as_array_buffer(&slice) {
            return std::task::Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("read error - {:?}", e),
            )));
        }
        *this.slice = Some(slice);
        std::task::Poll::Pending
    }
}

impl<'a> futures_io::AsyncSeek for FileWrapper<'a> {
    fn poll_seek(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        pos: std::io::SeekFrom,
    ) -> std::task::Poll<std::io::Result<u64>> {
        let size = self.file.size() as u64;
        match pos {
            std::io::SeekFrom::Start(pos) => {
                self.position = pos.min(size);
            }
            std::io::SeekFrom::End(pos) => {
                self.position = size.saturating_add_signed(pos).min(size);
            }
            std::io::SeekFrom::Current(pos) => {
                self.position = self.position.saturating_add_signed(pos).min(size);
            }
        }
        std::task::Poll::Ready(Ok(self.position))
    }
}

impl<'a> std::io::Read for FileWrapper<'a> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let size = self.file.size() as u64;
        let remaining = size.saturating_sub(self.position);
        let bytes_to_read = remaining.min(buf.len() as u64);

        if bytes_to_read == 0 {
            return Ok(0);
        }

        let slice = self
            .file
            .slice_with_f64_and_f64(self.position as f64, (self.position + bytes_to_read) as f64)
            .map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("failed to create slice - {:?}", e),
                )
            })?;
        let reader = web_sys::FileReaderSync::new().map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("failed to create FileReaderSync - {:?}", e),
            )
        })?;
        let data = reader.read_as_array_buffer(&slice).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("read error - {:?}", e),
            )
        })?;
        let data = js_sys::Uint8Array::new(&data);
        let bytes_read = data.byte_length() as usize;
        let bytes_to_copy = bytes_read.min(buf.len());
        data.copy_to(&mut buf[..bytes_to_copy]);

        self.position += bytes_to_copy as u64;
        Ok(bytes_to_copy)
    }
}

impl<'a> std::io::Seek for FileWrapper<'a> {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        let size = self.file.size() as u64;
        let last = size.saturating_sub(1);
        match pos {
            std::io::SeekFrom::Start(pos) => {
                self.position = pos.min(last);
            }
            std::io::SeekFrom::End(pos) => {
                self.position = size.saturating_add_signed(pos).min(last);
            }
            std::io::SeekFrom::Current(pos) => {
                self.position = self.position.saturating_add_signed(pos).min(last);
            }
        }
        Ok(self.position)
    }
}

fn gui_main() -> Result<(), Box<dyn std::error::Error>> {
    let tracing_layer = tracing_wasm::WASMLayer::new(
        tracing_wasm::WASMLayerConfigBuilder::default()
            .set_max_level(tracing::Level::INFO)
            .build(),
    );
    let subscriber = tracing_subscriber::registry().with(tracing_layer);
    tracing::subscriber::set_global_default(subscriber).unwrap();

    let usb = Rc::new(webusb_web::Usb::new().unwrap());
    let serial = Rc::new(axdl::transport::webserial::new_serial().unwrap());
    let axdl_device: Rc<RefCell<Option<AxdlDevice>>> = Rc::new(RefCell::new(None));
    let image_file = Rc::new(RefCell::new(None));

    let ui = AppWindow::new()?;

    {
        let usb = usb.clone();
        let axdl_device = axdl_device.clone();
        let ui_handle = ui.as_weak();
        ui.on_open_usb_device(move || {
            let usb = usb.clone();
            let axdl_device = axdl_device.clone();
            let ui = ui_handle.unwrap();
            slint::spawn_local(async move {
                let result: Result<(), Box<dyn std::error::Error>> = async {
                    let device = usb
                        .request_device([axdl::transport::webusb::axdl_device_filter()])
                        .await?;
                    tracing::info!("Device selected: {:?}", device);
                    let open_device = device.open().await?;
                    tracing::info!("Device opened: {:?}", open_device);
                    open_device.claim_interface(0).await?;
                    axdl_device.replace(Some(AxdlDevice::Usb(open_device)));
                    ui.set_device_opened(true);
                    Ok(())
                }
                .await;

                if let Err(e) = result {
                    tracing::error!("Failed to open device: {:?}", e);
                    ui.set_device_opened(false);
                }
            });
        });
    }

    {
        let serial = serial.clone();
        let axdl_device = axdl_device.clone();
        let ui_handle = ui.as_weak();
        ui.on_open_serial_device(move || {
            let serial = serial.clone();
            let axdl_device = axdl_device.clone();
            let ui = ui_handle.unwrap();
            slint::spawn_local(async move {
                let result: Result<(), Box<dyn std::error::Error>> = async {
                    let options = web_sys::SerialPortRequestOptions::new();
                    options.set_filters(&js_sys::Array::of1(
                        &axdl::transport::webserial::axdl_device_filter(),
                    ));
                    let promise = serial.request_port_with_options(&options);
                    let device = web_sys::SerialPort::from(
                        wasm_bindgen_futures::JsFuture::from(promise)
                            .await
                            .map_err(AxdlError::WebSerialError)?,
                    );
                    tracing::info!("Device selected: {:?}", device);
                    let options = web_sys::SerialOptions::new(115200);
                    options.set_buffer_size(48000);
                    wasm_bindgen_futures::JsFuture::from(device.open(&options))
                        .await
                        .map_err(AxdlError::WebSerialError)?;
                    tracing::info!("Device opened: {:?}", device);
                    axdl_device.replace(Some(AxdlDevice::Serial(
                        axdl::transport::webserial::WebSerialDevice::new(device),
                    )));
                    ui.set_device_opened(true);
                    Ok(())
                }
                .await;

                if let Err(e) = result {
                    tracing::error!("Failed to open device: {:?}", e);
                    ui.set_device_opened(false);
                }
            });
        });
    }

    {
        let ui_handle = ui.as_weak();
        let image_file = image_file.clone();
        ui.on_open_image(move || {
            let ui = ui_handle.unwrap();
            let image_file = image_file.clone();
            slint::spawn_local(async move {
                let result: Result<(), Box<dyn std::error::Error>> = async {
                    let file = rfd::AsyncFileDialog::new()
                        .add_filter("AXDL Image", &["*.axp"])
                        .pick_file()
                        .await
                        .inspect(|path| {
                            tracing::info!("Selected file: {}", path.file_name());
                        });

                    ui.set_image_file_opened(file.is_some());
                    ui.set_image_file(
                        file.as_ref()
                            .map(|f| f.file_name())
                            .unwrap_or_default()
                            .into(),
                    );
                    *image_file.borrow_mut() = file;
                    Ok(())
                }
                .await;

                if let Err(e) = result {
                    tracing::error!("Failed to open image file: {:?}", e);
                    ui.set_image_file_opened(false);
                }
            });
        });
    }

    {
        let ui_handle = ui.as_weak();
        let image_file = image_file.clone();
        let axdl_device = axdl_device.clone();

        ui.on_download(move || {
            let ui_handle = ui_handle.clone();
            let ui = ui_handle.unwrap();
            if axdl_device.borrow().is_none() || image_file.borrow().is_none() {
                tracing::error!("Device or image file is not selected");
                return;
            }

            let image_file = image_file.clone();
            let axdl_device = axdl_device.clone();

            ui.set_downloading(true);

            slint::spawn_local(async move {
                let result: Result<(), Box<dyn std::error::Error>> = async {
                    let mut progress = GuiProgress::new(ui_handle.clone());
                    let config = DownloadConfig {
                        exclude_rootfs: ui.get_exclude_rootfs(),
                    };
                    let image_file_ref = image_file.borrow();
                    let file = FileWrapper::new(image_file_ref.as_ref().unwrap().inner());
                    let mut buf_file = BufReader::new(file, 1048576);

                    tracing::info!("Start downloading image file");
                    let result = axdl::download_image_async(
                        &mut buf_file,
                        axdl_device.borrow_mut().as_mut().unwrap(),
                        &config,
                        &mut progress,
                    )
                    .await?;
                    Ok(())
                }
                .await;

                ui.set_downloading(false);

                if let Err(e) = result {
                    tracing::error!("Failed to download image file: {:?}", e);
                    ui.invoke_set_progress(
                        format!("Failed to download image file: {:?}", e).into(),
                        -1.0,
                    );
                } else {
                    ui.invoke_set_progress("Done".into(), -1.0);
                }
            });
        });
    }

    ui.run()?;

    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(start))]
fn main() {
    gui_main().unwrap();
}
