use std::{cmp, fmt::{self, Display}};

use crate::{emu::emu::Emu, util::*};

const DOTS_PER_OAM_SCAN: u16 = 80;
const DOTS_PER_DRAW: u16 = 172; // min
const DOTS_PER_HBLANK: u16 = 87; // min
const DOTS_PER_VBLANK: u16 = 4560;

const REG_LCDC: u16 = 0xFF40;
const REG_LY: u16 = 0xFF44;

const REG_SCY: u16 = 0xFF42; // scroll y
const REG_SCX: u16 = 0xFF43; // scroll x

const ADDR_TILEMAP_1: u16 = 0x9800;
const ADDR_TILEMAP_2: u16 = 0x9C00;

// TILE NUMBERS     $9800-$9BFF AND $9C00-$9FFF
// TILE DATA        $8000-$97FF

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
    dots_leftover: u16,

    fetcher_internal_x: u16, // @todo - better name
}

impl Display for PPU {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "PPU")?;
        writeln!(f, "mode={:?}", self.mode)?;
        writeln!(f, "dots_mode={:?}", self.dots_mode)?;
        writeln!(f, "dots_scanline={:?}", self.dots_scanline)?;
        Ok(())
    }
}

impl PPU {
    pub fn new() -> Self {
        Self {
            mode: PpuMode::PpuOamScan,
            dots_mode: 0,
            dots_scanline: 0,
            dots_leftover: 0,
            fetcher_internal_x: 0,
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
            todo!("do we get here");
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
    // Mode 3 Length: https://gbdev.io/pandocs/Rendering.html#mode-3-length
    // 12 dots penalty at the start of scanline - https://hacktix.github.io/GBEDG/ppu/#background-pixel-fetching
    let remaining_budget = DOTS_PER_DRAW - emu.ppu.dots_mode;
    let dots = cmp::min(remaining_budget, dots_to_run);

    // println!("mode_draw - remaining_budget={}, dots_to_spend={}, budget={}", remaining_budget, dots_to_spend, dots);

    if dots == 0 {
        emu.ppu.dots_mode = 0;
        emu.ppu.mode = PpuMode::PpuHBlank;
        // println!("mode_draw - switching to PpuMode::PpuHBlank");
        return Some(0);
    }

    /*
        The FIFO and Pixel Fetcher work together to ensure that the FIFO always contains at least 8 pixels at any given time,
        as 8 pixels are required for the Pixel Rendering operation to take place.
        Each FIFO is manipulated only during mode 3 (pixel transfer).
        https://gbdev.io/pandocs/pixel_fifo.html#get-tile
    */

    // @todo - DOTS consumption
    // Number of available dots is unknown
    // Action need to be taken only if required dots are available
    //  - Otherwise return consumed dots or None to wait for more
    //  - If there are no actions to take, return None to wait for more dots
    //  if consume(dots, 4) { ...action } else { other action OR return None }

    // 1. Clock pixelfetchers (2 dots each)

    // 1) Fetch Tile No
    let fetch_bg = true; // bg | window

    let tilemap_index = emu.bus_read(REG_LCDC) & (1 << 3); // (1 << 6) for window
    let tilemap_addr = if tilemap_index == 0 {
        ADDR_TILEMAP_1
    } else {
        ADDR_TILEMAP_2
    };

    let mut x_coord: u16 = 0;
    let mut y_coord: u16 = 0;

    let ly = emu.bus_read(REG_LY);
    let scy = emu.bus_read(REG_SCY);

    if fetch_bg {
        // not in window
        let scx = emu.bus_read(REG_SCX);

        x_coord = ((u16::from(scx) / 8) + emu.ppu.fetcher_internal_x) & 0x1F;
        y_coord = (u16::from(ly) + u16::from(scy)) & 0xFF;
    }

    let current_tile_index = (x_coord + 32 * (y_coord / 8)) & 0x3FF;
    // println!("{current_tile_index}: {y_coord} {x_coord}");

    let tilemap_data_addr = tilemap_addr + current_tile_index;

    let tile_number = emu.bus_read(tilemap_data_addr);

    let addressing_mode_8000 = emu.bus_read(REG_LCDC) & (1 << 4);

    // 2) Fetch Tile Data (Low)

    let tile_lsb = if addressing_mode_8000 == 1 {
        emu.bus_read(0x8000 + u16::from(tile_number))
    } else {
        // todo!("do we get here");
        let e: i8;
        unsafe { e = std::mem::transmute::<u8, i8>(tile_number); }
        emu.bus_read((0x9000 as u16).wrapping_add_signed(e.into()))
    };

    // 3) Fetch Tile Data (High)
    // Note: 12 dot penalty (this step is restarted, took 6 steps to get here, restart -> 12 steps to continue)
    let tile_msb = if addressing_mode_8000 == 1 {
        emu.bus_read(0x8000 + u16::from(tile_number) + 1)
    } else {
        // todo!("do we get here");
        let e: i8;
        unsafe { e = std::mem::transmute::<u8, i8>(tile_number); }
        emu.bus_read((0x9000 as u16).wrapping_add_signed(e.into()) + 1)
    };

    // let tile_data = util::value(tile_msb, tile_lsb);
    // println!("{:#x?}", tile_data);

    // Decoding tile to pixels experiment
    {
        for bit_idx in (0..8).rev() {
            let hb = (tile_msb >> bit_idx) & 0x1;
            let lb = (tile_lsb >> bit_idx) & 0x1;
            let color = lb | (hb << 1);
            if color == 3 {
                print!("#");
            } else if color == 2 {
                print!("*");
            } else if color == 1 {
                print!(".");
            } else if color == 0 {
                print!(" ");
            }
        }
    }

    // panic!();

    emu.ppu.fetcher_internal_x += 1;

    if emu.ppu.fetcher_internal_x == 160 {
        print!("\n");
        // reset, move to hblank
        emu.ppu.fetcher_internal_x = 0;
        emu.ppu.dots_mode = 0;
        emu.ppu.mode = PpuMode::PpuHBlank;
        return Some(0); // Some(dots); // @todo actual dots that need to be consumed
    }

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
    // panic!();
    if dots == 0 {
        emu.ppu.dots_mode = 0;
        emu.ppu.dots_scanline = 0;

        let current_ly = emu.bus_read(REG_LY);

        let ly_next = current_ly + 1;
        emu.bus_write(REG_LY, ly_next);

        emu.ppu.mode = if current_ly == 143 {
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
    let ly_current = emu.bus_read(REG_LY);

    if emu.ppu.dots_mode != 0 && emu.ppu.dots_mode % 456 == 0 {
        let ly_next = ly_current + 1;
        emu.bus_write(REG_LY, ly_next);
    }

    // let vblank_end = dots == 0;
    let vblank_end = emu.ppu.dots_mode % 456 == 0 && ly_current == 153;

    if vblank_end {
        emu.ppu.dots_mode = 0;
        emu.ppu.dots_scanline = 0;
        emu.ppu.mode = PpuMode::PpuOamScan;

        emu.bus_write(REG_LY, 0);

        debug_assert!(dots == 0);

        println!("mode_vblank - NEXT FRAME");
        return Some(0);
    }

    return Some(dots);
}
