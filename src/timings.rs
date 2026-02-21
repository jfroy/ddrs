// Copyright 2026 Jean-Francois Roy
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use serde::Serialize;

use crate::smn::SmnReader;

fn bits(val: u32, hi: u32, lo: u32) -> u32 {
    (val >> lo) & ((1 << (hi - lo + 1)) - 1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BankRefreshMode {
    Normal,
    Fgr,
    Mixed,
    PbOnly,
    #[allow(dead_code)]
    Unknown,
}

impl std::fmt::Display for BankRefreshMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BankRefreshMode::Normal => write!(f, "Normal"),
            BankRefreshMode::Fgr => write!(f, "FGR"),
            BankRefreshMode::Mixed => write!(f, "Mixed"),
            BankRefreshMode::PbOnly => write!(f, "PB Only"),
            BankRefreshMode::Unknown => write!(f, "Unknown"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct NitroSettings {
    pub rx_data: u8,
    pub tx_data: u8,
    pub ctrl_line: u8,
}

impl std::fmt::Display for NitroSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}/{}", self.rx_data, self.tx_data, self.ctrl_line)
    }
}

/// All DDR5 timing parameters read from the memory controller.
#[derive(Debug, Clone, Serialize)]
pub struct Ddr5Timings {
    pub ratio: f32,
    pub frequency: f32,

    // Command rate / mode
    pub cmd2t: bool,
    pub gdm: bool,
    pub power_down: bool,

    // Primary timings
    pub cl: u32,
    pub rcdrd: u32,
    pub rcdwr: u32,
    pub ras: u32,
    pub rp: u32,
    pub rc: u32,

    // Secondary timings
    pub rrds: u32,
    pub rrdl: u32,
    pub rtp: u32,
    pub faw: u32,
    pub cwl: u32,
    pub wtrs: u32,
    pub wtrl: u32,
    pub wr: u32,
    pub trcpage: u32,

    // Read-read / write-write
    pub rdrdscl: u32,
    pub rdrdsc: u32,
    pub rdrdsd: u32,
    pub rdrddd: u32,
    pub wrwrscl: u32,
    pub wrwrsc: u32,
    pub wrwrsd: u32,
    pub wrwrdd: u32,

    // Read-write / write-read
    pub rdwr: u32,
    pub wrrd: u32,

    // Refresh
    pub refi: u32,
    pub rfc: u32,
    pub rfc2: u32,
    pub rfcsb: u32,

    // Mode register
    pub mrd: u32,
    pub mod_: u32,
    pub mrdpda: u32,
    pub modpda: u32,

    // Misc
    pub stag: u32,
    pub stagsb: u32,
    pub cke: u32,
    pub xp: u32,

    // PHY
    pub phywrd: u32,
    pub phywrl: u32,
    pub phyrdl: u32,

    // Preamble
    pub wrpre: u32,
    pub rdpre: u32,

    // Bank group swap
    pub bgs: bool,
    pub bgs_alt: bool,

    // Nitro
    pub nitro: NitroSettings,

    // Refresh mode
    pub fgr: u32,
    pub refresh_mode: BankRefreshMode,
}

impl Ddr5Timings {
    pub fn rfc_ns(&self) -> f32 {
        to_ns(self.rfc, self.frequency)
    }

    pub fn rfc2_ns(&self) -> f32 {
        to_ns(self.rfc2, self.frequency)
    }

    pub fn refi_ns(&self) -> f32 {
        to_ns(self.refi, self.frequency)
    }
}

fn to_ns(clocks: u32, freq_mt: f32) -> f32 {
    if freq_mt <= 0.0 {
        return 0.0;
    }
    clocks as f32 * 2000.0 / freq_mt
}

const RFC_PLACEHOLDER: u32 = 0x00C0_0138;

