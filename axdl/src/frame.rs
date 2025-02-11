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

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::io::{Cursor, Read, Write};
use thiserror::Error;

/// USBフレーム構造体
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UsbFrame {
    pub raw: ByteBuf,
}

#[derive(Debug, Error)]
pub enum UsbFrameError {
    #[error("Invalid signature")]
    Signature,
    #[error("Invalid frame length")]
    Length,
    #[error("Invalid checksum")]
    Checksum,
}

pub const MINIMUM_LENGTH: usize = 4 + 2 + 2 + 2; // signature + length + command_response + checksum
pub const SIGNATURE: u32 = 0x5c6d8e9f;

#[derive(Debug)]
pub struct AxdlFrameView<'a> {
    data: &'a [u8],
}

impl<'a> std::fmt::Display for AxdlFrameView<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "AxdlFrameView({:08X}, {}, {:04X}, {:02X?}, {:04X})",
            self.signature().unwrap_or(0),
            self.length().unwrap_or(0),
            self.command_response().unwrap_or(0),
            self.payload().unwrap_or(&[]),
            self.checksum().unwrap_or(0)
        )
    }
}

impl<'a> AxdlFrameView<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    pub fn signature(&self) -> Option<u32> {
        if self.data.len() < 4 {
            return None;
        }
        Some(u32::from_le_bytes([
            self.data[0],
            self.data[1],
            self.data[2],
            self.data[3],
        ]))
    }

    pub fn length(&self) -> Option<u16> {
        if self.data.len() < 4 + 2 {
            return None;
        }
        Some(u16::from_le_bytes([self.data[4], self.data[5]]))
    }

    pub fn command_response(&self) -> Option<u16> {
        if self.data.len() < 4 + 2 + 2 {
            return None;
        }
        Some(u16::from_le_bytes([self.data[6], self.data[7]]))
    }

    pub fn payload(&self) -> Option<&[u8]> {
        let payload_length = self.length()? as usize;

        if self.data.len() < 4 + 2 + 2 + payload_length + 2 {
            return None;
        }

        Some(&self.data[4 + 2 + 2..4 + 2 + 2 + payload_length])
    }

    pub fn payload_unchecked(&self) -> Option<&[u8]> {
        if self.data.len() < 4 + 2 + 2 {
            return None;
        }
        Some(&self.data[4 + 2 + 2..self.data.len() - 2])
    }

    pub fn checksum(&self) -> Option<u16> {
        if self.data.len() < 4 + 2 + 2 + 2 {
            return None;
        }
        Some(u16::from_le_bytes([
            self.data[self.data.len() - 2],
            self.data[self.data.len() - 1],
        ]))
    }

    fn ones_complement_add(lhs: u16, rhs: u16) -> u16 {
        let mut sum = lhs as u32 + rhs as u32;

        while sum > 0xffff {
            sum = (sum & 0xffff) + (sum >> 16);
        }
        sum as u16
    }
    pub fn calculate_checksum(&self) -> Option<u16> {
        let payload = if let Some(payload) = self.payload() {
            payload
        } else {
            return None;
        };

        let length = self.length().unwrap();
        let command_response = self.command_response().unwrap();
        let mut checksum = self.checksum().unwrap();
        checksum = Self::ones_complement_add(checksum, length);
        checksum = Self::ones_complement_add(checksum, command_response);
        for i in 0..payload.len() / 2 {
            let value = u16::from_le_bytes([payload[i * 2], payload[i * 2 + 1]]);
            checksum = Self::ones_complement_add(checksum, value);
        }
        if payload.len() % 2 == 1 {
            checksum = Self::ones_complement_add(
                checksum,
                u16::from_le_bytes([payload[payload.len() - 1], 0]),
            );
        }

        Some(checksum)
    }

    pub fn verify_checksum(&self) -> bool {
        self.calculate_checksum()
            .map(|checksum| checksum == 0xffff)
            .unwrap_or(false)
    }

    pub fn is_valid(&self) -> bool {
        self.signature() == Some(SIGNATURE) && self.verify_checksum()
    }
}

pub struct AxdlFrameViewMut<'a> {
    buffer: &'a mut [u8],
}

