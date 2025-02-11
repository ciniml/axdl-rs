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

use rusb::{DeviceHandle, UsbContext};
use std::time::Duration;

mod frame;
mod partition;

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
    wait_device: bool,
    #[clap(long, help = "Timeout for waiting for the device to be ready")]
    wait_device_timeout_secs: Option<u64>,
}

const VENDOR_ID: u16 = 0x32c9;
const PRODUCT_ID: u16 = 0x1000;
const ENDPOINT_OUT: u8 = 0x01;
const ENDPOINT_IN: u8 = 0x81;
const TIMEOUT: Duration = Duration::from_secs(1);

const HANDSHAKE_REQUEST: [u8; 3] = [0x3c, 0x3c, 0x3c];

fn wait_handshake(
    handle: &mut DeviceHandle<rusb::GlobalContext>,
    expected_handshake: &str,
) -> anyhow::Result<()> {
    handle
        .write_bulk(ENDPOINT_OUT, &HANDSHAKE_REQUEST, TIMEOUT)
        .map_err(|e| anyhow::anyhow!("send error: {}", e))?;
    let mut buf = [0u8; 64];
    let length = handle
        .read_bulk(ENDPOINT_IN, &mut buf, TIMEOUT)
        .map_err(|e| anyhow::anyhow!("receive error: {}", e))?;

    tracing::debug!("received: {:02X?}", &buf[..length]);
    let view = frame::AxdlFrameView::new(&buf[..length]);
    tracing::debug!(
        "view: {}, checksum={:04X}",
        view,
        view.calculate_checksum().unwrap_or(0)
    );
    if !view.is_valid() {
        return Err(anyhow::anyhow!("invalid frame"));
    }
    let handshake = view
        .payload()
        .map(|payload| {
            std::str::from_utf8(payload).map_err(|e| anyhow::anyhow!("invalid utf-8: {}", e))
        })
        .transpose()?
        .ok_or(anyhow::anyhow!("no payload"))?;

    tracing::debug!("handshake: {}", handshake);
    if !handshake.contains(expected_handshake) {
        return Err(anyhow::anyhow!("unexpected handshake: {}", handshake));
    }
    Ok(())
}

fn receive_response(
    handle: &mut DeviceHandle<rusb::GlobalContext>,
    timeout: Duration,
) -> anyhow::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(65536);
    buf.resize(buf.capacity(), 0);
    let length = handle
        .read_bulk(ENDPOINT_IN, &mut buf, timeout)
        .map_err(|e| anyhow::anyhow!("receive error: {}", e))?;

    tracing::debug!("received: {:02X?}", &buf[..length]);
    let view = frame::AxdlFrameView::new(&buf[..length]);
    tracing::debug!(
        "view: {}, checksum={:04X}",
        view,
        view.calculate_checksum().unwrap_or(0)
    );
    if !view.is_valid() {
        return Err(anyhow::anyhow!("invalid frame"));
    }

    buf.resize(length, 0);
    Ok(buf)
}

fn start_ram_download(handle: &mut DeviceHandle<rusb::GlobalContext>) -> anyhow::Result<()> {
    tracing::debug!("start_ram_download");
    let mut buf = [0u8; frame::MINIMUM_LENGTH];
    let mut frame = frame::AxdlFrameViewMut::new(&mut buf);
    frame.init();
    frame.finalize();

    handle
        .write_bulk(ENDPOINT_OUT, &buf, TIMEOUT)
        .map_err(|e| anyhow::anyhow!("send error: {}", e))?;

    let response = receive_response(handle, TIMEOUT)?;
    let response_view = frame::AxdlFrameView::new(&response);
    if response_view.command_response() != Some(0x0080) {
        return Err(anyhow::anyhow!(
            "unexpected response: {:04X?}",
            response_view.command_response()
        ));
    }
    Ok(())
}

