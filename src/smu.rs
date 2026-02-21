// Copyright 2026 Jean-Francois Roy
// SPDX-License-Identifier: Apache-2.0

//! SMU (System Management Unit) mailbox communication and PM table reading.
//!
//! The SMU exposes a mailbox interface through SMN registers. A command is sent
//! by writing arguments, then the command ID, then polling for a response.
//! The PM (Power Management) table is a binary blob the SMU writes to a
//! DRAM address; it contains FCLK, UCLK, and MCLK as IEEE 754 floats at
//! platform-specific offsets.

use anyhow::{Result, bail};

use crate::smn::SmnReader;

// Zen4/Zen5 desktop RSMU mailbox addresses (shared across Raphael & GraniteRidge).
const RSMU_ADDR_MSG: u32 = 0x03B1_0524;
const RSMU_ADDR_RSP: u32 = 0x03B1_0570;
const RSMU_ADDR_ARG: u32 = 0x03B1_0A40;

const SMU_CMD_TRANSFER_TABLE: u32 = 0x3;
const SMU_CMD_GET_DRAM_BASE: u32 = 0x4;
const SMU_CMD_GET_TABLE_VERSION: u32 = 0x5;

const SMU_RSP_OK: u32 = 0x01;
const SMU_MAILBOX_ARGS: usize = 6;
const SMU_TIMEOUT: u32 = 8192;

/// Clock frequencies read from the PM table.
#[derive(Debug, Clone, Default)]
pub struct Clocks {
    pub fclk_mhz: f32,
    pub uclk_mhz: f32,
    pub mclk_mhz: f32,
}

fn smu_wait_done(smn: &dyn SmnReader) -> Result<bool> {
    for _ in 0..SMU_TIMEOUT {
        let rsp = smn.read(RSMU_ADDR_RSP)?;
        if rsp != 0 {
            return Ok(true);
        }
    }
    Ok(false)
}

fn smu_send_command(smn: &dyn SmnReader, cmd: u32, args: &mut [u32; SMU_MAILBOX_ARGS]) -> Result<u32> {
    if !smu_wait_done(smn)? {
        bail!("SMU mailbox not ready (timeout waiting for initial ready)");
    }

    smn.write(RSMU_ADDR_RSP, 0)?;

    for (i, arg) in args.iter().enumerate() {
        smn.write(RSMU_ADDR_ARG + (i as u32) * 4, *arg)?;
    }

    smn.write(RSMU_ADDR_MSG, cmd)?;

    if !smu_wait_done(smn)? {
        bail!("SMU command {cmd:#x} timed out");
    }

    let status = smn.read(RSMU_ADDR_RSP)?;

    if status == SMU_RSP_OK {
        for (i, arg) in args.iter_mut().enumerate() {
            *arg = smn.read(RSMU_ADDR_ARG + (i as u32) * 4)?;
        }
    }

    Ok(status)
}

fn get_table_version(smn: &dyn SmnReader) -> Result<u32> {
    let mut args = [0u32; SMU_MAILBOX_ARGS];
    let status = smu_send_command(smn, SMU_CMD_GET_TABLE_VERSION, &mut args)?;
    if status != SMU_RSP_OK {
        bail!("GetTableVersion failed (status {status:#x})");
    }
    Ok(args[0])
}

fn get_dram_base_address(smn: &dyn SmnReader) -> Result<u64> {
    let mut args = [0u32; SMU_MAILBOX_ARGS];
    let status = smu_send_command(smn, SMU_CMD_GET_DRAM_BASE, &mut args)?;
    if status != SMU_RSP_OK {
        bail!("GetDramBaseAddress failed (status {status:#x})");
    }
    Ok(args[0] as u64)
}

fn transfer_table_to_dram(smn: &dyn SmnReader) -> Result<()> {
    let mut args = [0u32; SMU_MAILBOX_ARGS];
    let status = smu_send_command(smn, SMU_CMD_TRANSFER_TABLE, &mut args)?;
    if status != SMU_RSP_OK {
        bail!("TransferTableToDram failed (status {status:#x})");
    }
    Ok(())
}

struct PmTableDef {
    table_size: usize,
    offset_fclk: usize,
    offset_uclk: usize,
    offset_mclk: usize,
}

fn get_pm_table_def(version: u32) -> Option<PmTableDef> {
    let prefix = version >> 16;
    // Try exact version first, then fall back to generic per-family.
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
/// Requires the kernel module backend (for physical memory reads).
pub fn read_clocks(smn: &dyn SmnReader) -> Result<Clocks> {
    let version = get_table_version(smn)?;
    eprintln!("PM table version: {version:#010x}");

    let def = get_pm_table_def(version)
        .ok_or_else(|| anyhow::anyhow!(
            "unknown PM table version {version:#010x}; \
             clock frequencies cannot be read"
        ))?;

    let dram_base = get_dram_base_address(smn)?;
    if dram_base == 0 {
        bail!("SMU returned DRAM base address 0");
    }
    eprintln!("PM table DRAM base: {dram_base:#010x}, size: {:#x}", def.table_size);

    transfer_table_to_dram(smn)?;

    let mut table = vec![0u8; def.table_size];
    smn.read_phys(dram_base, &mut table)?;

    let fclk = read_f32_le(&table, def.offset_fclk);
    let uclk = read_f32_le(&table, def.offset_uclk);
    let mclk = read_f32_le(&table, def.offset_mclk);

    Ok(Clocks {
        fclk_mhz: fclk,
        uclk_mhz: uclk,
        mclk_mhz: mclk,
    })
}