impl<'a> AxdlFrameViewMut<'a> {
    pub fn new(buffer: &'a mut [u8]) -> Self {
        Self { buffer }
    }

    pub fn init(&mut self) -> &mut Self {
        let length = self.buffer.len();
        self.set_signature(SIGNATURE);
        self.set_length((length - MINIMUM_LENGTH) as u16);

        self
    }

    pub fn signature(&self) -> u32 {
        AxdlFrameView::new(self.buffer).signature().unwrap()
    }

    pub fn length(&self) -> u16 {
        AxdlFrameView::new(self.buffer).length().unwrap()
    }

    pub fn command_response(&self) -> u16 {
        AxdlFrameView::new(self.buffer).command_response().unwrap()
    }

    pub fn checksum(&self) -> u16 {
        AxdlFrameView::new(self.buffer).checksum().unwrap()
    }

    pub fn set_signature(&mut self, signature: u32) -> &mut Self {
        self.buffer[0] = (signature & 0xff) as u8;
        self.buffer[1] = ((signature >> 8) & 0xff) as u8;
        self.buffer[2] = ((signature >> 16) & 0xff) as u8;
        self.buffer[3] = ((signature >> 24) & 0xff) as u8;
        self
    }
    pub fn set_length(&mut self, length: u16) -> &mut Self {
        assert!(length as usize + 4 + 2 + 2 + 2 <= self.buffer.len());

        self.buffer[4] = (length & 0xff) as u8;
        self.buffer[5] = ((length >> 8) & 0xff) as u8;

        self
    }
    pub fn set_command_response(&mut self, command_response: u16) -> &mut Self {
        self.buffer[6] = (command_response & 0xff) as u8;
        self.buffer[7] = ((command_response >> 8) & 0xff) as u8;

        self
    }

    pub fn payload_mut(&mut self) -> &mut [u8] {
        let length = self.length() as usize;
        &mut self.buffer[4 + 2 + 2..4 + 2 + 2 + length]
    }

    pub fn set_checksum(&mut self, checksum: u16) -> &mut Self {
        let length = self.length() as usize;
        self.buffer[4 + 2 + 2 + length + 0] = (checksum & 0xff) as u8;
        self.buffer[4 + 2 + 2 + length + 1] = (checksum >> 8) as u8;

        self
    }

