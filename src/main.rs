use rusb::{DeviceHandle, UsbContext};
use std::time::Duration;

mod frame;
mod partition;

const VENDOR_ID: u16 = 0x32c9;
const PRODUCT_ID: u16 = 0x1000;
const ENDPOINT_OUT: u8 = 0x01;
const ENDPOINT_IN: u8 = 0x81;
const TIMEOUT: Duration = Duration::from_secs(1);

const HANDSHAKE_REQUEST: [u8; 3] = [0x3c, 0x3c, 0x3c];

fn wait_handshake(handle: &mut DeviceHandle<rusb::GlobalContext>, expected_handshake: &str) -> anyhow::Result<()> {
    handle.write_bulk(ENDPOINT_OUT, &HANDSHAKE_REQUEST, TIMEOUT)
        .map_err(|e| anyhow::anyhow!("send error: {}", e))?;
    let mut buf = [0u8; 64];
    let length = handle.read_bulk(ENDPOINT_IN, &mut buf, TIMEOUT)
        .map_err(|e| anyhow::anyhow!("receive error: {}", e))?;

    tracing::debug!("received: {:02X?}", &buf[..length]);
    let view = frame::AxdlFrameView::new(&buf[..length]);
    tracing::debug!("view: {}, checksum={:04X}", view, view.calculate_checksum().unwrap_or(0));
    if !view.is_valid() {
        return Err(anyhow::anyhow!("invalid frame"));
    }
    let handshake = view.payload()
        .map(|payload| std::str::from_utf8(payload).map_err(|e| anyhow::anyhow!("invalid utf-8: {}", e)))
        .transpose()?
        .ok_or(anyhow::anyhow!("no payload"))?;


    tracing::debug!("handshake: {}", handshake);
    if !handshake.contains(expected_handshake) {
        return Err(anyhow::anyhow!("unexpected handshake: {}", handshake));
    }
    Ok(())
}

fn receive_response(handle: &mut DeviceHandle<rusb::GlobalContext>) -> anyhow::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(65536);
    buf.resize(buf.capacity(), 0);
    let length = handle.read_bulk(ENDPOINT_IN, &mut buf, TIMEOUT)
        .map_err(|e| anyhow::anyhow!("receive error: {}", e))?;

    tracing::debug!("received: {:02X?}", &buf[..length]);
    let view = frame::AxdlFrameView::new(&buf[..length]);
    tracing::debug!("view: {}, checksum={:04X}", view, view.calculate_checksum().unwrap_or(0));
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

    handle.write_bulk(ENDPOINT_OUT, &buf, TIMEOUT)
        .map_err(|e| anyhow::anyhow!("send error: {}", e))?;

    let response = receive_response(handle)?;
    let response_view = frame::AxdlFrameView::new(&response);
    if response_view.command_response() != Some(0x0080) {
        return Err(anyhow::anyhow!("unexpected response: {:04X?}", response_view.command_response()));
    }
    Ok(())
}


fn start_partition_absolute_32(handle: &mut DeviceHandle<rusb::GlobalContext>, start_address: u32, partition_length: u32) -> anyhow::Result<()> {
    tracing::debug!("start_partition_absolute: start_address={:#X}, partition_length={}", start_address, partition_length);
    let mut buf = [0u8; frame::MINIMUM_LENGTH + 8];
    let mut frame = frame::AxdlFrameViewMut::new(&mut buf);
    frame.init();
    frame.set_command_response(0x0001);  // Start partition
    {
        let payload = frame.payload_mut();
        payload[0..4].copy_from_slice(&start_address.to_le_bytes());
        payload[4..8].copy_from_slice(&partition_length.to_le_bytes());
    }
    frame.finalize();

    handle.write_bulk(ENDPOINT_OUT, &buf, TIMEOUT)
        .map_err(|e| anyhow::anyhow!("send error: {}", e))?;

    let response = receive_response(handle)?;
    let response_view = frame::AxdlFrameView::new(&response);
    if response_view.command_response() != Some(0x0080) {
        return Err(anyhow::anyhow!("unexpected response: {:04X?}", response_view.command_response()));
    }
    Ok(())
}

