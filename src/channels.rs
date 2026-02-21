// Copyright 2026 Jean-Francois Roy
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Result, bail};

use crate::smn::SmnReader;

const MAX_CHANNELS: u32 = 12;
const DRAM_TYPE_REG: u32 = 0x50100;
const DRAM_TYPE_MASK: u32 = 0x3;
const CHANNEL_ENABLE_REG: u32 = 0x50DF0;
const DIMM0_PRESENT_REG: u32 = 0x50000;
const DIMM1_PRESENT_REG: u32 = 0x50008;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemType {
    Ddr4,
    Ddr5,
    LpDdr4,
    LpDdr5,
    Unknown(u32),
}

impl std::fmt::Display for MemType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemType::Ddr4 => write!(f, "DDR4"),
            MemType::Ddr5 => write!(f, "DDR5"),
            MemType::LpDdr4 => write!(f, "LPDDR4"),
            MemType::LpDdr5 => write!(f, "LPDDR5"),
            MemType::Unknown(v) => write!(f, "Unknown({v})"),
        }
    }
}

impl MemType {
    fn from_register(val: u32) -> Self {
        match val & DRAM_TYPE_MASK {
            0 => MemType::Ddr4,
            1 => MemType::Ddr5,
            2 => MemType::LpDdr4,
            3 => MemType::LpDdr5,
            other => MemType::Unknown(other),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Dimm {
    pub slot: String,
}

#[derive(Debug, Clone)]
pub struct Channel {
    pub index: u32,
    pub offset: u32,
    pub dimms: Vec<Dimm>,
}

/// Probe UMC channels and detect memory type.
///
/// Returns `(mem_type, channels)` where channels is the list of enabled
/// channels with at least one DIMM present.
pub fn detect(smn: &dyn SmnReader) -> Result<(MemType, Vec<Channel>)> {
    let mut channels = Vec::new();
    let mut mem_type = MemType::Unknown(0xFF);

    for i in 0..MAX_CHANNELS {
        let offset = i << 20;

        // Read channel-enable bit (bit 19 == 0 means enabled).
        let ch_reg = smn
            .read(offset | CHANNEL_ENABLE_REG)
            .unwrap_or(0xFFFF_FFFF);
        let channel_enabled = (ch_reg >> 19) & 1 == 0;

        let dimm0_present = smn.read(offset | DIMM0_PRESENT_REG).unwrap_or(0) & 1 == 1;
        let dimm1_present = smn.read(offset | DIMM1_PRESENT_REG).unwrap_or(0) & 1 == 1;

        if !channel_enabled || (!dimm0_present && !dimm1_present) {
            continue;
        }

        // Detect memory type from the first enabled channel.
        if matches!(mem_type, MemType::Unknown(_)) {
            let type_reg = smn.read(offset | DRAM_TYPE_REG)?;
            mem_type = MemType::from_register(type_reg);
        }

        let label = char::from(b'A' + i as u8);
        let mut dimms = Vec::new();
        if dimm0_present {
            dimms.push(Dimm {
                slot: format!("{label}1"),
            });
        }
        if dimm1_present {
            dimms.push(Dimm {
                slot: format!("{label}2"),
            });
        }

        channels.push(Channel {
            index: i,
            offset,
            dimms,
        });
    }

    if channels.is_empty() {
        bail!("no enabled memory channels found");
    }

    Ok((mem_type, channels))
}
