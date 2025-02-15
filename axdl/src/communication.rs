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

use crate::AxdlError;

const HANDSHAKE_REQUEST: [u8; 3] = [0x3c, 0x3c, 0x3c];
pub const TIMEOUT: Duration = Duration::from_secs(5);

pub fn wait_handshake(
    device: &mut crate::transport::DynDevice,
    expected_handshake: &str,
) -> Result<(), AxdlError> {
    device.write_timeout(&HANDSHAKE_REQUEST, TIMEOUT)?;
    let mut buf = [0u8; 64];
    let length = device.read_timeout(&mut buf, TIMEOUT)?;

    tracing::debug!("received: {:02X?}", &buf[..length]);
    let view = crate::frame::AxdlFrameView::new(&buf[..length]);
    tracing::debug!(
        "view: {}, checksum={:04X}",
        view,
        view.calculate_checksum().unwrap_or(0)
    );
    if !view.is_valid() {
        return Err(AxdlError::InvalidFrame);
    }
    let handshake = view
        .payload()
        .map(|payload| {
            std::str::from_utf8(payload)
                .map_err(AxdlError::HandshakeDecodeError)
                .map(|s| s.to_string())
        })
        .transpose()?
        .ok_or(AxdlError::NoPayload)?;

    tracing::debug!("handshake: {}", handshake);
    if !handshake.contains(expected_handshake) {
        return Err(AxdlError::UnexpectedHandshake(handshake));
    }
    Ok(())
}

pub fn receive_response(
    device: &mut crate::transport::DynDevice,
    timeout: Duration,
) -> Result<Vec<u8>, AxdlError> {
    let mut buf = Vec::with_capacity(65536);
    buf.resize(buf.capacity(), 0);
    let length = device.read_timeout(&mut buf, timeout)?;

    tracing::debug!("received: {:02X?}", &buf[..length]);
    let view = crate::frame::AxdlFrameView::new(&buf[..length]);
    tracing::debug!(
        "view: {}, checksum={:04X}",
        view,
        view.calculate_checksum().unwrap_or(0)
    );
    if !view.is_valid() {
        return Err(AxdlError::InvalidFrame);
    }

    buf.resize(length, 0);
    Ok(buf)
}

pub fn start_ram_download(device: &mut crate::transport::DynDevice) -> Result<(), AxdlError> {
    tracing::debug!("start_ram_download");
    let mut buf = [0u8; crate::frame::MINIMUM_LENGTH];
    let mut frame = crate::frame::AxdlFrameViewMut::new(&mut buf);
    frame.init();
    frame.finalize();

    device.write_timeout(&buf, TIMEOUT)?;

    let response = receive_response(device, TIMEOUT)?;
    let response_view = crate::frame::AxdlFrameView::new(&response);
    if response_view.command_response() != Some(0x0080) {
        return Err(AxdlError::UnexpectedResponse(
            response_view.command_response().unwrap(),
        ));
    }
    Ok(())
}

pub fn start_partition_absolute_32(
    device: &mut crate::transport::DynDevice,
    start_address: u32,
    partition_length: u32,
) -> Result<(), AxdlError> {
    tracing::debug!(
        "start_partition_absolute: start_address={:#X}, partition_length={}",
        start_address,
        partition_length
    );
    let mut buf = [0u8; crate::frame::MINIMUM_LENGTH + 8];
    let mut frame = crate::frame::AxdlFrameViewMut::new(&mut buf);
    frame.init();
    frame.set_command_response(0x0001); // Start partition
    {
        let payload = frame.payload_mut();
        payload[0..4].copy_from_slice(&start_address.to_le_bytes());
        payload[4..8].copy_from_slice(&partition_length.to_le_bytes());
    }
    frame.finalize();

    device.write_timeout(&buf, TIMEOUT)?;

    let response = receive_response(device, TIMEOUT)?;
    let response_view = crate::frame::AxdlFrameView::new(&response);
    if response_view.command_response() != Some(0x0080) {
        return Err(AxdlError::UnexpectedResponse(
            response_view.command_response().unwrap(),
        ));
    }
    Ok(())
}

