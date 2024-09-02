use std::{cmp, fmt::{self, Display}};

use crate::{emu::emu::Emu, util::*};

const DOTS_PER_OAM_SCAN: u16 = 80;
const DOTS_PER_DRAW: u16 = 172; // min
const DOTS_PER_HBLANK: u16 = 87; // min
const DOTS_PER_VBLANK: u16 = 4560;

const REG_LCDC: u16 = 0xFF40;

#[derive(Debug)]
pub enum PpuMode {
    PpuOamScan,
    PpuDraw,
    PpuHBlank,
    PpuVBlank
}

pub struct PPU {
    mode: PpuMode,
    dots_mode: u16,
    dots_scanline: u16,
    scanline_count: u8,
    dots_leftover: u16,
}

impl Display for PPU {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "PPU")?;
        writeln!(f, "mode={:?}", self.mode)?;
        writeln!(f, "mode_dots={:?}", self.dots_mode)?;
        writeln!(f, "scanline_dots={:?}", self.dots_scanline)?;
        writeln!(f, "scanline_count={:?}", self.scanline_count)?;
        Ok(())
    }
}

impl PPU {
    pub fn new() -> Self {
        Self {
            mode: PpuMode::PpuOamScan,
            dots_mode: 0,
            dots_scanline: 0,
            scanline_count: 0,
            dots_leftover: 0,
        }
    }
}

pub fn step(emu: &mut Emu, cycles_passed: u8) -> u8 {
    let mut dots_budget = u16::from(cycles_passed * 4) + emu.ppu.dots_leftover;

    while dots_budget > 0 {
        let mode_result = match emu.ppu.mode {
            PpuMode::PpuOamScan => { mode_oam_scan(emu, dots_budget) }
            PpuMode::PpuDraw => { mode_draw(emu, dots_budget) }
            PpuMode::PpuHBlank => { mode_hblank(emu, dots_budget) }
            PpuMode::PpuVBlank => { mode_vblank(emu, dots_budget) }
        };

        if let Some(dots_spent) = mode_result {
            emu.ppu.dots_scanline += dots_spent;
            emu.ppu.dots_mode += dots_spent;
            dots_budget -= dots_spent;
        } else {
            emu.ppu.dots_leftover = dots_budget;
            break;
        }
    }

    return 1;
}

fn mode_oam_scan(emu: &mut Emu, dots_to_run: u16) -> Option<u16> {
    let remaining_budget = DOTS_PER_OAM_SCAN - emu.ppu.dots_mode;
    let dots = cmp::min(remaining_budget, dots_to_run);

    // println!("mode_oam_scan - remaining_budget={}, dots_to_spend={}, budget={}", remaining_budget, dots_to_spend, dots);

    if dots == 0 {
        emu.ppu.dots_mode = 0;
        emu.ppu.mode = PpuMode::PpuDraw;
        // println!("mode_oam_scan - switching to PpuMode::PpuDraw");
        return Some(0);
    }

    if emu.ppu.dots_mode == 0 {
        /*
            @todo - OAM scanning https://hacktix.github.io/GBEDG/ppu/#timing-diagram
            New entry from OAM is checked every 8 dots, 10 objects buffered = 80 dots total
         */
    }

    return Some(dots);
}

fn mode_draw(emu: &mut Emu, dots_to_run: u16) -> Option<u16> {
    // @todo drawing takes 172–289 dots depending on factors
    // https://gbdev.io/pandocs/Rendering.html
    let remaining_budget = DOTS_PER_DRAW - emu.ppu.dots_mode;
    let dots = cmp::min(remaining_budget, dots_to_run);

    // println!("mode_draw - remaining_budget={}, dots_to_spend={}, budget={}", remaining_budget, dots_to_spend, dots);

    if dots == 0 {
        emu.ppu.dots_mode = 0;
        emu.ppu.mode = PpuMode::PpuHBlank;
        // println!("mode_draw - switching to PpuMode::PpuHBlank");
        return Some(0);
    }

    // 1. Clock pixelfetchers
    // 2. Check if any pixels in BG FIFO (if not, exit)
    // 3. Check if any pixels in Sprite FIFO (if yes, merge with BG pixel)
    // 4. Sprite fetching
    // 5. Window fetching
    // 6. End scanline

    return Some(dots);
}

fn mode_hblank(emu: &mut Emu, dots_to_run: u16) -> Option<u16> {
    // @todo hblank takes 87–204 dots depending on length of drawing (376 - mode 3’s duration)
    let remaining_budget = DOTS_PER_HBLANK - emu.ppu.dots_mode;
    let dots = cmp::min(remaining_budget, dots_to_run);

    // println!("mode_hblank - remaining_budget={}, dots_to_spend={}, budget={}", remaining_budget, dots_to_spend, dots);

    if dots == 0 {
        emu.ppu.dots_mode = 0;
        emu.ppu.scanline_count += 1;

        emu.ppu.mode = if emu.ppu.scanline_count == 144 {
            PpuMode::PpuVBlank
        } else {
            PpuMode::PpuOamScan
        };

        // println!("mode_hblank - switching to {:?}", emu.ppu.mode);
        return Some(0);
    }

    return Some(dots);
}

fn mode_vblank(emu: &mut Emu, dots_to_run: u16) -> Option<u16> {
    let remaining_budget = DOTS_PER_VBLANK - emu.ppu.dots_mode;
    let dots = cmp::min(remaining_budget, dots_to_run);

    // println!("mode_vblank - remaining_budget={}, dots_to_spend={}, budget={}", remaining_budget, dots_to_spend, dots);

    if dots == 0 {
        emu.ppu.dots_mode = 0;
        emu.ppu.dots_scanline = 0;
        emu.ppu.scanline_count = 0;
        emu.ppu.mode = PpuMode::PpuOamScan;
        // println!("mode_vblank - NEXT FRAME");
        return Some(0);
    }

    return Some(dots);
}
