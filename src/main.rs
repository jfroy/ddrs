// Copyright 2026 Jean-Francois Roy
// SPDX-License-Identifier: Apache-2.0

mod channels;
mod display;
mod smn;
mod timings;

use anyhow::{Result, bail};
use clap::Parser;

#[derive(Parser)]
#[command(
    name = "ddr5timings",
    about = "Read DDR5 memory timings on AMD AM5 systems (Zen4/Zen5)"
)]
struct Cli {
    /// Force a specific SMN access backend instead of auto-detecting.
    #[arg(long, value_enum)]
    backend: Option<Backend>,

    /// Only show timings for a specific channel index (0-based).
    #[arg(long)]
    channel: Option<u32>,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum Backend {
    /// Use the amd_smn kernel module (/dev/amd_smn).
    Module,
    /// Use sysfs PCI config space directly (requires root).
    Sysfs,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let reader: Box<dyn smn::SmnReader> = match cli.backend {
        Some(Backend::Module) => Box::new(smn::KernelModuleReader::open()?),
        Some(Backend::Sysfs) => Box::new(smn::SysfsPciReader::open()?),
        None => smn::auto_detect()?,
    };

    let (mem_type, detected_channels) = channels::detect(reader.as_ref())?;

    if mem_type != channels::MemType::Ddr5 {
        bail!(
            "detected memory type {mem_type}, but only DDR5 is supported. \
             DDR4 register maps are not implemented."
        );
    }

    println!("Memory type: {mem_type}");
    println!("Active channels: {}", detected_channels.len());
    println!();

    for ch in &detected_channels {
        if let Some(filter) = cli.channel {
            if ch.index != filter {
                continue;
            }
        }

        match timings::read_ddr5(reader.as_ref(), ch.offset) {
            Ok(t) => display::print_channel(ch, &t),
            Err(e) => {
                eprintln!(
                    "Warning: failed to read timings for channel {} (UMC{}): {e}",
                    char::from(b'A' + ch.index as u8),
                    ch.index
                );
            }
        }
    }

    Ok(())
}
