// Copyright 2026 Jean-Francois Roy
// SPDX-License-Identifier: Apache-2.0

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use anyhow::{Context, Result, bail};

/// Result of reading the SMU PM table.
pub struct PmTableResult {
    pub version: u32,
    pub data: Vec<u8>,
}

pub trait SmnReader {
    fn read(&self, address: u32) -> Result<u32>;

    /// Read the SMU PM table from ryzen_smu sysfs.
    /// Only supported by the ryzen_smu backend.
    fn read_pm_table(&self, max_size: usize) -> Result<PmTableResult>;
}

const RYZEN_SMU_BASE: &str = "/sys/kernel/ryzen_smu_drv";

/// Reads SMN registers and PM table through the ryzen_smu kernel module sysfs.
///
/// Uses `/sys/kernel/ryzen_smu_drv/smn` for SMN access and the pm_table* files
/// for clock frequencies. Requires the [ryzen_smu](https://github.com/amkillam/ryzen_smu)
/// module to be loaded.
pub struct RyzenSmuReader {
    smn_file: File,
}

impl RyzenSmuReader {
    pub fn open() -> Result<Self> {
        let smn_path = format!("{RYZEN_SMU_BASE}/smn");
        let smn_file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&smn_path)
            .with_context(|| {
                format!(
                    "failed to open {smn_path} (is the ryzen_smu module loaded?)"
                )
            })?;
        Ok(Self { smn_file })
    }

    fn read_smn(&self, address: u32) -> Result<u32> {
        let mut file = &self.smn_file;
        file.seek(SeekFrom::Start(0))?;
        file.write_all(&address.to_le_bytes())
            .with_context(|| format!("SMN write failed for address {address:#010x}"))?;
        file.seek(SeekFrom::Start(0))?;
        let mut buf = [0u8; 4];
        file.read_exact(&mut buf)
            .with_context(|| format!("SMN read failed for address {address:#010x}"))?;
        Ok(u32::from_le_bytes(buf))
    }

    fn read_pm_table_inner(&self, max_size: usize) -> Result<PmTableResult> {
        let version_path = format!("{RYZEN_SMU_BASE}/pm_table_version");
        let size_path = format!("{RYZEN_SMU_BASE}/pm_table_size");
        let table_path = format!("{RYZEN_SMU_BASE}/pm_table");

        if !Path::new(&version_path).exists() {
            bail!(
                "PM table not available (pm_table_version missing). \
                 ryzen_smu may not support PM table on this platform."
            );
        }

        let version = {
            let mut f = File::open(&version_path)
                .with_context(|| format!("failed to open {version_path}"))?;
            let mut buf = [0u8; 4];
            f.read_exact(&mut buf)?;
            u32::from_le_bytes(buf)
        };

        let size = {
            let mut f = File::open(&size_path)
                .with_context(|| format!("failed to open {size_path}"))?;
            let mut buf = [0u8; 8];
            f.read_exact(&mut buf)?;
            u64::from_le_bytes(buf) as usize
        };

        let mut data = vec![0u8; size.min(max_size)];
        let mut f = File::open(&table_path)
            .with_context(|| format!("failed to open {table_path}"))?;
        let n = f.read(&mut data)?;
        data.truncate(n);

        Ok(PmTableResult { version, data })
    }
}

impl SmnReader for RyzenSmuReader {
    fn read(&self, address: u32) -> Result<u32> {
        self.read_smn(address)
    }

    fn read_pm_table(&self, max_size: usize) -> Result<PmTableResult> {
        self.read_pm_table_inner(max_size)
    }
}

/// Reads/writes SMN registers directly through sysfs PCI config space.
///
/// This writes the SMN address to PCI config offset 0xC4 and reads/writes
/// through offset 0xC8 on the AMD host bridge (typically 0000:00:00.0).
/// Requires root or CAP_SYS_RAWIO.
pub struct SysfsPciReader {
    file: File,
}

const SMN_PCI_ADDR_OFFSET: u64 = 0xC4;
const SMN_PCI_DATA_OFFSET: u64 = 0xC8;

impl SysfsPciReader {
    pub fn open() -> Result<Self> {
        let config_path = find_amd_host_bridge_config()?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&config_path)
            .with_context(|| {
                format!("failed to open {config_path} (are you root?)")
            })?;
        Ok(Self { file })
    }
}

impl SmnReader for SysfsPciReader {
    fn read(&self, address: u32) -> Result<u32> {
        use std::os::unix::fs::FileExt;

        let addr_bytes = address.to_le_bytes();
        self.file
            .write_at(&addr_bytes, SMN_PCI_ADDR_OFFSET)
            .with_context(|| {
                format!("pwrite to PCI config offset {SMN_PCI_ADDR_OFFSET:#x} failed")
            })?;

        let mut data_bytes = [0u8; 4];
        self.file
            .read_at(&mut data_bytes, SMN_PCI_DATA_OFFSET)
            .with_context(|| {
                format!("pread from PCI config offset {SMN_PCI_DATA_OFFSET:#x} failed")
            })?;

        Ok(u32::from_le_bytes(data_bytes))
    }

    fn read_pm_table(&self, _max_size: usize) -> Result<PmTableResult> {
        bail!(
            "PM table reading is not supported via sysfs PCI config space; \
             use the ryzen_smu kernel module instead"
        )
    }
}

fn find_amd_host_bridge_config() -> Result<String> {
    let base = "/sys/bus/pci/devices";
    let candidate = format!("{base}/0000:00:00.0/config");
    if Path::new(&candidate).exists() {
        let vendor_path = format!("{base}/0000:00:00.0/vendor");
        if let Ok(vendor) = std::fs::read_to_string(&vendor_path) {
            if vendor.trim() == "0x1022" {
                return Ok(candidate);
            }
        }
    }
    bail!(
        "could not find AMD host bridge PCI device at 0000:00:00.0 \
         (vendor 0x1022 expected)"
    );
}

/// Auto-detect the best available SMN reader: ryzen_smu first, then sysfs.
pub fn auto_detect() -> Result<Box<dyn SmnReader>> {
    if Path::new(&format!("{RYZEN_SMU_BASE}/smn")).exists() {
        if let Ok(reader) = RyzenSmuReader::open() {
            eprintln!("Using ryzen_smu kernel module");
            return Ok(Box::new(reader));
        }
    }
    if let Ok(reader) = SysfsPciReader::open() {
        eprintln!("Using sysfs PCI config space (direct)");
        return Ok(Box::new(reader));
    }
    bail!(
        "no SMN access method available.\n\
         Either load the ryzen_smu kernel module or run as root for sysfs access."
    );
}
