// Copyright 2026 Jean-Francois Roy
// SPDX-License-Identifier: Apache-2.0

//! PM (Power Management) table parsing for clock frequencies.
//!
//! The kernel module handles the SMU mailbox protocol and physical memory
//! access internally. Userspace receives the raw PM table bytes and the
//! table version, then extracts FCLK, UCLK, and MCLK at version-dependent
//! offsets.

use anyhow::{Result, bail};

use crate::smn::SmnReader;

/// Clock frequencies read from the PM table.
#[derive(Debug, Clone, Default)]
pub struct Clocks {
    pub fclk_mhz: f32,
    pub uclk_mhz: f32,
    pub mclk_mhz: f32,
}

struct PmTableDef {
    table_size: usize,
    offset_fclk: usize,
    offset_uclk: usize,
    offset_mclk: usize,
}

fn get_pm_table_def(version: u32) -> Option<PmTableDef> {
    let prefix = version >> 16;
    match version {
        // Zen4 Desktop (Raphael) known versions
        0x540000..=0x540199 => Some(PmTableDef {
            table_size: 0x8C8,
            offset_fclk: 0x118,
            offset_uclk: 0x128,
            offset_mclk: 0x138,
        }),
        0x540208 => Some(PmTableDef {
            table_size: 0x8D0,
            offset_fclk: 0x11C,
            offset_uclk: 0x12C,
            offset_mclk: 0x13C,
        }),
        // Zen5 GraniteRidge known versions
        0x620105 | 0x621102 => Some(PmTableDef {
            table_size: 0x724,
            offset_fclk: 0x11C,
            offset_uclk: 0x12C,
            offset_mclk: 0x13C,
        }),
        0x620205 | 0x621202 => Some(PmTableDef {
            table_size: 0x994,
            offset_fclk: 0x11C,
            offset_uclk: 0x12C,
            offset_mclk: 0x13C,
        }),
        _ => {
            // Generic fallback by family prefix
            match prefix {
                0x54 => Some(PmTableDef {
                    table_size: 0x948,
                    offset_fclk: 0x118,
                    offset_uclk: 0x128,
                    offset_mclk: 0x138,
                }),
                0x62 => Some(PmTableDef {
                    table_size: 0x994,
                    offset_fclk: 0x11C,
                    offset_uclk: 0x12C,
                    offset_mclk: 0x13C,
                }),
                0x5C => Some(PmTableDef {
                    table_size: 0xD9C,
                    offset_fclk: 0x19C,
                    offset_uclk: 0x1B0,
                    offset_mclk: 0x1C4,
                }),
                0x73 => Some(PmTableDef {
                    table_size: 0xAFC,
                    offset_fclk: 0x20C,
                    offset_uclk: 0x21C,
                    offset_mclk: 0x22C,
                }),
                _ => None,
            }
        }
    }
}

fn read_f32_le(data: &[u8], offset: usize) -> f32 {
    let bytes: [u8; 4] = data[offset..offset + 4].try_into().unwrap();
    f32::from_le_bytes(bytes)
}

/// Read FCLK, UCLK, MCLK from the SMU PM table.
///
/// Requires the kernel module backend.
pub fn read_clocks(smn: &dyn SmnReader) -> Result<Clocks> {
    // First pass: request with the maximum possible size to get the version.
    let result = smn.read_pm_table(16 * 1024)?;
    let version = result.version;
    eprintln!("PM table version: {version:#010x}");

    let def = get_pm_table_def(version)
        .ok_or_else(|| anyhow::anyhow!(
            "unknown PM table version {version:#010x}; \
             clock frequencies cannot be read"
        ))?;

    // Use the data we already have if it's large enough, otherwise re-read.
    let table = if result.data.len() >= def.table_size {
        result.data
    } else {
        bail!(
            "PM table too small: got {} bytes but need {} for version {version:#010x}",
            result.data.len(),
            def.table_size
        );
    };

    let fclk = read_f32_le(&table, def.offset_fclk);
    let uclk = read_f32_le(&table, def.offset_uclk);
    let mclk = read_f32_le(&table, def.offset_mclk);

    Ok(Clocks {
        fclk_mhz: fclk,
        uclk_mhz: uclk,
        mclk_mhz: mclk,
    })
}