pub fn start_partition_absolute(
    device: &mut crate::transport::DynDevice,
    start_address: u64,
    partition_length: u64,
) -> Result<(), AxdlError> {
    tracing::debug!(
        "start_partition_absolute: start_address={:#X}, partition_length={}",
        start_address,
        partition_length
    );
    let mut buf = [0u8; crate::frame::MINIMUM_LENGTH + 16];
    let mut frame = crate::frame::AxdlFrameViewMut::new(&mut buf);
    frame.init();
    frame.set_command_response(0x0001); // Start partition
    {
        let payload = frame.payload_mut();
        payload[0..8].copy_from_slice(&start_address.to_le_bytes());
        payload[8..16].copy_from_slice(&partition_length.to_le_bytes());
    }
    frame.finalize();

    device.write_timeout(&buf, TIMEOUT)?;

    let response = receive_response(device, TIMEOUT)?;
    let response_view = crate::frame::AxdlFrameView::new(&response);
    if response_view.command_response() != Some(0x0080) {
        return Err(AxdlError::UnexpectedResponse(
            response_view.command_response().unwrap(),
        ));
    }
    Ok(())
}

pub fn start_partition_id(
    device: &mut crate::transport::DynDevice,
    partition_name: &str,
    total_length: u64,
) -> Result<(), AxdlError> {
    tracing::debug!(
        "start_partition_id: partition_name={}, total_length={}",
        partition_name,
        total_length
    );
    let mut buf = [0u8; crate::frame::MINIMUM_LENGTH + 88];
    let mut frame = crate::frame::AxdlFrameViewMut::new(&mut buf);
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

    device.write_timeout(&buf, TIMEOUT)?;

    let response = receive_response(device, TIMEOUT)?;
    let response_view = crate::frame::AxdlFrameView::new(&response);
    if response_view.command_response() != Some(0x0080) {
        return Err(AxdlError::UnexpectedResponse(
            response_view.command_response().unwrap(),
        ));
    }
    Ok(())
}

pub fn start_block(
    device: &mut crate::transport::DynDevice,
    block_size: u16,
) -> Result<(), AxdlError> {
    tracing::debug!("start_block: block_size={}", block_size);
    let mut buf = [0u8; crate::frame::MINIMUM_LENGTH + 12];
    let mut frame = crate::frame::AxdlFrameViewMut::new(&mut buf);
    frame.init();
    frame.set_command_response(0x0002); // Start block
    {
        let payload = frame.payload_mut();
        payload[0..2].copy_from_slice(&block_size.to_le_bytes());
    }
    frame.finalize();

    device.write_timeout(&buf, TIMEOUT)?;

    let response = receive_response(device, TIMEOUT)?;
    let response_view = crate::frame::AxdlFrameView::new(&response);
    if response_view.command_response() != Some(0x0080) {
        return Err(AxdlError::UnexpectedResponse(
            response_view.command_response().unwrap(),
        ));
    }
    Ok(())
}

pub fn end_partition(
    device: &mut crate::transport::DynDevice,
    timeout: Duration,
) -> Result<(), AxdlError> {
    tracing::debug!("end_partition");
    let mut buf = [0u8; crate::frame::MINIMUM_LENGTH];
    let mut frame = crate::frame::AxdlFrameViewMut::new(&mut buf);
    frame.init();
    frame.set_command_response(0x0003); // End partition
    frame.finalize();

    device.write_timeout(&buf, timeout)?;

    let response = receive_response(device, timeout)?;
    let response_view = crate::frame::AxdlFrameView::new(&response);
    if response_view.command_response() != Some(0x0080) {
        return Err(AxdlError::UnexpectedResponse(
            response_view.command_response().unwrap(),
        ));
    }
    Ok(())
}

pub fn end_ram_download(device: &mut crate::transport::DynDevice) -> Result<(), AxdlError> {
    tracing::debug!("end_ram_download");
    let mut buf = [0u8; crate::frame::MINIMUM_LENGTH];
    let mut frame = crate::frame::AxdlFrameViewMut::new(&mut buf);
    frame.init();
    frame.set_command_response(0x0004); // End RAM download
    frame.finalize();

    device.write_timeout(&buf, TIMEOUT)?;

    let response = receive_response(device, TIMEOUT)?;
    let response_view = crate::frame::AxdlFrameView::new(&response);
    if response_view.command_response() != Some(0x0080) {
        return Err(AxdlError::UnexpectedResponse(
            response_view.command_response().unwrap(),
        ));
    }
    Ok(())
}