/// Read all DDR5 timings for a single UMC channel at the given SMN offset.
pub fn read_ddr5(smn: &dyn SmnReader, offset: u32) -> Result<Ddr5Timings> {
    let r = |addr: u32| -> Result<u32> { smn.read(offset | addr) };

    // 0x50200: Ratio, Cmd2T, GDM
    let reg_50200 = r(0x50200)?;
    let ratio = bits(reg_50200, 15, 0) as f32 / 100.0;
    let cmd2t = bits(reg_50200, 17, 17) == 1;
    let gdm = bits(reg_50200, 18, 18) == 1;

    // 0x50204: CL, RAS, RCDRD, RCDWR
    let reg_50204 = r(0x50204)?;
    let cl = bits(reg_50204, 5, 0);
    let ras = bits(reg_50204, 14, 8);
    let rcdrd = bits(reg_50204, 21, 16);
    let rcdwr = bits(reg_50204, 29, 24);

    // 0x50208: RC, RP
    let reg_50208 = r(0x50208)?;
    let rc = bits(reg_50208, 7, 0);
    let rp = bits(reg_50208, 21, 16);

    // 0x5020C: RRDS, RRDL, RTP
    let reg_5020c = r(0x5020C)?;
    let rrds = bits(reg_5020c, 4, 0);
    let rrdl = bits(reg_5020c, 12, 8);
    let rtp = bits(reg_5020c, 28, 24);

    // 0x50210: FAW
    let faw = bits(r(0x50210)?, 7, 0);

    // 0x50214: CWL, WTRS, WTRL
    let reg_50214 = r(0x50214)?;
    let cwl = bits(reg_50214, 5, 0);
    let wtrs = bits(reg_50214, 12, 8);
    let wtrl = bits(reg_50214, 22, 16);

    // 0x50218: WR
    let wr = bits(r(0x50218)?, 7, 0);

    // 0x5021C: TRCPAGE
    let trcpage = bits(r(0x5021C)?, 31, 20);

    // 0x50220: RDRD*
    let reg_50220 = r(0x50220)?;
    let rdrddd = bits(reg_50220, 3, 0);
    let rdrdsd = bits(reg_50220, 11, 8);
    let rdrdsc = bits(reg_50220, 19, 16);
    let rdrdscl = bits(reg_50220, 29, 24);

    // 0x50224: WRWR*
    let reg_50224 = r(0x50224)?;
    let wrwrdd = bits(reg_50224, 3, 0);
    let wrwrsd = bits(reg_50224, 11, 8);
    let wrwrsc = bits(reg_50224, 19, 16);
    let wrwrscl = bits(reg_50224, 29, 24);

    // 0x50228: WRRD, RDWR
    let reg_50228 = r(0x50228)?;
    let wrrd = bits(reg_50228, 3, 0);
    let rdwr = bits(reg_50228, 13, 8);

    // 0x50230: REFI
    let refi = bits(r(0x50230)?, 15, 0);

    // 0x50234: MRD, MOD, MRDPDA, MODPDA
    let reg_50234 = r(0x50234)?;
    let mrd = bits(reg_50234, 5, 0);
    let mod_ = bits(reg_50234, 13, 8);
    let mrdpda = bits(reg_50234, 21, 16);
    let modpda = bits(reg_50234, 29, 24);

    // 0x50250: STAGsb, STAG
    let reg_50250 = r(0x50250)?;
    let stagsb = bits(reg_50250, 8, 0);
    let stag = bits(reg_50250, 26, 16);

    // 0x50254: XP, CKE
    let reg_50254 = r(0x50254)?;
    let xp = bits(reg_50254, 5, 0);
    let cke = bits(reg_50254, 28, 24);

    // 0x50258: PHYWRL, PHYRDL, PHYWRD
    let reg_50258 = r(0x50258)?;
    let phywrl = bits(reg_50258, 15, 8);
    let phyrdl = bits(reg_50258, 23, 16);
    let phywrd = bits(reg_50258, 26, 24);

    // 0x502A4: RDPRE, WRPRE
    let reg_502a4 = r(0x502A4)?;
    let rdpre = bits(reg_502a4, 2, 0);
    let wrpre = bits(reg_502a4, 10, 8) + 1; // zero-based in register, off by one

    // --- Special DDR5 reads ---

    // RFC / RFC2: probe 0x50260..0x5026C, take first != 0x00C00138
    let mut rfc = 0u32;
    let mut rfc2 = 0u32;
    for addr in [0x50260u32, 0x50264, 0x50268, 0x5026C] {
        let val = r(addr)?;
        if val != RFC_PLACEHOLDER {
            rfc = bits(val, 15, 0);
            rfc2 = bits(val, 31, 16);
            break;
        }
    }

    // RFCsb: probe 0x502C0..0x502CC, take first non-zero
    let mut rfcsb = 0u32;
    for addr in [0x502C0u32, 0x502C4, 0x502C8, 0x502CC] {
        let val = bits(r(addr)?, 10, 0);
        if val != 0 {
            rfcsb = val;
            break;
        }
    }

    // Nitro settings: 0x50284 bits 11:0
    let nitro_raw = bits(r(0x50284)?, 11, 0);
    let nitro = NitroSettings {
        ctrl_line: (nitro_raw & 0x3) as u8,
        tx_data: ((nitro_raw >> 2) & 0x3) as u8,  
        rx_data: ((nitro_raw >> 4) & 0x3) as u8,
    };

    // Refresh mode: 0x5012C
    let reg_5012c = r(0x5012C)?;
    let power_down = bits(reg_5012c, 28, 28) == 1;
    let fgr = bits(reg_5012c, 18, 16);
    let per_bank_refresh = bits(reg_5012c, 1, 1) == 1;

    let refresh_mode = if !per_bank_refresh {
        if fgr == 0 {
            BankRefreshMode::Normal
        } else {
            BankRefreshMode::Fgr
        }
    } else if fgr != 0 {
        BankRefreshMode::Mixed
    } else {
        BankRefreshMode::PbOnly
    };

    // BGS: 0x50050 / 0x50058
    let bgs0 = r(0x50050)?;
    let bgs1 = r(0x50058)?;
    let bgs = !(bgs0 == 0x8765_4321 && bgs1 == 0x8765_4321);

    // BGSAlt: 0x500D0 / 0x500D4
    let bgsa0 = r(0x500D0)?;
    let bgsa1 = r(0x500D4)?;
    let bgs_alt = bits(bgsa0, 10, 4) > 0 || bits(bgsa1, 10, 4) > 0;

    let frequency = ratio * 200.0;

    Ok(Ddr5Timings {
        ratio,
        frequency,
        cmd2t,
        gdm,
        power_down,
        cl,
        rcdrd,
        rcdwr,
        ras,
        rp,
        rc,
        rrds,
        rrdl,
        rtp,
        faw,
        cwl,
        wtrs,
        wtrl,
        wr,
        trcpage,
        rdrdscl,
        rdrdsc,
        rdrdsd,
        rdrddd,
        wrwrscl,
        wrwrsc,
        wrwrsd,
        wrwrdd,
        rdwr,
        wrrd,
        refi,
        rfc,
        rfc2,
        rfcsb,
        mrd,
        mod_,
        mrdpda,
        modpda,
        stag,
        stagsb,
        cke,
        xp,
        phywrd,
        phywrl,
        phyrdl,
        wrpre,
        rdpre,
        bgs,
        bgs_alt,
        nitro,
        fgr,
        refresh_mode,
    })
}
