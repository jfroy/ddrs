// Copyright 2026 Jean-Francois Roy
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Result, bail};
use serde::Serialize;

use crate::dmi;
use crate::smn::SmnReader;

const MAX_CHANNELS: u32 = 12;
const DRAM_TYPE_REG: u32 = 0x50100;
const DRAM_TYPE_MASK: u32 = 0x3;
const CHANNEL_ENABLE_REG: u32 = 0x50DF0;
const DIMM0_PRESENT_REG: u32 = 0x50000;
const DIMM1_PRESENT_REG: u32 = 0x50008;

fn bits(val: u32, hi: u32, lo: u32) -> u32 {
    (val >> lo) & ((1 << (hi - lo + 1)) - 1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Rank {
    Single,
    Dual,
}

impl std::fmt::Display for Rank {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Rank::Single => write!(f, "SR"),
            Rank::Dual => write!(f, "DR"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Dimm {
    pub slot: String,
    pub rank: Rank,
    pub capacity_bytes: u64,
    pub model: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Channel {
    pub index: u32,
    #[serde(skip)]
    pub offset: u32,
    pub dimms: Vec<Dimm>,
}

impl Channel {
    pub fn total_capacity_bytes(&self) -> u64 {
        self.dimms.iter().map(|d| d.capacity_bytes).sum()
    }
}

/// Detect rank for a DDR5 DIMM from the rank config register.
fn detect_rank_ddr5(smn: &dyn SmnReader, rank_reg: u32) -> Rank {
    let val = smn.read(rank_reg).unwrap_or(0);
    if val != 0 && (val == 0x07FF_FBFE || bits(val, 10, 9) < 3) {
        return Rank::Dual;
    }
    let val2 = smn.read(rank_reg + 4).unwrap_or(0);
    if val2 != 0 && (val2 == 0x07FF_FBFE || bits(val2, 10, 9) < 3) {
        return Rank::Dual;
    }
    Rank::Single
}

/// Compute DIMM capacity from the DRAM address configuration register.
///
/// Register layout (DDR5):
///   [21:20] numBanks     (0=8, 1=16, 2=32, 3=64)
///   [19:16] numCol       (actual column bits = 5 + field)
///   [11:8]  numRow       (actual row bits = 10 + field)
///   [6:4]   numRM
///   [3:2]   numBankGroups (0=1, 1=2, 2=4, 3=8)
///
/// Capacity per rank = total_banks * 2^row_bits * 2^col_bits * 8 (bus width bytes)
fn compute_capacity(smn: &dyn SmnReader, addr_config_reg: u32, rank: Rank) -> u64 {
    let val = smn.read(addr_config_reg).unwrap_or(0);
    if val == 0 {
        return 0;
    }

    let num_banks_field = bits(val, 21, 20);
    let num_col_field = bits(val, 19, 16);
    let num_row_field = bits(val, 11, 8);

    let total_banks: u64 = 8 << num_banks_field;
    let rows: u64 = 1 << (10 + num_row_field);
    let cols: u64 = 1 << (5 + num_col_field);
    let bus_width_bytes: u64 = 8; // DDR5 subchannel = 64 bits

    let ranks: u64 = match rank {
        Rank::Single => 1,
        Rank::Dual => 2,
    };

    total_banks * rows * cols * bus_width_bytes * ranks
}

/// Probe UMC channels and detect memory type, ranks, capacity, and model strings.
pub fn detect(smn: &dyn SmnReader) -> Result<(MemType, Vec<Channel>)> {
    let mut channels = Vec::new();
    let mut mem_type = MemType::Unknown(0xFF);

    // Read SMBIOS Type 17 entries; they are consumed in order to match DIMMs.
    let mut dmi_devices = dmi::read_memory_devices().into_iter();

    for i in 0..MAX_CHANNELS {
        let offset = i << 20;

        let ch_reg = smn
            .read(offset | CHANNEL_ENABLE_REG)
            .unwrap_or(0xFFFF_FFFF);
        let channel_enabled = (ch_reg >> 19) & 1 == 0;

        let dimm0_present = smn.read(offset | DIMM0_PRESENT_REG).unwrap_or(0) & 1 == 1;
        let dimm1_present = smn.read(offset | DIMM1_PRESENT_REG).unwrap_or(0) & 1 == 1;

        if !channel_enabled || (!dimm0_present && !dimm1_present) {
            continue;
        }

        if matches!(mem_type, MemType::Unknown(_)) {
            let type_reg = smn.read(offset | DRAM_TYPE_REG)?;
            mem_type = MemType::from_register(type_reg);
        }

        let label = char::from(b'A' + i as u8);
        let mut dimms = Vec::new();

        if dimm0_present {
            let rank = detect_rank_ddr5(smn, offset | 0x50020);
            let capacity = compute_capacity(smn, offset | 0x50040, rank);
            let model = dmi_devices
                .next()
                .map(|d| d.model_string())
                .unwrap_or_default();
            dimms.push(Dimm {
                slot: format!("{label}1"),
                rank,
                capacity_bytes: capacity,
                model,
            });
        }
        if dimm1_present {
            let rank = detect_rank_ddr5(smn, offset | 0x50028);
            let capacity = compute_capacity(smn, offset | 0x50048, rank);
            let model = dmi_devices
                .next()
                .map(|d| d.model_string())
                .unwrap_or_default();
            dimms.push(Dimm {
                slot: format!("{label}2"),
                rank,
                capacity_bytes: capacity,
                model,
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

pub fn format_capacity(bytes: u64) -> String {
    const GIB: u64 = 1024 * 1024 * 1024;
    const MIB: u64 = 1024 * 1024;
    if bytes >= GIB {
        let gib = bytes as f64 / GIB as f64;
        format!("{gib:.1} GiB")
    } else if bytes >= MIB {
        let mib = bytes as f64 / MIB as f64;
        format!("{mib:.1} MiB")
    } else {
        format!("{bytes} B")
    }
}