pub fn set_partition_table(
    device: &mut crate::transport::DynDevice,
    partition_table: &crate::partition::PartitionTable,
) -> Result<(), AxdlError> {
    tracing::debug!("set_partition_table: {:?}", partition_table);
    let partition_table_image = partition_table.to_bytes();
    let mut buf = Vec::with_capacity(crate::frame::MINIMUM_LENGTH + partition_table_image.len());
    buf.resize(
        crate::frame::MINIMUM_LENGTH + partition_table_image.len(),
        0,
    );

    let mut frame = crate::frame::AxdlFrameViewMut::new(&mut buf);
    frame.init();
    frame.set_command_response(0x000b); // Set partition table
    {
        let payload = frame.payload_mut();
        payload.copy_from_slice(&partition_table_image);
    }
    frame.finalize();

    device.write_timeout(&buf, TIMEOUT)?;

    let response = receive_response(device, TIMEOUT)?;
    let response_view = crate::frame::AxdlFrameView::new(&response);
    if response_view.command_response() != Some(0x0080) {
        return Err(AxdlError::UnexpectedResponse(
            response_view.command_response().unwrap(),
        ));
    }
    Ok(())
}

pub fn write_image<R: std::io::Read>(
    device: &mut crate::transport::DynDevice,
    reader: &mut R,
    chunk_size: usize,
    image_name: &str,
    image_size: usize,
    report_every: Option<usize>,
    progress: &mut impl crate::DownloadProgress,
) -> Result<(), AxdlError> {
    let mut buffer = Vec::with_capacity(chunk_size);
    buffer.resize(chunk_size, 0);

    let mut report_every_counter = 0;
    let mut bytes_transferred: usize = 0;
    loop {
        progress.check_is_cancelled()?;

        let bytes_read = reader
            .read(&mut buffer)
            .map_err(|e| AxdlError::IoError("read error".to_string(), e))?;
        if bytes_read == 0 {
            break;
        }
        let chunk = &buffer[..bytes_read];
        start_block(device, chunk.len() as u16)?;
        device.write_timeout(chunk, Duration::from_secs(60))?;
        let response = receive_response(device, Duration::from_secs(60))?;
        let response_view = crate::frame::AxdlFrameView::new(&response);
        if response_view.command_response() != Some(0x0080) {
            return Err(AxdlError::UnexpectedResponse(
                response_view.command_response().unwrap(),
            ));
        }
        bytes_transferred += chunk.len();
        if let Some(report_every) = report_every {
            report_every_counter += 1;
            if report_every_counter >= report_every {
                report_every_counter = 0;
                tracing::debug!("{}/{} bytes sent", bytes_transferred, image_size);
                progress.report_progress(
                    &format!("Downloading image {}", image_name),
                    Some(bytes_transferred as f32 / image_size as f32),
                );
            }
        }
    }
    Ok(())
}

