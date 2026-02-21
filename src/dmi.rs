// Copyright 2026 Jean-Francois Roy
// SPDX-License-Identifier: Apache-2.0

//! Read DIMM part numbers and manufacturer strings from SMBIOS Type 17
//! (Memory Device) entries via smbios-lib.

use smbioslib::{MemorySize, SMBiosMemoryDevice, table_load_from_device};

/// Information extracted from an SMBIOS Type 17 record.
#[derive(Debug, Clone)]
pub struct DmiMemoryDevice {
    manufacturer: String,
    part_number: String,
}

impl DmiMemoryDevice {
    /// Formatted model string: "Manufacturer PartNumber"
    pub fn model_string(&self) -> String {
        let mfr = self.manufacturer.trim();
        let pn = self.part_number.trim();
        match (mfr.is_empty(), pn.is_empty()) {
            (true, true) => "Unknown".to_string(),
            (true, false) => pn.to_string(),
            (false, true) => mfr.to_string(),
            (false, false) => format!("{mfr} {pn}"),
        }
    }
}

fn is_installed(dev: &SMBiosMemoryDevice<'_>) -> bool {
    match dev.size() {
        Some(MemorySize::NotInstalled | MemorySize::Unknown) | None => {
            // SeeExtendedSize means size >= 32 GB, check extended_size.
            false
        }
        Some(MemorySize::SeeExtendedSize) => dev.extended_size().is_some(),
        Some(MemorySize::Kilobytes(_) | MemorySize::Megabytes(_)) => true,
    }
}

/// Read all populated SMBIOS Type 17 entries from the system.
///
/// Returns entries in SMBIOS table order (which matches physical slot order),
/// filtered to only DIMMs that are installed.
pub fn read_memory_devices() -> Vec<DmiMemoryDevice> {
    let Ok(smbios) = table_load_from_device() else {
        return Vec::new();
    };

    smbios
        .collect::<SMBiosMemoryDevice<'_>>()
        .into_iter()
        .filter(|dev| is_installed(dev))
        .map(|dev| DmiMemoryDevice {
            manufacturer: dev.manufacturer().to_string(),
            part_number: dev.part_number().to_string(),
        })
        .collect()
}
