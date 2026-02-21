// Copyright 2026 Jean-Francois Roy
// SPDX-License-Identifier: Apache-2.0

use crate::channels::Channel;
use crate::timings::Ddr5Timings;

fn enabled_disabled(v: bool) -> &'static str {
    if v { "Enabled" } else { "Disabled" }
}

fn cmd_rate(v: bool) -> &'static str {
    if v { "2T" } else { "1T" }
}

pub fn print_channel(channel: &Channel, t: &Ddr5Timings) {
    let dimm_slots: Vec<&str> = channel.dimms.iter().map(|d| d.slot.as_str()).collect();

    println!("══════════════════════════════════════════════════");
    println!(
        "  Channel {} (UMC{})  Slots: {}",
        char::from(b'A' + channel.index as u8),
        channel.index,
        dimm_slots.join(", ")
    );
    println!("══════════════════════════════════════════════════");
    println!();

    println!("  Frequency        {:.0} MT/s (ratio {:.2})", t.frequency, t.ratio);
    println!("  Command Rate     {}", cmd_rate(t.cmd2t));
    println!("  Gear Down Mode   {}", enabled_disabled(t.gdm));
    println!("  Power Down       {}", enabled_disabled(t.power_down));
    println!("  Bank Group Swap  {}", enabled_disabled(t.bgs));
    println!("  BGS Alt          {}", enabled_disabled(t.bgs_alt));
    println!();

    println!("  ── Primary ──────────────────────────────────");
    println!("  tCL              {}", t.cl);
    println!("  tRCDRD           {}", t.rcdrd);
    println!("  tRCDWR           {}", t.rcdwr);
    println!("  tRP              {}", t.rp);
    println!("  tRAS             {}", t.ras);
    println!("  tRC              {}", t.rc);
    println!();

    println!("  ── Secondary ────────────────────────────────");
    println!("  tRRDS            {}", t.rrds);
    println!("  tRRDL            {}", t.rrdl);
    println!("  tFAW             {}", t.faw);
    println!("  tWTRS            {}", t.wtrs);
    println!("  tWTRL            {}", t.wtrl);
    println!("  tWR              {}", t.wr);
    println!("  tRTP             {}", t.rtp);
    println!("  tCWL             {}", t.cwl);
    println!("  tRDWR            {}", t.rdwr);
    println!("  tWRRD            {}", t.wrrd);
    println!("  tTRCPAGE         {}", t.trcpage);
    println!();

    println!("  ── Read/Read ────────────────────────────────");
    println!("  tRDRDSCL         {}", t.rdrdscl);
    println!("  tRDRDSC          {}", t.rdrdsc);
    println!("  tRDRDSD          {}", t.rdrdsd);
    println!("  tRDRDDD          {}", t.rdrddd);
    println!();

    println!("  ── Write/Write ──────────────────────────────");
    println!("  tWRWRSCL         {}", t.wrwrscl);
    println!("  tWRWRSC          {}", t.wrwrsc);
    println!("  tWRWRSD          {}", t.wrwrsd);
    println!("  tWRWRDD          {}", t.wrwrdd);
    println!();

    println!("  ── Refresh ──────────────────────────────────");
    println!("  tRFC             {} ({:.2} ns)", t.rfc, t.rfc_ns());
    println!("  tRFC2            {} ({:.2} ns)", t.rfc2, t.rfc2_ns());
    println!("  tRFCsb           {}", t.rfcsb);
    println!("  tREFI            {} ({:.2} ns)", t.refi, t.refi_ns());
    println!("  Refresh Mode     {}", t.refresh_mode);
    println!("  FGR              {}", t.fgr);
    println!();

    println!("  ── Mode Register ────────────────────────────");
    println!("  tMRD             {}", t.mrd);
    println!("  tMOD             {}", t.mod_);
    println!("  tMRDPDA          {}", t.mrdpda);
    println!("  tMODPDA          {}", t.modpda);
    println!();

    println!("  ── Misc ─────────────────────────────────────");
    println!("  tSTAG            {}", t.stag);
    println!("  tSTAGsb          {}", t.stagsb);
    println!("  tCKE             {}", t.cke);
    println!("  tXP              {}", t.xp);
    println!();

    println!("  ── PHY ──────────────────────────────────────");
    println!("  PHYWRD           {}", t.phywrd);
    println!("  PHYWRL           {}", t.phywrl);
    println!("  PHYRDL           {}", t.phyrdl);
    println!();

    println!("  ── Preamble ─────────────────────────────────");
    println!("  WRPRE            {}", t.wrpre);
    println!("  RDPRE            {}", t.rdpre);
    println!();

    println!("  ── Nitro ────────────────────────────────────");
    println!("  Nitro            {} (Rx/Tx/Ctrl)", t.nitro);
    println!();
}
