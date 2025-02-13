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

use std::time::Duration;

use axdl::{
    download_image,
    transport::{DynDevice, Transport as _},
    DownloadConfig, DownloadProgress,
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
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(tracing::level_filters::LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .with_file(true)
        .with_line_number(true)
        .init();

    let ui = AppWindow::new()?;

    ui.on_request_increase_value({
        let ui_handle = ui.as_weak();
        move || {
            let ui = ui_handle.unwrap();
            ui.set_counter(ui.get_counter() + 1);
        }
    });

    ui.run()?;

    Ok(())
}


#[cfg_attr(target_arch = "wasm32",
           wasm_bindgen::prelude::wasm_bindgen(start))]
fn main() {
    gui_main().unwrap();
}