fn start_partition_absolute(handle: &mut DeviceHandle<rusb::GlobalContext>, start_address: u64, partition_length: u64) -> anyhow::Result<()> {
    tracing::debug!("start_partition_absolute: start_address={:#X}, partition_length={}", start_address, partition_length);
    let mut buf = [0u8; frame::MINIMUM_LENGTH + 16];
    let mut frame = frame::AxdlFrameViewMut::new(&mut buf);
    frame.init();
    frame.set_command_response(0x0001);  // Start partition
    {
        let payload = frame.payload_mut();
        payload[0..8].copy_from_slice(&start_address.to_le_bytes());
        payload[8..16].copy_from_slice(&partition_length.to_le_bytes());
    }
    frame.finalize();

    handle.write_bulk(ENDPOINT_OUT, &buf, TIMEOUT)
        .map_err(|e| anyhow::anyhow!("send error: {}", e))?;

    let response = receive_response(handle)?;
    let response_view = frame::AxdlFrameView::new(&response);
    if response_view.command_response() != Some(0x0080) {
        return Err(anyhow::anyhow!("unexpected response: {:04X?}", response_view.command_response()));
    }
    Ok(())
}


fn start_partition_id(handle: &mut DeviceHandle<rusb::GlobalContext>, partition_name: &str, total_length: u64) -> anyhow::Result<()> {
    tracing::debug!("start_partition_id: partition_name={}, total_length={}", partition_name, total_length);
    let mut buf = [0u8; frame::MINIMUM_LENGTH + 88];
    let mut frame = frame::AxdlFrameViewMut::new(&mut buf);
    frame.init();
    frame.set_command_response(0x0001);  // Start partition
    {
        let payload = frame.payload_mut();
        let partition_name_bytes = partition_name.encode_utf16().map(|c| c.to_le_bytes()).flatten().collect::<Vec<_>>();
        payload[0..partition_name_bytes.len()].copy_from_slice(&partition_name_bytes);
        payload[72..80].copy_from_slice(&total_length.to_le_bytes());
    }
    frame.finalize();

    handle.write_bulk(ENDPOINT_OUT, &buf, TIMEOUT)
        .map_err(|e| anyhow::anyhow!("send error: {}", e))?;

    let response = receive_response(handle)?;
    let response_view = frame::AxdlFrameView::new(&response);
    if response_view.command_response() != Some(0x0080) {
        return Err(anyhow::anyhow!("unexpected response: {:04X?}", response_view.command_response()));
    }
    Ok(())
}

fn start_block(handle: &mut DeviceHandle<rusb::GlobalContext>, block_size: u16) -> anyhow::Result<()> {
    tracing::debug!("start_block: block_size={}", block_size);
    let mut buf = [0u8; frame::MINIMUM_LENGTH + 12];
    let mut frame = frame::AxdlFrameViewMut::new(&mut buf);
    frame.init();
    frame.set_command_response(0x0002);  // Start block
    {
        let payload = frame.payload_mut();
        payload[0..2].copy_from_slice(&block_size.to_le_bytes());
    }
    frame.finalize();

    handle.write_bulk(ENDPOINT_OUT, &buf, TIMEOUT)
        .map_err(|e| anyhow::anyhow!("send error: {}", e))?;

    let response = receive_response(handle)?;
    let response_view = frame::AxdlFrameView::new(&response);
    if response_view.command_response() != Some(0x0080) {
        return Err(anyhow::anyhow!("unexpected response: {:04X?}", response_view.command_response()));
    }
    Ok(())
}