pub mod r#async {
    use crate::{communication::HANDSHAKE_REQUEST, transport::AsyncDevice, AxdlError};

    pub async fn wait_handshake<D: AsyncDevice>(
        device: &mut D,
        expected_handshake: &str,
    ) -> Result<(), AxdlError> {
        device.write(&HANDSHAKE_REQUEST).await?;
        let mut buf = [0u8; 64];
        let length = device.read(&mut buf).await?;

        tracing::debug!("received: {:02X?}", &buf[..length]);
        let view = crate::frame::AxdlFrameView::new(&buf[..length]);
        tracing::debug!(
            "view: {}, checksum={:04X}",
            view,
            view.calculate_checksum().unwrap_or(0)
        );
        if !view.is_valid() {
            return Err(AxdlError::InvalidFrame);
        }
        let handshake = view
            .payload()
            .map(|payload| {
                std::str::from_utf8(payload)
                    .map_err(AxdlError::HandshakeDecodeError)
                    .map(|s| s.to_string())
            })
            .transpose()?
            .ok_or(AxdlError::NoPayload)?;

        tracing::debug!("handshake: {}", handshake);
        if !handshake.contains(expected_handshake) {
            return Err(AxdlError::UnexpectedHandshake(handshake));
        }
        Ok(())
    }

    pub async fn receive_response<D: crate::transport::AsyncDevice>(
        device: &mut D,
    ) -> Result<Vec<u8>, AxdlError> {
        let mut buf = Vec::with_capacity(65536);
        buf.resize(buf.capacity(), 0);
        let length = device.read(&mut buf).await?;

        tracing::debug!("received: {:02X?}", &buf[..length]);
        let view = crate::frame::AxdlFrameView::new(&buf[..length]);
        tracing::debug!(
            "view: {}, checksum={:04X}",
            view,
            view.calculate_checksum().unwrap_or(0)
        );
        if !view.is_valid() {
            return Err(AxdlError::InvalidFrame);
        }

        buf.resize(length, 0);
        Ok(buf)
    }

    pub async fn start_ram_download<D: AsyncDevice>(
        device: &mut D,
    ) -> Result<(), AxdlError> {
        tracing::debug!("start_ram_download");
        let mut buf = [0u8; crate::frame::MINIMUM_LENGTH];
        let mut frame = crate::frame::AxdlFrameViewMut::new(&mut buf);
        frame.init();
        frame.finalize();

        device.write(&buf).await?;

        let response = receive_response(device).await?;
        let response_view = crate::frame::AxdlFrameView::new(&response);
        if response_view.command_response() != Some(0x0080) {
            return Err(AxdlError::UnexpectedResponse(
                response_view.command_response().unwrap(),
            ));
        }
        Ok(())
    }

    pub async fn start_partition_absolute_32<D: AsyncDevice>(
        device: &mut D,
        start_address: u32,
        partition_length: u32,
    ) -> Result<(), AxdlError> {
        tracing::debug!(
            "start_partition_absolute: start_address={:#X}, partition_length={}",
            start_address,
            partition_length
        );
        let mut buf = [0u8; crate::frame::MINIMUM_LENGTH + 8];
        let mut frame = crate::frame::AxdlFrameViewMut::new(&mut buf);
        frame.init();
        frame.set_command_response(0x0001); // Start partition
        {
            let payload = frame.payload_mut();
            payload[0..4].copy_from_slice(&start_address.to_le_bytes());
            payload[4..8].copy_from_slice(&partition_length.to_le_bytes());
        }
        frame.finalize();

        device.write(&buf).await?;

        let response = receive_response(device).await?;
        let response_view = crate::frame::AxdlFrameView::new(&response);
        if response_view.command_response() != Some(0x0080) {
            return Err(AxdlError::UnexpectedResponse(
                response_view.command_response().unwrap(),
            ));
        }
        Ok(())
    }

    pub async fn start_partition_absolute<D: AsyncDevice>(
        device: &mut D,
        start_address: u64,
        partition_length: u64,
    ) -> Result<(), AxdlError> {
        tracing::debug!(
            "start_partition_absolute: start_address={:#X}, partition_length={}",
            start_address,
            partition_length
        );
        let mut buf = [0u8; crate::frame::MINIMUM_LENGTH + 16];
        let mut frame = crate::frame::AxdlFrameViewMut::new(&mut buf);
        frame.init();
        frame.set_command_response(0x0001); // Start partition
        {
            let payload = frame.payload_mut();
            payload[0..8].copy_from_slice(&start_address.to_le_bytes());
            payload[8..16].copy_from_slice(&partition_length.to_le_bytes());
        }
        frame.finalize();

        device.write(&buf).await?;

        let response = receive_response(device).await?;
        let response_view = crate::frame::AxdlFrameView::new(&response);
        if response_view.command_response() != Some(0x0080) {
            return Err(AxdlError::UnexpectedResponse(
                response_view.command_response().unwrap(),
            ));
        }
        Ok(())
    }

    pub async fn start_partition_id<D: crate::transport::AsyncDevice>(
        device: &mut D,
        partition_name: &str,
        total_length: u64,
    ) -> Result<(), AxdlError> {
        tracing::debug!(
            "start_partition_id: partition_name={}, total_length={}",
            partition_name,
            total_length
        );
        let mut buf = [0u8; crate::frame::MINIMUM_LENGTH + 88];
        let mut frame = crate::frame::AxdlFrameViewMut::new(&mut buf);
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

        device.write(&buf).await?;

        let response = receive_response(device).await?;
        let response_view = crate::frame::AxdlFrameView::new(&response);
        if response_view.command_response() != Some(0x0080) {
            return Err(AxdlError::UnexpectedResponse(
                response_view.command_response().unwrap(),
            ));
        }
        Ok(())
    }

    pub async fn start_block<D: crate::transport::AsyncDevice>(
        device: &mut D,
        block_size: u16,
    ) -> Result<(), AxdlError> {
        tracing::debug!("start_block: block_size={}", block_size);
        let mut buf = [0u8; crate::frame::MINIMUM_LENGTH + 12];
        let mut frame = crate::frame::AxdlFrameViewMut::new(&mut buf);
        frame.init();
        frame.set_command_response(0x0002); // Start block
        {
            let payload = frame.payload_mut();
            payload[0..2].copy_from_slice(&block_size.to_le_bytes());
        }
        frame.finalize();

        device.write(&buf).await?;

        let response = receive_response(device).await?;
        let response_view = crate::frame::AxdlFrameView::new(&response);
        if response_view.command_response() != Some(0x0080) {
            return Err(AxdlError::UnexpectedResponse(
                response_view.command_response().unwrap(),
            ));
        }
        Ok(())
    }

    pub async fn end_partition<D: crate::transport::AsyncDevice>(
        device: &mut D,
    ) -> Result<(), AxdlError> {
        tracing::debug!("end_partition");
        let mut buf = [0u8; crate::frame::MINIMUM_LENGTH];
        let mut frame = crate::frame::AxdlFrameViewMut::new(&mut buf);
        frame.init();
        frame.set_command_response(0x0003); // End partition
        frame.finalize();

        device.write(&buf).await?;

        let response = receive_response(device).await?;
        let response_view = crate::frame::AxdlFrameView::new(&response);
        if response_view.command_response() != Some(0x0080) {
            return Err(AxdlError::UnexpectedResponse(
                response_view.command_response().unwrap(),
            ));
        }
        Ok(())
    }

    pub async fn end_ram_download<D: crate::transport::AsyncDevice>(
        device: &mut D,
    ) -> Result<(), AxdlError> {
        tracing::debug!("end_ram_download");
        let mut buf = [0u8; crate::frame::MINIMUM_LENGTH];
        let mut frame = crate::frame::AxdlFrameViewMut::new(&mut buf);
        frame.init();
        frame.set_command_response(0x0004); // End RAM download
        frame.finalize();

        device.write(&buf).await?;

        let response = receive_response(device).await?;
        let response_view = crate::frame::AxdlFrameView::new(&response);
        if response_view.command_response() != Some(0x0080) {
            return Err(AxdlError::UnexpectedResponse(
                response_view.command_response().unwrap(),
            ));
        }
        Ok(())
    }

    pub async fn set_partition_table<D: crate::transport::AsyncDevice>(
        device: &mut D,
        partition_table: &crate::partition::PartitionTable,
    ) -> Result<(), AxdlError> {
        tracing::debug!("set_partition_table: {:?}", partition_table);
        let partition_table_image = partition_table.to_bytes();
        let mut buf =
            Vec::with_capacity(crate::frame::MINIMUM_LENGTH + partition_table_image.len());
        buf.resize(
            crate::frame::MINIMUM_LENGTH + partition_table_image.len(),
            0,
        );

        let mut frame = crate::frame::AxdlFrameViewMut::new(&mut buf);
        frame.init();
        frame.set_command_response(0x000b); // Set partition table
        {
            let payload = frame.payload_mut();
            payload.copy_from_slice(&partition_table_image);
        }
        frame.finalize();

        device.write(&buf).await?;

        let response = receive_response(device).await?;
        let response_view = crate::frame::AxdlFrameView::new(&response);
        if response_view.command_response() != Some(0x0080) {
            return Err(AxdlError::UnexpectedResponse(
                response_view.command_response().unwrap(),
            ));
        }
        Ok(())
    }

    pub async fn write_image<D: AsyncDevice, R: std::io::Read>(
        device: &mut D,
        reader: &mut R,
        chunk_size: usize,
        image_name: &str,
        image_size: usize,
        report_every: Option<usize>,
        progress: &mut impl crate::DownloadProgress,
    ) -> Result<(), AxdlError> {
        let mut buffer = Vec::with_capacity(chunk_size);
        buffer.resize(chunk_size, 0);

        let mut report_every_counter = 0;
        let mut bytes_transferred: usize = 0;
        loop {
            progress.check_is_cancelled()?;

            let bytes_read = reader
                .read(&mut buffer)
                .map_err(|e| AxdlError::IoError("read error".to_string(), e))?;
            if bytes_read == 0 {
                break;
            }
            let chunk = &buffer[..bytes_read];
            start_block(device, chunk.len() as u16).await?;
            device.write(chunk).await?;
            let response = receive_response(device).await?;
            let response_view = crate::frame::AxdlFrameView::new(&response);
            if response_view.command_response() != Some(0x0080) {
                return Err(AxdlError::UnexpectedResponse(
                    response_view.command_response().unwrap(),
                ));
            }
            bytes_transferred += chunk.len();
            if let Some(report_every) = report_every {
                report_every_counter += 1;
                if report_every_counter >= report_every {
                    report_every_counter = 0;
                    tracing::debug!("{}/{} bytes sent", bytes_transferred, image_size);
                    progress.report_progress(
                        &format!("Downloading image {}", image_name),
                        Some(bytes_transferred as f32 / image_size as f32),
                    );
                }
            }
        }
        Ok(())
    }
}
