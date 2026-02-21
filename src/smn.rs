// Copyright 2026 Jean-Francois Roy
// SPDX-License-Identifier: Apache-2.0

use std::fs::{File, OpenOptions};
use std::os::unix::io::AsRawFd;
use std::path::Path;

use anyhow::{Context, Result, bail};

// Must match kernel/amd_smn.h
const AMD_SMN_IOC_MAGIC: u8 = b'S';
const AMD_SMN_IOC_READ_NR: u8 = 1;
const AMD_SMN_IOC_READ_PM_TABLE_NR: u8 = 4;

#[repr(C)]
#[derive(Default)]
struct AmdSmnReq {
    address: u32,
    value: u32,
}

#[repr(C)]
#[derive(Default)]
struct AmdPmTableReq {
    version: u32,
    size: u32,
    buffer: u64,
}

nix::ioctl_readwrite!(smn_ioctl_read, AMD_SMN_IOC_MAGIC, AMD_SMN_IOC_READ_NR, AmdSmnReq);
nix::ioctl_readwrite!(smn_ioctl_read_pm_table, AMD_SMN_IOC_MAGIC, AMD_SMN_IOC_READ_PM_TABLE_NR, AmdPmTableReq);

const SMN_PCI_ADDR_OFFSET: u64 = 0xC4;
const SMN_PCI_DATA_OFFSET: u64 = 0xC8;

/// Result of reading the SMU PM table.
pub struct PmTableResult {
    pub version: u32,
    pub data: Vec<u8>,
}

pub trait SmnReader {
    fn read(&self, address: u32) -> Result<u32>;

    /// Read the SMU PM table. The kernel module handles the entire SMU mailbox
    /// flow and returns the table version alongside the data.
    /// Only supported by the kernel module backend.
    fn read_pm_table(&self, max_size: usize) -> Result<PmTableResult>;
}

/// Reads/writes SMN registers through the amd_smn kernel module (/dev/amd_smn).
pub struct KernelModuleReader {
    file: File,
}

impl KernelModuleReader {
    pub fn open() -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/amd_smn")
            .context("failed to open /dev/amd_smn (is the amd_smn module loaded?)")?;
        Ok(Self { file })
    }
}

impl SmnReader for KernelModuleReader {
    fn read(&self, address: u32) -> Result<u32> {
        let mut req = AmdSmnReq {
            address,
            value: 0,
        };
        unsafe { smn_ioctl_read(self.file.as_raw_fd(), &mut req) }
            .with_context(|| format!("SMN read ioctl failed for address {address:#010x}"))?;
        Ok(req.value)
    }

    fn read_pm_table(&self, max_size: usize) -> Result<PmTableResult> {
        let mut buf = vec![0u8; max_size];
        let mut req = AmdPmTableReq {
            version: 0,
            size: max_size as u32,
            buffer: buf.as_mut_ptr() as u64,
        };
        unsafe { smn_ioctl_read_pm_table(self.file.as_raw_fd(), &mut req) }
            .context("PM table read ioctl failed")?;
        buf.truncate(req.size as usize);
        Ok(PmTableResult {
            version: req.version,
            data: buf,
        })
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
        bail!("PM table reading is not supported via sysfs PCI config space; \
               use the amd_smn kernel module instead")
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

/// Auto-detect the best available SMN reader: kernel module first, then sysfs.
pub fn auto_detect() -> Result<Box<dyn SmnReader>> {
    if let Ok(reader) = KernelModuleReader::open() {
        eprintln!("Using amd_smn kernel module");
        return Ok(Box::new(reader));
    }
    if let Ok(reader) = SysfsPciReader::open() {
        eprintln!("Using sysfs PCI config space (direct)");
        return Ok(Box::new(reader));
    }
    bail!(
        "no SMN access method available.\n\
         Either load the amd_smn kernel module or run as root for sysfs access."
    );
}
