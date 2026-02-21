// Copyright 2026 Jean-Francois Roy
// SPDX-License-Identifier: Apache-2.0

mod channels;
mod display;
mod dmi;
mod smn;
mod smu;
mod timings;

use anyhow::{Result, bail};
use clap::Parser;
use nix::sys::signal::{self, SigHandler, Signal};
use serde::Serialize;

#[derive(Parser)]
#[command(
    name = "ddrs",
    about = "Read DDR5 memory timings on AMD AM5 systems (Zen4/Zen5)"
)]
struct Cli {
    /// Force a specific SMN access backend instead of auto-detecting.
    #[arg(long, value_enum)]
    backend: Option<Backend>,

    /// Only show timings for a specific channel index (0-based).
    #[arg(long)]
    channel: Option<u32>,

    /// Skip reading FCLK/UCLK/MCLK from the SMU PM table.
    #[arg(long)]
    no_clocks: bool,

    /// Output as JSON instead of human-readable text.
    #[arg(long)]
    json: bool,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum Backend {
    /// Use the amd_smn kernel module (/dev/amd_smn).
    Module,
    /// Use sysfs PCI config space directly (requires root).
    Sysfs,
}

#[derive(Serialize)]
struct ChannelOutput {
    index: u32,
    label: String,
    total_capacity_bytes: u64,
    dimms: Vec<channels::Dimm>,
    timings: Option<timings::Ddr5Timings>,
}

#[derive(Serialize)]
struct JsonOutput {
    memory_type: String,
    active_channels: usize,
    total_capacity_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    clocks: Option<smu::Clocks>,
    channels: Vec<ChannelOutput>,
}

fn main() -> Result<()> {
    unsafe { signal::signal(Signal::SIGPIPE, SigHandler::SigDfl) }.ok();

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

    let clocks = if !cli.no_clocks {
        match smu::read_clocks(reader.as_ref()) {
            Ok(c) => Some(c),
            Err(e) => {
                eprintln!(
                    "Warning: could not read clocks from SMU PM table: {e:#}\n\
                     (use --no-clocks to skip, or ensure the amd_smn kernel module is loaded)"
                );
                None
            }
        }
    } else {
        None
    };

    let channel_outputs: Vec<ChannelOutput> = detected_channels
        .iter()
        .filter(|ch| cli.channel.is_none_or(|f| f == ch.index))
        .map(|ch| {
            let t = match timings::read_ddr5(reader.as_ref(), ch.offset) {
                Ok(t) => Some(t),
                Err(e) => {
                    eprintln!(
                        "Warning: failed to read timings for channel {} (UMC{}): {e}",
                        char::from(b'A' + ch.index as u8),
                        ch.index
                    );
                    None
                }
            };
            ChannelOutput {
                index: ch.index,
                label: String::from(char::from(b'A' + ch.index as u8)),
                total_capacity_bytes: ch.total_capacity_bytes(),
                dimms: ch.dimms.clone(),
                timings: t,
            }
        })
        .collect();

    if cli.json {
        let total_cap: u64 = channel_outputs.iter().map(|c| c.total_capacity_bytes).sum();
        let output = JsonOutput {
            memory_type: mem_type.to_string(),
            active_channels: channel_outputs.len(),
            total_capacity_bytes: total_cap,
            clocks,
            channels: channel_outputs,
        };
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        let total_cap: u64 = detected_channels.iter().map(|c| c.total_capacity_bytes()).sum();
        println!("Memory type: {mem_type}");
        println!("Active channels: {}", detected_channels.len());
        println!("Total capacity: {}", channels::format_capacity(total_cap));
        println!();

        if let Some(ref c) = clocks {
            println!("  ── Clocks ───────────────────────────────────");
            display::print_clocks(c);
        } else if !cli.no_clocks {
            println!();
        }

        for co in &channel_outputs {
            let ch = detected_channels.iter().find(|c| c.index == co.index).unwrap();
            if let Some(ref t) = co.timings {
                display::print_channel(ch, t);
            }
        }
    }

    Ok(())
}