fn start_partition_absolute_32(
    handle: &mut DeviceHandle<rusb::GlobalContext>,
    start_address: u32,
    partition_length: u32,
) -> anyhow::Result<()> {
    tracing::debug!(
        "start_partition_absolute: start_address={:#X}, partition_length={}",
        start_address,
        partition_length
    );
    let mut buf = [0u8; frame::MINIMUM_LENGTH + 8];
    let mut frame = frame::AxdlFrameViewMut::new(&mut buf);
    frame.init();
    frame.set_command_response(0x0001); // Start partition
    {
        let payload = frame.payload_mut();
        payload[0..4].copy_from_slice(&start_address.to_le_bytes());
        payload[4..8].copy_from_slice(&partition_length.to_le_bytes());
    }
    frame.finalize();

    handle
        .write_bulk(ENDPOINT_OUT, &buf, TIMEOUT)
        .map_err(|e| anyhow::anyhow!("send error: {}", e))?;

    let response = receive_response(handle, TIMEOUT)?;
    let response_view = frame::AxdlFrameView::new(&response);
    if response_view.command_response() != Some(0x0080) {
        return Err(anyhow::anyhow!(
            "unexpected response: {:04X?}",
            response_view.command_response()
        ));
    }
    Ok(())
}

fn start_partition_absolute(
    handle: &mut DeviceHandle<rusb::GlobalContext>,
    start_address: u64,
    partition_length: u64,
) -> anyhow::Result<()> {
    tracing::debug!(
        "start_partition_absolute: start_address={:#X}, partition_length={}",
        start_address,
        partition_length
    );
    let mut buf = [0u8; frame::MINIMUM_LENGTH + 16];
    let mut frame = frame::AxdlFrameViewMut::new(&mut buf);
    frame.init();
    frame.set_command_response(0x0001); // Start partition
    {
        let payload = frame.payload_mut();
        payload[0..8].copy_from_slice(&start_address.to_le_bytes());
        payload[8..16].copy_from_slice(&partition_length.to_le_bytes());
    }
    frame.finalize();

    handle
        .write_bulk(ENDPOINT_OUT, &buf, TIMEOUT)
        .map_err(|e| anyhow::anyhow!("send error: {}", e))?;

    let response = receive_response(handle, TIMEOUT)?;
    let response_view = frame::AxdlFrameView::new(&response);
    if response_view.command_response() != Some(0x0080) {
        return Err(anyhow::anyhow!(
            "unexpected response: {:04X?}",
            response_view.command_response()
        ));
    }
    Ok(())
}

fn start_partition_id(
    handle: &mut DeviceHandle<rusb::GlobalContext>,
    partition_name: &str,
    total_length: u64,
) -> anyhow::Result<()> {
    tracing::debug!(
        "start_partition_id: partition_name={}, total_length={}",
        partition_name,
        total_length
    );
    let mut buf = [0u8; frame::MINIMUM_LENGTH + 88];
    let mut frame = frame::AxdlFrameViewMut::new(&mut buf);
    frame.init();
    frame.set_command_response(0x0001); // Start partition
    {
        let payload = frame.payload_mut();
        let partition_name_bytes = partition_name
            .encode_utf16()
            .map(|c| c.to_le_bytes())
            .flatten()
            .collect::<Vec<_>>();
        payload[0..partition_name_bytes.len()].copy_from_slice(&partition_name_bytes);
        payload[72..80].copy_from_slice(&total_length.to_le_bytes());
    }
    frame.finalize();

    handle
        .write_bulk(ENDPOINT_OUT, &buf, TIMEOUT)
        .map_err(|e| anyhow::anyhow!("send error: {}", e))?;

    let response = receive_response(handle, TIMEOUT)?;
    let response_view = frame::AxdlFrameView::new(&response);
    if response_view.command_response() != Some(0x0080) {
        return Err(anyhow::anyhow!(
            "unexpected response: {:04X?}",
            response_view.command_response()
        ));
    }
    Ok(())
}

