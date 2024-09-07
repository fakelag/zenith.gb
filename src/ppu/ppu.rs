use std::{borrow::Borrow, cmp, collections::VecDeque, fmt::{self, Display}};

use crate::{cpu::{*}, emu::emu::Emu, util::*};

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

struct GbPixel {
    pub color: u8,
    pub palette: u8, // OBP0 / OBP1 for DMG, a value between 0 and 7 on CGB
    // pub sprite_priority: u8, // CGB
    pub bg_priority: u8,
}

struct Pixelfetcher {
    pub current_step: u8,
    pub fetcher_x: u8,
}

pub struct PPU {
    mode: PpuMode,
    dots_mode: u16,
    dots_scanline: u16,
    dots_leftover: u16,

    bg_fifo: VecDeque<GbPixel>,
    scx_discard_count: u8,

    pixelfetcher: Pixelfetcher,
    current_x: u8, // @todo - better name

    rt: [[u8; 160]; 144],
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
            pixelfetcher: Pixelfetcher{
                current_step: 0,
                fetcher_x: 0,
            },
            current_x: 0,
            bg_fifo: VecDeque::new(),
            scx_discard_count: 0,
            rt: [[0; 160]; 144],
        }
    }
}

pub fn step(emu: &mut Emu, cycles_passed: u8) -> u8 {
    let lcd_enable = emu.bus_read(REG_LCDC) & (1 << 7);
    if lcd_enable == 0 {
        return 0;
    }

    debug_assert!(emu.bus_read(REG_LY) <= 153);

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

    debug_assert!(dots > 0);

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

    // 1. Clock pixelfetchers
    Pixelfetcher::step(emu, dots_to_run);

    if emu.ppu.dots_mode == 0 {
        debug_assert!(emu.ppu.current_x == 0);
        // Start of scanline, discard scx % 8 pixels
        let scx = emu.bus_read(REG_SCX);
        emu.ppu.scx_discard_count = scx % 8;
    }
    
    if let Some(pixel) = emu.ppu.bg_fifo.pop_front() {
        if emu.ppu.scx_discard_count > 0 {
            emu.ppu.scx_discard_count -= 1;
        } else {
            let ly = emu.bus_read(REG_LY);

            emu.ppu.rt[ly as usize][emu.ppu.current_x as usize] = pixel.color;
            emu.ppu.current_x += 1;
        }
    }

    if emu.ppu.current_x == 160 {
        // reset, move to hblank
        emu.ppu.bg_fifo.clear();
        emu.ppu.scx_discard_count = 0;
        emu.ppu.pixelfetcher.reset();
        emu.ppu.current_x = 0;
        emu.ppu.dots_mode = 0;
        emu.ppu.mode = PpuMode::PpuHBlank;
        return Some(1); // Some(dots); // @todo actual dots that need to be consumed
    }

    // 2. Check if any pixels in BG FIFO (if not, exit)
    // 3. Check if any pixels in Sprite FIFO (if yes, merge with BG pixel)
    // 4. Sprite fetching
    // 5. Window fetching
    // 6. End scanline

    return Some(1);
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
            // set vblank interrupt
            let flags_if = emu.bus_read(cpu::HREG_IF);
            emu.bus_write(cpu::HREG_IF, flags_if | cpu::INTERRUPT_BIT_VBLANK);

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
        // @todo - Sending will fail when exiting the app.
        // - A way to close emu thread and exit gracefully
        emu.frame_chan.send(emu.ppu.rt).unwrap();

        emu.ppu.dots_mode = 0;
        emu.ppu.dots_scanline = 0;
        emu.ppu.mode = PpuMode::PpuOamScan;

        emu.bus_write(REG_LY, 0);

        debug_assert!(dots == 0);

        std::thread::sleep(std::time::Duration::from_micros((16.6 * 1000.0) as u64));
        println!("mode_vblank - NEXT FRAME {}", emu.bus_read(REG_SCX));
        return Some(0);
    }

    return Some(dots);
}

impl Pixelfetcher {
    fn reset(&mut self) {
        self.current_step = 0;
        self.fetcher_x = 0;
    }

    fn step(emu: &mut Emu, dots_to_run: u16) -> Option<u16> {
        if emu.ppu.bg_fifo.len() > 0 {
            return Some(1);
        }
        // 1) Fetch Tile No
        let tile_number = Pixelfetcher::fetch_tile_number(emu);

        // 2) Fetch Tile Data (Low)
        let tile_lsb = Pixelfetcher::fetch_tile_byte(emu, tile_number, 0);

        // 3) Fetch Tile Data (High)
        // Note: 12 dot penalty (this step is restarted, took 6 steps to get here, restart -> 12 steps to continue)
        let tile_msb = Pixelfetcher::fetch_tile_byte(emu, tile_number, 1);

        for bit_idx in (0..8).rev() {
            let hb = (tile_msb >> bit_idx) & 0x1;
            let lb = (tile_lsb >> bit_idx) & 0x1;
            let color = lb | (hb << 1);

            emu.ppu.bg_fifo.push_back(GbPixel{ color, palette: 0, bg_priority: 0 });
        }

        emu.ppu.pixelfetcher.fetcher_x += 1;

        Some(1)
    }

    fn fetch_tile_number(emu: &mut Emu) -> u8 {
        let fetch_bg = true;

        let tilemap_index = emu.bus_read(REG_LCDC) & (1 << 3); // (1 << 6) for window
        let tilemap_addr = if tilemap_index == 0 {
            ADDR_TILEMAP_1
        } else {
            ADDR_TILEMAP_2
        };

        let mut x_coord: u8 = 0;
        let mut y_coord: u8 = 0;

        let ly = emu.bus_read(REG_LY);
        let scy = emu.bus_read(REG_SCY);

        let scx = emu.bus_read(REG_SCX);

        if fetch_bg {
            // not in window
            x_coord = ((scx / 8) + emu.ppu.pixelfetcher.fetcher_x) & 0x1F;
            y_coord = (ly + scy) & 0xFF; // deadcscroll overflow
        }

        let current_tile_index = (u16::from(x_coord) + 32 * (u16::from(y_coord) / 8)) & 0x3FF;
        // println!("{current_tile_index}: {y_coord} {x_coord}");

        let tilemap_data_addr = tilemap_addr + current_tile_index;

        let tile_number = emu.bus_read(tilemap_data_addr);
        tile_number
    }

    fn fetch_tile_byte(emu: &mut Emu, tile_number: u8, offset: u8) -> u8 {
        let ly = emu.bus_read(REG_LY);
        let scy = emu.bus_read(REG_SCY);
        let addressing_mode_8000 = emu.bus_read(REG_LCDC) & (1 << 4);

        let tile_byte = if addressing_mode_8000 == 1 {
            let o = u8::from(2 * ((ly + scy) % 8));
            emu.bus_read(0x8000 + u16::from(tile_number * 16 + o) + u16::from(offset))
        } else {
            let e: i8;
            unsafe { e = std::mem::transmute::<u8, i8>(tile_number); }
            let base: u16 = 0x9000;
            let o = i16::from(2 * ((ly + scy) % 8));
            emu.bus_read(base.wrapping_add_signed(e as i16 * 16 + o) + u16::from(offset))
        };

        tile_byte
    }
}