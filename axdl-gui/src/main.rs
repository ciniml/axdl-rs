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
    download_image, transport::{AsyncTransport, DynDevice, Transport as _}, AxdlError, DownloadConfig, DownloadProgress
};

slint::include_modules!();

struct CliProgress {
    pb: Option<indicatif::ProgressBar>,
    last_description: String,
}

impl CliProgress {
    fn new() -> Self {
        Self {
            pb: None,
            last_description: String::new(),
        }
    }
}

impl axdl::DownloadProgress for CliProgress {
    fn is_cancelled(&self) -> bool {
        false
    }
    fn report_progress(&mut self, description: &str, progress: Option<f32>) {
        if let Some(progress) = progress {
            if self.pb.is_none() {
                let pb = indicatif::ProgressBar::new(100);
                pb.set_style(
                    indicatif::ProgressStyle::with_template(
                        "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}]",
                    )
                    .unwrap()
                    .progress_chars("#>-"),
                );
                self.pb = Some(pb);
            }
            self.pb
                .as_ref()
                .unwrap()
                .set_position((progress * 100.0) as u64);
        } else {
            if let Some(pb) = self.pb.take() {
                pb.finish();
            }
            tracing::info!("{}", description);
        }
        self.last_description = description.to_string();
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
    let usb_device: Rc<RefCell<Option<webusb_web::OpenUsbDevice>>> = Rc::new(RefCell::new(None));
    let serial = Rc::new(axdl::transport::webserial::new_serial().unwrap());
    let serial_device = Rc::new(RefCell::new(None));

    let ui = AppWindow::new()?;

    {
        let usb = usb.clone();
        let usb_device = usb_device.clone();
        let ui_handle = ui.as_weak();
        ui.on_open_usb_device(move || {
            let usb = usb.clone();
            let usb_device = usb_device.clone();
            let ui = ui_handle.unwrap();
            slint::spawn_local(async move {
                let result: Result<(), Box<dyn std::error::Error>> = async {
                    let device = usb.request_device([axdl::transport::webusb::axdl_device_filter()]).await?;
                    tracing::info!("Device selected: {:?}", device);
                    let open_device = device.open().await?;
                    tracing::info!("Device opened: {:?}", open_device);
                    usb_device.replace(Some(open_device));
                    ui.set_device_opened(true);
                    Ok(())
                }.await;

                if let Err(e) = result {
                    tracing::error!("Failed to open device: {:?}", e);
                    ui.set_device_opened(false);
                }
            });
        });
    }

    {
        let serial = serial.clone();
        let serial_device = serial_device.clone();
        let ui_handle = ui.as_weak();
        ui.on_open_serial_device(move || {
            let serial = serial.clone();
            let serial_device = serial_device.clone();
            let ui = ui_handle.unwrap();
            slint::spawn_local(async move {
                let result: Result<(), Box<dyn std::error::Error>> = async {
                    let options = web_sys::SerialPortRequestOptions::new();
                    options.set_filters(&js_sys::Array::of1(&axdl::transport::webserial::axdl_device_filter()));
                    let promise = serial.request_port_with_options(&options);
                    let device =  web_sys::SerialPort::from(wasm_bindgen_futures::JsFuture::from(promise).await
                        .map_err(AxdlError::WebSerialError)?);
                    tracing::info!("Device selected: {:?}", device);
                    wasm_bindgen_futures::JsFuture::from(device.open(&web_sys::SerialOptions::new(115200))).await
                        .map_err(AxdlError::WebSerialError)?;
                    tracing::info!("Device opened: {:?}", device);
                    serial_device.replace(Some(axdl::transport::webserial::WebSerialDevice::new(device)));
                    ui.set_device_opened(true);
                    Ok(())
                }.await;

                if let Err(e) = result {
                    tracing::error!("Failed to open device: {:?}", e);
                    ui.set_device_opened(false);
                }
            });
        });
    }

    ui.run()?;

    Ok(())
}


#[cfg_attr(target_arch = "wasm32",
           wasm_bindgen::prelude::wasm_bindgen(start))]
fn main() {
    gui_main().unwrap();
}