fn start_block(
    handle: &mut DeviceHandle<rusb::GlobalContext>,
    block_size: u16,
) -> anyhow::Result<()> {
    tracing::debug!("start_block: block_size={}", block_size);
    let mut buf = [0u8; frame::MINIMUM_LENGTH + 12];
    let mut frame = frame::AxdlFrameViewMut::new(&mut buf);
    frame.init();
    frame.set_command_response(0x0002); // Start block
    {
        let payload = frame.payload_mut();
        payload[0..2].copy_from_slice(&block_size.to_le_bytes());
    }
    frame.finalize();

    handle
        .write_bulk(ENDPOINT_OUT, &buf, TIMEOUT)
        .map_err(|e| anyhow::anyhow!("send error: {}", e))?;

    let response = receive_response(handle, TIMEOUT)?;
    let response_view = frame::AxdlFrameView::new(&response);
    if response_view.command_response() != Some(0x0080) {
        return Err(anyhow::anyhow!(
            "unexpected response: {:04X?}",
            response_view.command_response()
        ));
    }
    Ok(())
}

fn end_partition(
    handle: &mut DeviceHandle<rusb::GlobalContext>,
    timeout: Duration,
) -> anyhow::Result<()> {
    tracing::debug!("end_partition");
    let mut buf = [0u8; frame::MINIMUM_LENGTH];
    let mut frame = frame::AxdlFrameViewMut::new(&mut buf);
    frame.init();
    frame.set_command_response(0x0003); // End partition
    frame.finalize();

    handle
        .write_bulk(ENDPOINT_OUT, &buf, timeout)
        .map_err(|e| anyhow::anyhow!("send error: {}", e))?;

    let response = receive_response(handle, timeout)?;
    let response_view = frame::AxdlFrameView::new(&response);
    if response_view.command_response() != Some(0x0080) {
        return Err(anyhow::anyhow!(
            "unexpected response: {:04X?}",
            response_view.command_response()
        ));
    }
    Ok(())
}

fn end_ram_download(handle: &mut DeviceHandle<rusb::GlobalContext>) -> anyhow::Result<()> {
    tracing::debug!("end_ram_download");
    let mut buf = [0u8; frame::MINIMUM_LENGTH];
    let mut frame = frame::AxdlFrameViewMut::new(&mut buf);
    frame.init();
    frame.set_command_response(0x0004); // End RAM download
    frame.finalize();

    handle
        .write_bulk(ENDPOINT_OUT, &buf, TIMEOUT)
        .map_err(|e| anyhow::anyhow!("send error: {}", e))?;

    let response = receive_response(handle, TIMEOUT)?;
    let response_view = frame::AxdlFrameView::new(&response);
    if response_view.command_response() != Some(0x0080) {
        return Err(anyhow::anyhow!(
            "unexpected response: {:04X?}",
            response_view.command_response()
        ));
    }
    Ok(())
}

fn set_partition_table(
    handle: &mut DeviceHandle<rusb::GlobalContext>,
    partition_table: &partition::PartitionTable,
) -> anyhow::Result<()> {
    tracing::debug!("set_partition_table: {:?}", partition_table);
    let partition_table_image = partition_table.to_bytes();
    let mut buf = Vec::with_capacity(frame::MINIMUM_LENGTH + partition_table_image.len());
    buf.resize(frame::MINIMUM_LENGTH + partition_table_image.len(), 0);

    let mut frame = frame::AxdlFrameViewMut::new(&mut buf);
    frame.init();
    frame.set_command_response(0x000b); // Set partition table
    {
        let payload = frame.payload_mut();
        payload.copy_from_slice(&partition_table_image);
    }
    frame.finalize();

    handle
        .write_bulk(ENDPOINT_OUT, &buf, TIMEOUT)
        .map_err(|e| anyhow::anyhow!("send error: {}", e))?;

    let response = receive_response(handle, TIMEOUT)?;
    let response_view = frame::AxdlFrameView::new(&response);
    if response_view.command_response() != Some(0x0080) {
        return Err(anyhow::anyhow!(
            "unexpected response: {:04X?}",
            response_view.command_response()
        ));
    }
    Ok(())
}

