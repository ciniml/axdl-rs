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

use std::time::Duration;

use axdl::{
    download_image,
    transport::{DynDevice, Transport as _},
    DownloadConfig, DownloadProgress,
};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Transport {
    #[default]
    Usb,
    Serial,
}
impl std::str::FromStr for Transport {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "usb" => Ok(Self::Usb),
            "serial" => Ok(Self::Serial),
            _ => Err(format!("Unknown transport method: {}", s)),
        }
    }
}

/// command line arguments
#[derive(Debug, clap::Parser)]
struct Args {
    #[clap(short, long, help = "AXP image file")]
    file: std::path::PathBuf,
    #[clap(
        short,
        long,
        help = "Exclude root filesystem from the download operation"
    )]
    exclude_rootfs: bool,
    #[clap(short, long, help = "Wait for the device to be ready")]
    wait_for_device: bool,
    #[clap(long, help = "Timeout for waiting for the device to be ready")]
    wait_for_device_timeout_secs: Option<u64>,
    #[clap(
        short,
        long,
        help = "Specify the transport method",
        default_value = "usb"
    )]
    transport: Transport,
}

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

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(tracing::level_filters::LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .with_file(true)
        .with_line_number(true)
        .init();

    // Parse command line arguments.
    let args: Args = <Args as clap::Parser>::parse();

    // Open the specified image file and find the configuration XML file.
    let mut file = std::fs::File::open(&args.file)?;
    let config = DownloadConfig {
        exclude_rootfs: args.exclude_rootfs,
    };

    let mut progress = CliProgress::new();

    if args.wait_for_device {
        if let Some(timeout) = args.wait_for_device_timeout_secs {
            tracing::debug!(
                "Waiting for the device to be ready (timeout={}s)...",
                timeout
            );
            progress.report_progress(
                &format!("Waiting for the device to be ready (timeout={}s)", timeout),
                None,
            );
        } else {
            tracing::debug!("Waiting for the device to be ready...");
            progress.report_progress("Waiting for the device to be ready", None);
        }
    }

    let wait_start = std::time::Instant::now();
    let mut device = loop {
        let device: Option<DynDevice> = match args.transport {
            Transport::Serial => {
                axdl::transport::SerialTransport::list_devices()?
                    .iter()
                    .next()
                    .map(|path| axdl::transport::SerialTransport::open_device(path).ok())
                    .flatten()
                    .map(|device| {
                        let device: DynDevice = Box::new(device);
                        device
                    })
            }
            Transport::Usb => axdl::transport::UsbTransport::list_devices()?
                .iter()
                .next()
                .map(|path| axdl::transport::UsbTransport::open_device(path).ok())
                .flatten()
                .map(|device| {
                    let device: DynDevice = Box::new(device);
                    device
                }),
        };

        if let Some(device) = device {
            break device;
        }

        if args.wait_for_device {
            if let Some(timeout) = args.wait_for_device_timeout_secs {
                if wait_start.elapsed() > Duration::from_secs(timeout) {
                    return Err(anyhow::anyhow!("Timeout waiting for the device"));
                }
            }
            std::thread::sleep(Duration::from_secs(1));
        } else {
            return Err(anyhow::anyhow!("Device not found"));
        }
    };

    // Perform download
    download_image(&mut file, &mut device, &config, &mut progress)?;

    Ok(())
}
