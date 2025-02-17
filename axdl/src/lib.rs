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

use transport::AsyncDevice;

pub mod communication;
pub mod frame;
pub mod partition;
pub mod transport;

#[derive(Debug, thiserror::Error)]
pub enum AxdlError {
    #[cfg(feature = "usb")]
    #[error("USB error: {0}")]
    UsbError(rusb::Error),
    #[cfg(feature = "serial")]
    #[error("Serial communication error: {0}")]
    SerialError(serialport::Error),
    #[cfg(feature = "webusb")]
    #[error("WebUSB error: {0}")]
    WebUsbError(webusb_web::Error),
    #[cfg(feature = "webusb")]
    #[error("WebSerial error: {0:?}")]
    WebSerialError(js_sys::wasm_bindgen::JsValue),
    #[error("Invalid frame received")]
    InvalidFrame,
    #[error("Failed to decode handshake: {0}")]
    HandshakeDecodeError(std::str::Utf8Error),
    #[error("Unexpected handshake: {0}")]
    UnexpectedHandshake(String),
    #[error("Frame has no payload")]
    NoPayload,
    #[error("Unexpected response: {0:02X}")]
    UnexpectedResponse(u16),
    #[error("IO Error: {0}, {1}")]
    IoError(String, std::io::Error),
    #[error("AXP image zip error: {0}")]
    ImageZipError(#[from] zip::result::ZipError),
    #[cfg(feature = "async")]
    #[error("AXP image zip error: {0}")]
    ImageAsyncZipError(#[from] async_zip::error::ZipError),
    #[error("Image error: {0}")]
    ImageError(String),
    #[error("Device not found")]
    DeviceNotFound,
    #[error("Device timeout")]
    DeviceTimeout,
    #[error("User cancelled the operation")]
    UserCancelled,
    #[error("Unsupported: {0}")]
    Unsupported(String),
}

#[derive(Debug)]
pub struct DownloadConfig {
    pub exclude_rootfs: bool,
}

pub trait DownloadProgress {
    fn is_cancelled(&self) -> bool;
    fn report_progress(&mut self, description: &str, progress: Option<f32>);

    fn check_is_cancelled(&self) -> Result<(), AxdlError> {
        if self.is_cancelled() {
            Err(AxdlError::UserCancelled)
        } else {
            Ok(())
        }
    }
}

pub fn download_image<R: std::io::Read + std::io::Seek, Progress: DownloadProgress>(
    image_reader: &mut R,
    device: &mut transport::DynDevice,
    config: &DownloadConfig,
    progress: &mut Progress,
) -> Result<(), AxdlError> {
    // Open the specified image file and find the configuration XML file.
    let mut archive = zip::ZipArchive::new(image_reader).map_err(AxdlError::ImageZipError)?;
    let mut config_string = None;

    progress.report_progress("Loading the AXP image configuration", None);
    // Load the axp image configuration.
    let project = {
        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            if file.name().ends_with(".xml") {
                config_string = Some(String::new());
                std::io::Read::read_to_string(&mut file, config_string.as_mut().unwrap()).map_err(
                    |e| AxdlError::ImageError(format!("failed to read configuration file: {}", e)),
                )?;
                break;
            }
        }
        let config_string = config_string.ok_or(AxdlError::ImageError(
            "configuration file not found in the image".into(),
        ))?;
        let config: partition::deserialize::Config = serde_xml_rs::from_str(&config_string)
            .map_err(|e| {
                AxdlError::ImageError(format!("failed to parse the configuration file: {}", e))
            })?;
        partition::Project::from(config.project)
    };

    tracing::debug!("{:#?}", project);
    let partition_table = project.partition_table();
    tracing::debug!("{:#?}", partition_table);

    tracing::debug!("Starting the download process...");
    progress.report_progress("Start download", None);

    // Check if romcode is running on the device.
    progress.report_progress("Handshaking with the device", None);
    communication::wait_handshake(device, "romcode")?;

    progress.report_progress("Downloading the flash downloaders", None);
    // Find the FDL1 image and download it.
    let fdl1_image = project
        .images()
        .iter()
        .find(|image| image.name() == "FDL1")
        .ok_or(AxdlError::ImageError("FDL1 image not found".into()))?;
    let fdl1_image_file = fdl1_image.file().ok_or(AxdlError::ImageError(
        "FDL1 image file not specified in the project".into(),
    ))?;
    let mut fdl1 = archive.by_name(fdl1_image_file).map_err(|e| {
        AxdlError::ImageError(format!("FDL1 image was not found in the image file: {}", e))
    })?;
    let fdl1_address = match fdl1_image.block() {
        partition::Block::Absolute(address) => address,
        _ => return Err(AxdlError::ImageError("FDL1 block is not absolute".into())),
    };

    // Start the RAM download (FDL1)
    communication::start_ram_download(device)?;
    let fdl1_image_size = fdl1.size();
    communication::start_partition_absolute_32(
        device,
        *fdl1_address as u32,
        fdl1_image_size as u32,
    )?;
    communication::write_image(
        device,
        &mut fdl1,
        1000,
        "FDL1",
        fdl1_image_size as usize,
        Some(100),
        progress,
    )?;
    drop(fdl1);
    communication::end_partition(device, communication::TIMEOUT)?;
    communication::end_ram_download(device)?;

    communication::wait_handshake(device, "fdl1")?;

    // Find the FDL2 image and download it.
    let fdl2_image = project
        .images()
        .iter()
        .find(|image| image.name() == "FDL2")
        .ok_or(AxdlError::ImageError("FDL2 image not found".into()))?;
    let fdl2_image_file = fdl2_image.file().ok_or(AxdlError::ImageError(
        "FDL2 image file not specified in the project".into(),
    ))?;
    let mut fdl2 = archive.by_name(fdl2_image_file).map_err(|e| {
        AxdlError::ImageError(format!("FDL2 image was not found in the image file: {}", e))
    })?;
    let fdl2_address = match fdl2_image.block() {
        partition::Block::Absolute(address) => address,
        _ => return Err(AxdlError::ImageError("FDL2 block is not absolute".into())),
    };
    // Start the RAM download (FDL2)
    communication::start_ram_download(device)?;

    let fdl2_image_size = fdl2.size();
    communication::start_partition_absolute(device, *fdl2_address, fdl2_image_size)?;
    communication::write_image(
        device,
        &mut fdl2,
        1000,
        "FDL2",
        fdl2_image_size as usize,
        Some(100),
        progress,
    )?;
    drop(fdl2);
    communication::end_partition(device, communication::TIMEOUT)?;
    communication::end_ram_download(device)?;

    // Download the partition table.
    progress.report_progress("Downloading the partition table", None);
    communication::set_partition_table(device, &partition_table)?;

    // Download all of "CODE" images
    for image in project.images().iter().filter(|image| {
        image.r#type() == partition::ImageType::Code
            && (!config.exclude_rootfs || image.name() != "ROOTFS")
    }) {
        tracing::debug!("Downloading image: {}", image.name());
        progress.report_progress(&format!("Downloading image {}", image.name()), None);

        progress.check_is_cancelled()?;

        let image_file_name = image.file().ok_or(AxdlError::ImageError(format!(
            "image {} file not specified in the project",
            image.name()
        )))?;
        let mut image_data = archive.by_name(&image_file_name).map_err(|e| {
            AxdlError::ImageError(format!(
                "image {} was not found in the archive: {}",
                image.name(),
                e
            ))
        })?;
        let image_id = match image.block() {
            partition::Block::Partition(id) => id,
            _ => {
                return Err(AxdlError::ImageError(format!(
                    "image {} block is not partition",
                    image.name()
                )))
            }
        };
        let image_data_size = image_data.size();
        communication::start_partition_id(device, &image_id, image_data_size)?;
        communication::write_image(
            device,
            &mut image_data,
            48000,
            image.name(),
            image_data_size as usize,
            Some(100),
            progress,
        )?;
        communication::end_partition(device, Duration::from_secs(60))?;
    }
    tracing::info!("Done");
    Ok(())
}

type AsyncZipEntryReaderWithEntry<'a, R> =
    async_zip::base::read::ZipEntryReader<'a, R, async_zip::base::read::WithEntry<'a>>;

async fn read_zip_entry_as_string<
    R: futures_io::AsyncBufRead + futures_io::AsyncSeek + Unpin,
    F: Fn(&async_zip::ZipEntry) -> bool,
>(
    archive: &mut async_zip::base::read::seek::ZipFileReader<R>,
    predicate: F,
) -> Result<Option<String>, AxdlError> {
    for i in 0.. {
        match archive.reader_with_entry(i).await {
            Ok(mut reader) => {
                if predicate(reader.entry()) {
                    let mut config_string = String::new();
                    reader
                        .read_to_string_checked(&mut config_string)
                        .await
                        .map_err(AxdlError::ImageAsyncZipError)?;
                    return Ok(Some(config_string));
                }
            }
            Err(async_zip::error::ZipError::EntryIndexOutOfBounds) => break,
            Err(e) => return Err(AxdlError::ImageAsyncZipError(e.into())),
        }
    }
    Ok(None)
}

enum WriteImagePartition {
    Absolute32(u32),
    Absolute64(u64),
    PartitionId(String),
}

async fn write_partition_from_zip_file_async<
    R: futures_io::AsyncBufRead + futures_io::AsyncSeek + Unpin,
    D: AsyncDevice,
>(
    device: &mut D,
    archive: &mut async_zip::base::read::seek::ZipFileReader<R>,
    image_name: &str,
    partition: &WriteImagePartition,
    file_name: &str,
    chunk_size: usize,
    report_every: Option<usize>,
    progress: &mut impl DownloadProgress,
) -> Result<(), AxdlError> {
    for i in 0.. {
        match archive.reader_with_entry(i).await {
            Ok(mut reader) => {
                if reader
                    .entry()
                    .filename()
                    .as_str()
                    .map(|s| s == file_name)
                    .unwrap_or(false)
                {
                    let image_size = reader.entry().uncompressed_size();
                    match partition {
                        WriteImagePartition::Absolute32(address) => {
                            communication::r#async::start_partition_absolute_32(
                                device,
                                *address,
                                image_size as u32,
                            )
                            .await?;
                        }
                        WriteImagePartition::Absolute64(address) => {
                            communication::r#async::start_partition_absolute(
                                device, *address, image_size,
                            )
                            .await?;
                        }
                        WriteImagePartition::PartitionId(id) => {
                            communication::r#async::start_partition_id(device, id, image_size)
                                .await?;
                        }
                    }
                    communication::r#async::write_image(
                        device,
                        &mut reader,
                        chunk_size,
                        image_name,
                        image_size as usize,
                        report_every,
                        progress,
                    )
                    .await?;
                    communication::r#async::end_partition(device).await?;
                    return Ok(());
                }
            }
            Err(async_zip::error::ZipError::EntryIndexOutOfBounds) => break,
            Err(e) => return Err(AxdlError::ImageAsyncZipError(e.into())),
        }
    }
    Err(AxdlError::ImageError(format!(
        "image was not found in the image file: {}",
        file_name
    )))
}