fn write_image<R: std::io::Read>(
    handle: &mut DeviceHandle<rusb::GlobalContext>,
    reader: &mut R,
    chunk_size: usize,
    image_size: usize,
    report_every: Option<usize>,
) -> anyhow::Result<()> {
    let mut buffer = Vec::with_capacity(chunk_size);
    buffer.resize(chunk_size, 0);

    let mut report_every_counter = 0;
    let mut bytes_transferred = 0;
    loop {
        let bytes_read = reader
            .read(&mut buffer)
            .map_err(|e| anyhow::anyhow!("read error: {}", e))?;
        if bytes_read == 0 {
            break;
        }
        let chunk = &buffer[..bytes_read];
        start_block(handle, chunk.len() as u16)?;
        handle
            .write_bulk(ENDPOINT_OUT, chunk, Duration::from_secs(60))
            .map_err(|e| anyhow::anyhow!("send error: {}", e))?;
        let response = receive_response(handle, Duration::from_secs(60))?;
        let response_view = frame::AxdlFrameView::new(&response);
        if response_view.command_response() != Some(0x0080) {
            return Err(anyhow::anyhow!(
                "unexpected response: {:04X?}",
                response_view.command_response()
            ));
        }
        bytes_transferred += chunk.len();
        if let Some(report_every) = report_every {
            report_every_counter += 1;
            if report_every_counter >= report_every {
                report_every_counter = 0;
                tracing::info!("{}/{} bytes sent", bytes_transferred, image_size);
            }
        }
    }
    Ok(())
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
    let file = std::fs::File::open(&args.file)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| anyhow::anyhow!("failed to open AXP image: {}", e))?;
    let mut config_string = None;

    // Load the axp image configuration.
    let project = {
        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            if file.name().ends_with(".xml") {
                config_string = Some(String::new());
                std::io::Read::read_to_string(&mut file, config_string.as_mut().unwrap())?;
                break;
            }
        }
        let config_string =
            config_string.ok_or(anyhow::anyhow!("configuration file not found in the image"))?;
        let config: partition::deserialize::Config = serde_xml_rs::from_str(&config_string)
            .map_err(|e| anyhow::anyhow!("failed to parse the configuration file: {}", e))?;
        partition::Project::from(config.project)
    };

    tracing::debug!("{:#?}", project);
    let partition_table = project.partition_table();
    tracing::debug!("{:#?}", partition_table);

    if args.wait_device {
        if let Some(timeout) = args.wait_device_timeout_secs {
            tracing::info!(
                "Waiting for the device to be ready (timeout={}s)...",
                timeout
            );
        } else {
            tracing::info!("Waiting for the device to be ready...");
        }
    }
    let wait_start = std::time::Instant::now();
    let mut handle = loop {
        match rusb::open_device_with_vid_pid(VENDOR_ID, PRODUCT_ID) {
            Some(handle) => {
                break handle;
            }
            None => {}
        }
        if args.wait_device {
            if let Some(timeout) = args.wait_device_timeout_secs {
                if wait_start.elapsed() > Duration::from_secs(timeout) {
                    return Err(anyhow::anyhow!("timeout waiting for the device"));
                }
            }
            std::thread::sleep(Duration::from_secs(1));
        } else {
            return Err(anyhow::anyhow!("device not found"));
        }
    };

    tracing::info!("Stating the download process...");

    if let Err(e) = claim_interface(&mut handle, 0) {
        tracing::error!("failed to claim interface: {}", e);
        return Err(anyhow::anyhow!("failed to claim interface"));
    }

    // Check if romcode is runnning on the device.
    wait_handshake(&mut handle, "romcode")?;

    // Find the FDL1 image and download it.
    let fdl1_image = project
        .images()
        .iter()
        .find(|image| image.name() == "FDL1")
        .ok_or(anyhow::anyhow!("FDL1 image not found"))?;
    let fdl1_image_file = fdl1_image.file().ok_or(anyhow::anyhow!(
        "FDL1 image file not specified in the project"
    ))?;
    let mut fdl1 = archive
        .by_name(fdl1_image_file)
        .map_err(|e| anyhow::anyhow!("FDL1 image was not found in the image file: {}", e))?;
    let fdl1_address = match fdl1_image.block() {
        partition::Block::Absolute(address) => address,
        _ => return Err(anyhow::anyhow!("FDL1 block is not absolute")),
    };

    // Start the RAM download (FDL1)
    start_ram_download(&mut handle)?;
    let fdl1_image_size = fdl1.size();
    start_partition_absolute_32(&mut handle, *fdl1_address as u32, fdl1_image_size as u32)?;
    write_image(
        &mut handle,
        &mut fdl1,
        1000,
        fdl1_image_size as usize,
        Some(100),
    )?;
    drop(fdl1);
    end_partition(&mut handle, TIMEOUT)?;
    end_ram_download(&mut handle)?;

    wait_handshake(&mut handle, "fdl1")?;

    // Find the FDL2 image and download it.
    let fdl2_image = project
        .images()
        .iter()
        .find(|image| image.name() == "FDL2")
        .ok_or(anyhow::anyhow!("FDL2 image not found"))?;
    let fdl2_image_file = fdl2_image.file().ok_or(anyhow::anyhow!(
        "FDL2 image file not specified in the project"
    ))?;
    let mut fdl2 = archive
        .by_name(fdl2_image_file)
        .map_err(|e| anyhow::anyhow!("FDL2 image was not found in the image file: {}", e))?;
    let fdl2_address = match fdl2_image.block() {
        partition::Block::Absolute(address) => address,
        _ => return Err(anyhow::anyhow!("FDL2 block is not absolute")),
    };
    // Start the RAM download (FDL2)
    start_ram_download(&mut handle)?;

    let fdl2_image_size = fdl2.size();
    start_partition_absolute(&mut handle, *fdl2_address, fdl2_image_size)?;
    write_image(
        &mut handle,
        &mut fdl2,
        1000,
        fdl2_image_size as usize,
        Some(100),
    )?;
    drop(fdl2);
    end_partition(&mut handle, TIMEOUT)?;
    end_ram_download(&mut handle)?;

    // Download the partition table.
    set_partition_table(&mut handle, &partition_table)?;

    // Download all of "CODE" images
    for image in project.images().iter().filter(|image| {
        image.r#type() == partition::ImageType::Code
            && (!args.exclude_rootfs || image.name() != "ROOTFS")
    }) {
        tracing::info!("Downloading image: {}", image.name());

        let image_file_name = image.file().ok_or(anyhow::anyhow!(
            "image {} file not specified in the project",
            image.name()
        ))?;
        let mut image_data = archive.by_name(&image_file_name).map_err(|e| {
            anyhow::anyhow!("image {} was not found in the archive: {}", image.name(), e)
        })?;
        let image_id = match image.block() {
            partition::Block::Partition(id) => id,
            _ => {
                return Err(anyhow::anyhow!(
                    "image {} block is not partition",
                    image.name()
                ))
            }
        };
        let image_data_size = image_data.size();
        start_partition_id(&mut handle, &image_id, image_data_size)?;
        write_image(
            &mut handle,
            &mut image_data,
            48000,
            image_data_size as usize,
            Some(100),
        )?;
        end_partition(&mut handle, Duration::from_secs(60))?;
    }
    tracing::info!("Done");
    Ok(())
}

fn claim_interface<T: UsbContext>(handle: &mut DeviceHandle<T>, interface: u8) -> rusb::Result<()> {
    handle.claim_interface(interface)?;
    Ok(())
}