    pub fn finalize(mut self) {
        self.set_checksum(0);
        let checksum = AxdlFrameView::new(self.buffer)
            .calculate_checksum()
            .unwrap();
        self.set_checksum(!checksum);
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_axdl_frame_view_empty() {
        let data = hex_literal::hex!("9f 8e 6d 5c 00 00 01 00 fe ff");
        let view = AxdlFrameView::new(&data);
        assert_eq!(view.signature(), Some(SIGNATURE));
        assert_eq!(view.length(), Some(0));
        assert_eq!(view.command_response(), Some(0x0001));
        assert_eq!(view.payload(), Some(&data[4 + 2 + 2..4 + 2 + 2 as usize]));
        assert_eq!(view.checksum(), Some(0xfffe));
        assert_eq!(view.calculate_checksum(), Some(0xffff));
        assert_eq!(view.verify_checksum(), true);
        assert_eq!(view.is_valid(), true);
    }

    #[test]
    fn test_axdl_frame_view_command_1() {
        let data = hex_literal::hex!("9f 8e 6d 5c 08 00 01 00 00 00 00 03 00 68 01 00 f5 94");
        let view = AxdlFrameView::new(&data);
        assert_eq!(view.signature(), Some(SIGNATURE));
        assert_eq!(view.length(), Some(8));
        assert_eq!(view.command_response(), Some(0x0001));
        assert_eq!(
            view.payload(),
            Some(&data[4 + 2 + 2..4 + 2 + 8 + 2 as usize])
        );
        assert_eq!(view.checksum(), Some(0x94f5));
        assert_eq!(view.calculate_checksum(), Some(0xffff));
        assert_eq!(view.verify_checksum(), true);
        assert_eq!(view.is_valid(), true);
    }

    #[test]
    fn test_axdl_frame_view_command_2() {
        let data = hex_literal::hex!(
            "9F 8E 6D 5C 10 00 81 00 72 6F 6D 63 6F 64 65 20 76 31 2E 30 3B 72 61 77 79 5C"
        );
        let view = AxdlFrameView::new(&data);
        assert_eq!(view.signature(), Some(SIGNATURE));
        assert_eq!(view.length(), Some(16));
        assert_eq!(view.command_response(), Some(0x0081));
        assert_eq!(
            view.payload(),
            Some(&data[4 + 2 + 2..4 + 2 + 16 + 2 as usize])
        );
        assert_eq!(view.checksum(), Some(0x5c79));
        assert_eq!(view.calculate_checksum(), Some(0xffff));
        assert_eq!(view.verify_checksum(), true);
        assert_eq!(view.is_valid(), true);
    }

    #[test]
    fn test_axdl_frame_view_mut() {
        let mut data = [0u8; 12];

        let mut view_mut = AxdlFrameViewMut::new(&mut data);
        view_mut
            .init()
            .set_command_response(0x1234)
            .set_checksum(0x5678)
            .payload_mut()
            .copy_from_slice(&[0x9a, 0xbc]);
        drop(view_mut);

        let view = AxdlFrameView::new(&data);
        assert_eq!(view.signature(), Some(SIGNATURE));
        assert_eq!(view.length(), Some(2));
        assert_eq!(view.command_response(), Some(0x1234));
        assert_eq!(view.payload(), Some(&[0x9au8, 0xbc][..]));
        assert_eq!(view.checksum(), Some(0x5678));
    }

    #[test]
    fn test_axdl_frame_view_mut_empty() {
        let mut data = [0u8; 10];
        let mut view_mut = AxdlFrameViewMut::new(&mut data);
        view_mut.init();
        view_mut.finalize();

        let view = AxdlFrameView::new(&data);
        assert_eq!(view.signature(), Some(SIGNATURE));
        assert_eq!(view.length(), Some(0));
        assert_eq!(view.command_response(), Some(0x0000));
        assert_eq!(view.payload(), Some(&data[4 + 2 + 2..4 + 2 + 2 as usize]));
        assert_eq!(view.checksum(), Some(0xffff));
        assert_eq!(view.calculate_checksum(), Some(0xffff));
        assert_eq!(view.verify_checksum(), true);
        assert_eq!(view.is_valid(), true);
    }

    #[test]
    fn test_axdl_frame_view_mut_empty_command() {
        let mut data = [0u8; 10];
        let mut view_mut = AxdlFrameViewMut::new(&mut data);
        view_mut.init().set_command_response(0xcafe);
        view_mut.finalize();

        let view = AxdlFrameView::new(&data);
        assert_eq!(view.signature(), Some(SIGNATURE));
        assert_eq!(view.length(), Some(0));
        assert_eq!(view.command_response(), Some(0xcafe));
        assert_eq!(view.payload(), Some(&data[4 + 2 + 2..4 + 2 + 2 as usize]));
        assert_eq!(view.checksum(), Some(!0xcafe));
        assert_eq!(view.calculate_checksum(), Some(0xffff));
        assert_eq!(view.verify_checksum(), true);
        assert_eq!(view.is_valid(), true);
    }

    #[test]
    fn test_axdl_frame_view_mut_with_payload() {
        let mut data = [0u8; 12];
        let mut view_mut = AxdlFrameViewMut::new(&mut data);
        view_mut.init().set_command_response(0xcafe);
        view_mut.payload_mut().copy_from_slice(&[0x01, 0x02]);
        view_mut.finalize();

        let view = AxdlFrameView::new(&data);
        assert_eq!(view.signature(), Some(SIGNATURE));
        assert_eq!(view.length(), Some(2));
        assert_eq!(view.command_response(), Some(0xcafe));
        assert_eq!(view.payload(), Some(&[0x01, 0x02][..]));
        assert_eq!(view.checksum(), Some(!(0xcafe + 0x0002 + 0x0201)));
        assert_eq!(view.calculate_checksum(), Some(0xffff));
        assert_eq!(view.verify_checksum(), true);
        assert_eq!(view.is_valid(), true);
    }
}