pub async fn download_image_async<
    R: futures_io::AsyncBufRead + futures_io::AsyncSeek + Unpin,
    D: AsyncDevice,
    Progress: DownloadProgress,
>(
    image_reader: &mut R,
    device: &mut D,
    config: &DownloadConfig,
    progress: &mut Progress,
) -> Result<(), AxdlError> {
    tracing::info!("download_image_async");
    // Open the specified image file and find the configuration XML file.
    let mut archive = async_zip::base::read::seek::ZipFileReader::new(image_reader)
        .await
        .map_err(AxdlError::ImageAsyncZipError)?;
    tracing::info!("image file opened");
    progress.report_progress("Loading the AXP image configuration", None);
    // Load the axp image configuration.
    let project = {
        let config_string = read_zip_entry_as_string(&mut archive, |entry| {
            entry
                .filename()
                .as_str()
                .map(|s| s.ends_with(".xml"))
                .unwrap_or(false)
        })
        .await?
        .ok_or(AxdlError::ImageError(
            "configuration file not found in the image".into(),
        ))?;
        let config: partition::deserialize::Config = serde_xml_rs::from_str(&config_string)
            .map_err(|e| {
                AxdlError::ImageError(format!("failed to parse the configuration file: {}", e))
            })?;
        partition::Project::from(config.project)
    };

    tracing::debug!("{:#?}", project);
    let partition_table = project.partition_table();
    tracing::debug!("{:#?}", partition_table);

    tracing::debug!("Starting the download process...");
    progress.report_progress("Start download", None);

    // Check if romcode is running on the device.
    progress.report_progress("Handshaking with the device", None);
    communication::r#async::wait_handshake(device, "romcode").await?;

    progress.report_progress("Downloading the flash downloaders", None);
    // Find the FDL1 image and download it.
    let fdl1_image = project
        .images()
        .iter()
        .find(|image| image.name() == "FDL1")
        .ok_or(AxdlError::ImageError("FDL1 image not found".into()))?;
    let fdl1_image_file = fdl1_image.file().ok_or(AxdlError::ImageError(
        "FDL1 image file not specified in the project".into(),
    ))?;
    let fdl1_address = match fdl1_image.block() {
        partition::Block::Absolute(address) => address,
        _ => return Err(AxdlError::ImageError("FDL1 block is not absolute".into())),
    };

    // Start the RAM download (FDL1)
    communication::r#async::start_ram_download(device).await?;
    write_partition_from_zip_file_async(
        device,
        &mut archive,
        "FDL1",
        &WriteImagePartition::Absolute32(*fdl1_address as u32),
        fdl1_image_file,
        1000,
        Some(100),
        progress,
    )
    .await?;
    communication::r#async::end_ram_download(device).await?;

    communication::r#async::wait_handshake(device, "fdl1").await?;

    // Find the FDL2 image and download it.
    let fdl2_image = project
        .images()
        .iter()
        .find(|image| image.name() == "FDL2")
        .ok_or(AxdlError::ImageError("FDL2 image not found".into()))?;
    let fdl2_image_file = fdl2_image.file().ok_or(AxdlError::ImageError(
        "FDL2 image file not specified in the project".into(),
    ))?;
    let fdl2_address = match fdl2_image.block() {
        partition::Block::Absolute(address) => address,
        _ => return Err(AxdlError::ImageError("FDL2 block is not absolute".into())),
    };
    // Start the RAM download (FDL2)
    communication::r#async::start_ram_download(device).await?;
    write_partition_from_zip_file_async(
        device,
        &mut archive,
        "FDL2",
        &WriteImagePartition::Absolute64(*fdl2_address),
        fdl2_image_file,
        1000,
        Some(100),
        progress,
    )
    .await?;
    communication::r#async::end_ram_download(device).await?;

    // Download the partition table.
    progress.report_progress("Downloading the partition table", None);
    communication::r#async::set_partition_table(device, &partition_table).await?;

    // Download all of "CODE" images
    for image in project.images().iter().filter(|image| {
        image.r#type() == partition::ImageType::Code
            && (!config.exclude_rootfs || image.name() != "ROOTFS")
    }) {
        tracing::debug!("Downloading image: {}", image.name());
        progress.report_progress(&format!("Downloading image {}", image.name()), None);

        progress.check_is_cancelled()?;

        let image_file_name = image.file().ok_or(AxdlError::ImageError(format!(
            "image {} file not specified in the project",
            image.name()
        )))?;

        let image_id = match image.block() {
            partition::Block::Partition(id) => id,
            _ => {
                return Err(AxdlError::ImageError(format!(
                    "image {} block is not partition",
                    image.name()
                )))
            }
        };

        write_partition_from_zip_file_async(
            device,
            &mut archive,
            image.name(),
            &WriteImagePartition::PartitionId(image_id.clone()),
            image_file_name,
            48000,
            Some(100),
            progress,
        )
        .await?;
    }
    tracing::info!("Done");
    Ok(())
}