fn end_partition(handle: &mut DeviceHandle<rusb::GlobalContext>) -> anyhow::Result<()> {
    tracing::debug!("end_partition");
    let mut buf = [0u8; frame::MINIMUM_LENGTH];
    let mut frame = frame::AxdlFrameViewMut::new(&mut buf);
    frame.init();
    frame.set_command_response(0x0003);  // End partition
    frame.finalize();

    handle.write_bulk(ENDPOINT_OUT, &buf, TIMEOUT)
        .map_err(|e| anyhow::anyhow!("send error: {}", e))?;

    let response = receive_response(handle)?;
    let response_view = frame::AxdlFrameView::new(&response);
    if response_view.command_response() != Some(0x0080) {
        return Err(anyhow::anyhow!("unexpected response: {:04X?}", response_view.command_response()));
    }
    Ok(())
}

fn end_ram_download(handle: &mut DeviceHandle<rusb::GlobalContext>) -> anyhow::Result<()> {
    tracing::debug!("end_ram_download");
    let mut buf = [0u8; frame::MINIMUM_LENGTH];
    let mut frame = frame::AxdlFrameViewMut::new(&mut buf);
    frame.init();
    frame.set_command_response(0x0004);  // End RAM download
    frame.finalize();

    handle.write_bulk(ENDPOINT_OUT, &buf, TIMEOUT)
        .map_err(|e| anyhow::anyhow!("send error: {}", e))?;

    let response = receive_response(handle)?;
    let response_view = frame::AxdlFrameView::new(&response);
    if response_view.command_response() != Some(0x0080) {
        return Err(anyhow::anyhow!("unexpected response: {:04X?}", response_view.command_response()));
    }
    Ok(())
}

