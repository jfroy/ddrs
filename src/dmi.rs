// Copyright 2026 Jean-Francois Roy
// SPDX-License-Identifier: Apache-2.0

//! Parse SMBIOS Type 17 (Memory Device) entries from Linux sysfs to get DIMM
//! part numbers and manufacturer strings.

use std::fs;
use std::path::Path;

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

/// Extract the Nth null-terminated string from the string area that follows
/// an SMBIOS structure header. String indices are 1-based; index 0 means
/// "no string".
fn smbios_string(string_area: &[u8], index: u8) -> String {
    if index == 0 {
        return String::new();
    }
    let mut current = 1u8;
    let mut start = 0;
    for (i, &b) in string_area.iter().enumerate() {
        if b == 0 {
            if current == index {
                return String::from_utf8_lossy(&string_area[start..i])
                    .trim()
                    .to_string();
            }
            current += 1;
            start = i + 1;
        }
    }
    String::new()
}

/// Parse one SMBIOS Type 17 raw record.
fn parse_type17(raw: &[u8]) -> Option<DmiMemoryDevice> {
    if raw.len() < 0x1B {
        return None;
    }
    if raw[0] != 17 {
        return None;
    }
    let header_len = raw[1] as usize;
    if raw.len() < header_len {
        return None;
    }

    // Size at offset 0x0C (WORD LE): 0 = not installed, 0xFFFF = unknown.
    let size_raw = u16::from_le_bytes([raw[0x0C], raw[0x0D]]);
    if size_raw == 0 || size_raw == 0xFFFF {
        return None;
    }

    let manufacturer_idx = raw[0x17];
    let part_number_idx = raw[0x1A];

    let string_area = &raw[header_len..];

    Some(DmiMemoryDevice {
        manufacturer: smbios_string(string_area, manufacturer_idx),
        part_number: smbios_string(string_area, part_number_idx),
    })
}

/// Read all populated SMBIOS Type 17 entries from sysfs.
///
/// Returns entries in sysfs enumeration order (which matches physical slot
/// order), filtered to only DIMMs with non-zero size.
pub fn read_memory_devices() -> Vec<DmiMemoryDevice> {
    let base = Path::new("/sys/firmware/dmi/entries");
    let mut entries = Vec::new();

    // Collect entry directory names matching "17-N".
    let Ok(dir) = fs::read_dir(base) else {
        return entries;
    };
    let mut dirs: Vec<_> = dir
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.starts_with("17-"))
        })
        .collect();

    // Sort by instance number so order is deterministic.
    dirs.sort_by_key(|e| {
        e.file_name()
            .to_str()
            .and_then(|n| n.strip_prefix("17-"))
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(u32::MAX)
    });

    for entry in dirs {
        let raw_path = entry.path().join("raw");
        if let Ok(raw) = fs::read(&raw_path) {
            if let Some(dev) = parse_type17(&raw) {
                entries.push(dev);
            }
        }
    }

    entries
}
