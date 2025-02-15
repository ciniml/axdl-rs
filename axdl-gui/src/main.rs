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

use std::{cell::RefCell, rc::Rc, time::Duration};

use axdl::{
    download_image,
    transport::{AsyncTransport, DynDevice, Transport as _},
    AxdlError, DownloadConfig, DownloadProgress,
};

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

fn gui_main() -> Result<(), Box<dyn std::error::Error>> {
    // tracing_subscriber::fmt()
    //     .with_env_filter(
    //         tracing_subscriber::EnvFilter::builder()
    //             .with_default_directive(tracing::level_filters::LevelFilter::INFO.into())
    //             .from_env_lossy(),
    //     )
    //     .with_file(true)
    //     .with_line_number(true)
    //     .init();
    tracing_wasm::set_as_global_default();

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
                    wasm_bindgen_futures::JsFuture::from(
                        device.open(&web_sys::SerialOptions::new(115200)),
                    )
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
                    ui.set_image_file(file.map(|f| f.file_name()).unwrap_or_default().into());
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
            let ui = ui_handle.unwrap();

            if axdl_device.borrow().is_none() || image_file.borrow().is_none() {
                tracing::error!("Device or image file is not selected");
                return;
            }

            let image_file = image_file.clone();
            let axdl_device = axdl_device.clone();

            slint::spawn_local(async move {
                let result: Result<(), Box<dyn std::error::Error>> = async {
                    let mut progress = GuiProgress::new(ui_handle.clone());
                    let config = DownloadConfig {
                        exclude_rootfs: ui.get_exclude_rootfs(),
                    };
                    let result = axdl::download_image_async(
                        image_file.borrow().as_mut().unwrap(),
                        axdl_device.borrow().as_mut().unwrap(),
                        &config,
                        &mut progress,
                    )?;
                    Ok(())
                }
                .await;

                if let Err(e) = result {
                    tracing::error!("Failed to download image file: {:?}", e);
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