fn set_partition_table(handle: &mut DeviceHandle<rusb::GlobalContext>, partition_table: &partition::PartitionTable) -> anyhow::Result<()> {
    tracing::debug!("set_partition_table: {:?}", partition_table);
    let partition_table_image = partition_table.to_bytes();
    let mut buf = Vec::with_capacity(frame::MINIMUM_LENGTH + partition_table_image.len());
    buf.resize(frame::MINIMUM_LENGTH + partition_table_image.len(), 0);

    let mut frame = frame::AxdlFrameViewMut::new(&mut buf);
    frame.init();
    frame.set_command_response(0x000b);  // Set partition table
    {
        let payload = frame.payload_mut();
        payload.copy_from_slice(&partition_table_image);
    }
    frame.finalize();

    handle.write_bulk(ENDPOINT_OUT, &buf, TIMEOUT)
        .map_err(|e| anyhow::anyhow!("send error: {}", e))?;

    let response = receive_response(handle)?;
    let response_view = frame::AxdlFrameView::new(&response);
    if response_view.command_response() != Some(0x0080) {
        return Err(anyhow::anyhow!("unexpected response: {:04X?}", response_view.command_response()));
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

    let base_path = std::path::PathBuf::from("M5_LLM_ubuntu22.04_not_fixed");

    // Load the axp image configuration.
    let config_string =
        std::fs::read_to_string(base_path.join("AX630C_emmc_arm64_k419.xml"))
            .expect("Failed to read the configuration file.");
    let config: partition::deserialize::Config =
        serde_xml_rs::from_str(&config_string).expect("Failed to parse the configuration file.");
    let project = partition::Project::from(config.project);
    tracing::info!("{:#?}", project);
    let partition_table = project.partition_table();
    tracing::info!("{:#?}", partition_table);

    match rusb::open_device_with_vid_pid(VENDOR_ID, PRODUCT_ID) {
        Some(mut handle) => {
            tracing::info!("Device opened");

            if let Err(e) = claim_interface(&mut handle, 0) {
                tracing::error!("failed to claim interface: {}", e);
                return Err(anyhow::anyhow!("failed to claim interface"));
            }
            
            // Check if romcode is runnning on the device.
            wait_handshake(&mut handle, "romcode")?;

            // Download the flash downloader FDL1 and FDL2.
            let fdl1_image = project.images().iter().find(|image| image.name() == "FDL1")
                .ok_or(anyhow::anyhow!("FDL1 image not found"))?;
            let fdl1 = std::fs::read(base_path.join(fdl1_image.file().unwrap_or_default()))
                .map_err(|e| anyhow::anyhow!("failed to read FDL1 image: {}", e))?;
            let fdl1_address = match fdl1_image.block() {
                partition::Block::Absolute(address) => address,
                _ => return Err(anyhow::anyhow!("FDL1 block is not absolute")),
            };
            let fdl2_image = project.images().iter().find(|image| image.name() == "FDL2")
                .ok_or(anyhow::anyhow!("FDL2 image not found"))?;
            let fdl2 = std::fs::read(base_path.join(fdl2_image.file().unwrap_or_default()))
                .map_err(|e| anyhow::anyhow!("failed to read FDL2 image: {}", e))?;
            let fdl2_address = match fdl2_image.block() {
                partition::Block::Absolute(address) => address,
                _ => return Err(anyhow::anyhow!("FDL2 block is not absolute")),
            };
            // Start the RAM download (FDL1)
            start_ram_download(&mut handle)?;
            start_partition_absolute_32(&mut handle, *fdl1_address as u32, fdl1.len() as u32)?;
            for chunk in fdl1.chunks(1000).into_iter() {
                start_block(&mut handle, chunk.len() as u16)?;
                handle.write_bulk(ENDPOINT_OUT, chunk, TIMEOUT)
                    .map_err(|e| anyhow::anyhow!("send error: {}", e))?;
                let response = receive_response(&mut handle)?;
                let response_view = frame::AxdlFrameView::new(&response);
                if response_view.command_response() != Some(0x0080) {
                    return Err(anyhow::anyhow!("unexpected response: {:04X?}", response_view.command_response()));
                }
            }
            end_partition(&mut handle)?;
            end_ram_download(&mut handle)?;

            wait_handshake(&mut handle, "fdl1")?;

            // Start the RAM download (FDL2)
            start_ram_download(&mut handle)?;
            start_partition_absolute(&mut handle, *fdl2_address, fdl2.len() as u64)?;
            for chunk in fdl2.chunks(48000).into_iter() {
                start_block(&mut handle, chunk.len() as u16)?;
                handle.write_bulk(ENDPOINT_OUT, chunk, TIMEOUT)
                    .map_err(|e| anyhow::anyhow!("send error: {}", e))?;
                let response = receive_response(&mut handle)?;
                let response_view = frame::AxdlFrameView::new(&response);
                if response_view.command_response() != Some(0x0080) {
                    return Err(anyhow::anyhow!("unexpected response: {:04X?}", response_view.command_response()));
                }
            }
            end_partition(&mut handle)?;
            end_ram_download(&mut handle)?;

            // Download the partition table.
            set_partition_table(&mut handle, &partition_table)?;

            // Download all of "CODE" images except rootfs.
            for image in project.images().iter().filter(|image| image.r#type() == partition::ImageType::Code && image.name() != "ROOTFS") {
                let image_data = std::fs::read(base_path.join(image.file().unwrap_or_default()))
                    .map_err(|e| anyhow::anyhow!("failed to read image: {}", e))?;
                let image_id  = match image.block() {
                    partition::Block::Partition(id) => id,
                    _ => return Err(anyhow::anyhow!("image {} block is not partition", image.name())),
                };
                start_partition_id(&mut handle, &image_id, image_data.len() as u64)?;
                for chunk in image_data.chunks(48000).into_iter() {
                    start_block(&mut handle, chunk.len() as u16)?;
                    handle.write_bulk(ENDPOINT_OUT, chunk, TIMEOUT)
                        .map_err(|e| anyhow::anyhow!("send error: {}", e))?;
                    let response = receive_response(&mut handle)?;
                    let response_view = frame::AxdlFrameView::new(&response);
                    if response_view.command_response() != Some(0x0080) {
                        return Err(anyhow::anyhow!("unexpected response: {:04X?}", response_view.command_response()));
                    }
                }
                end_partition(&mut handle)?;
            }
        }
        None => tracing::error!("device not found"),
    }

    Ok(())
}

fn claim_interface<T: UsbContext>(handle: &mut DeviceHandle<T>, interface: u8) -> rusb::Result<()> {
    handle.claim_interface(interface)?;
    Ok(())
}
