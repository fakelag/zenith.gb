use core::panic;
use std::{borrow::Borrow, cmp, collections::VecDeque, fmt::{self, Display}};

use crate::{cpu::{*}, emu::emu::Emu, util::*};

const DOTS_PER_OAM_SCAN: u16 = 80;
const DOTS_PER_VBLANK: u16 = 4560;

const REG_LCDC: u16 = 0xFF40;
const REG_STAT: u16 = 0xFF41;
const REG_LY: u16 = 0xFF44;

const REG_SCY: u16 = 0xFF42; // scroll y
const REG_SCX: u16 = 0xFF43; // scroll x

const ADDR_TILEMAP_1: u16 = 0x9800;
const ADDR_TILEMAP_2: u16 = 0x9C00;

const STAT_SELECT_LYC: u8 = 1 << 6;
const STAT_SELECT_MODE2: u8 = 1 << 5;
const STAT_SELECT_MODE1: u8 = 1 << 4;
const STAT_SELECT_MODE0: u8 = 1 << 3;

// TILE NUMBERS     $9800-$9BFF AND $9C00-$9FFF
// TILE DATA        $8000-$97FF

#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum PpuMode {
    PpuOamScan = 2,
    PpuDraw = 3,
    PpuHBlank = 0,
    PpuVBlank = 1,
}

struct GbPixel {
    pub color: u8,
    pub palette: u8, // OBP0 / OBP1 for DMG, a value between 0 and 7 on CGB
    // pub sprite_priority: u8, // CGB
    pub bg_priority: u8,
}

enum PixelfetcherStep {
    FetchTile,
    TileDataLow,
    TileDataHigh,
    PushFifo,
}

struct Pixelfetcher {
    current_step: PixelfetcherStep,
    fetcher_x: u8,
    tile_number: u8,
    tile_lsb: u8,
    tile_msb: u8,
    fresh_scanline: bool, 
}

pub struct PPU {
    mode: PpuMode,
    dots_mode: u16,
    dots_scanline: u16,
    dots_leftover: u16,

    stat_interrupt: u8,
    stat_interrupt_prev: u8,

    bg_fifo: VecDeque<GbPixel>,
    bg_scroll_count: u8,

    pixelfetcher: Pixelfetcher,
    current_x: u8,

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
            stat_interrupt: 0,
            stat_interrupt_prev: 0,
            pixelfetcher: Pixelfetcher{
                current_step: PixelfetcherStep::FetchTile,
                fetcher_x: 0,
                tile_number: 0,
                tile_lsb: 0xFF,
                tile_msb: 0xFF,
                fresh_scanline: true,
            },
            current_x: 0,
            bg_fifo: VecDeque::new(),
            bg_scroll_count: 0,
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

        // @todo better hooks and abstractions
        // @todo LY=LYC is constantly updated
        // let mut stat = emu.bus_read(REG_STAT);

        // let ly = emu.bus_read(REG_LY);
        // let lyc = emu.bus_read(REG_LYC);

        // emu.bus_write(REG_STAT, stat);

        if let Some(dots_spent) = mode_result {
            emu.ppu.dots_scanline += dots_spent;
            emu.ppu.dots_mode += dots_spent;
            dots_budget -= dots_spent;
        } else {
            emu.ppu.dots_leftover = dots_budget;
            break;
        }
    }

    // @todo - Use mode directly from REG_STAT
    let mut stat = emu.bus_read(REG_STAT);
    stat &= 0xFC;
    stat |= emu.ppu.mode as u8 & 0x3;
    emu.bus_write(REG_STAT, stat);

    handle_stat_interrupt(emu);

    return 1;
}

fn mode_oam_scan(emu: &mut Emu, dots_to_run: u16) -> Option<u16> {
    let remaining_budget = DOTS_PER_OAM_SCAN - emu.ppu.dots_mode;
    let dots = cmp::min(remaining_budget, dots_to_run);

    // println!("mode_oam_scan - remaining_budget={}, dots_to_spend={}, budget={}", remaining_budget, dots_to_spend, dots);

    if dots == 0 {
        emu.ppu.dots_mode = 0;
        emu.ppu.mode = PpuMode::PpuDraw;

        // Start of scanline, discard scx % 8 pixels
        debug_assert!(emu.ppu.current_x == 0);
        let scx = emu.bus_read(REG_SCX);
        emu.ppu.bg_scroll_count = scx % 8;

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
    debug_assert!(emu.ppu.dots_mode < 289);

    if emu.ppu.current_x == 160 {
        debug_assert!(emu.ppu.dots_mode >= 172);
        debug_assert!(emu.ppu.dots_mode <= 289);

        // reset, move to hblank
        emu.ppu.bg_fifo.clear();
        emu.ppu.pixelfetcher.reset();
        emu.ppu.bg_scroll_count = 0;
        emu.ppu.current_x = 0;
        emu.ppu.dots_mode = 0;
        emu.ppu.mode = PpuMode::PpuHBlank;
        return Some(0);
    }

    /*
        The FIFO and Pixel Fetcher work together to ensure that the FIFO always contains at least 8 pixels at any given time,
        as 8 pixels are required for the Pixel Rendering operation to take place.
        Each FIFO is manipulated only during mode 3 (pixel transfer).
        https://gbdev.io/pandocs/pixel_fifo.html#get-tile
    */

    // 1. Clock pixelfetchers
    let fetcher_run = Pixelfetcher::step(emu, dots_to_run);

    if !fetcher_run {
        return None;
    }

    let fetcher_dots = 2;

    for dot in 1..=fetcher_dots {
        if let Some(pixel) = emu.ppu.bg_fifo.pop_front() {
            if emu.ppu.bg_scroll_count > 0 {
                emu.ppu.bg_scroll_count -= 1;
            } else {
                let ly = emu.bus_read(REG_LY);

                emu.ppu.rt[ly as usize][emu.ppu.current_x as usize] = pixel.color;
                emu.ppu.current_x += 1;
            }
        }

        if emu.ppu.current_x == 160 {
            return Some(dot);
        }
    }

    // 2. Check if any pixels in BG FIFO (if not, exit)
    // 3. Check if any pixels in Sprite FIFO (if yes, merge with BG pixel)
    // 4. Sprite fetching
    // 5. Window fetching
    // 6. End scanline

    return Some(fetcher_dots);
}

fn mode_hblank(emu: &mut Emu, dots_to_run: u16) -> Option<u16> {
    let mode3_duration = emu.ppu.dots_scanline - DOTS_PER_OAM_SCAN;
    let remaining_dots = 376 - mode3_duration;
    let dots = cmp::min(remaining_dots, dots_to_run);

    if dots == 0 {
        debug_assert!(emu.ppu.dots_mode >= 87);
        debug_assert!(emu.ppu.dots_mode <= 204);
        debug_assert!(emu.ppu.dots_scanline == 456);

        emu.ppu.dots_mode = 0;
        emu.ppu.dots_scanline = 0;

        let current_ly = emu.bus_read(REG_LY);

        let ly_next = current_ly + 1;
        emu.bus_write(REG_LY, ly_next);

        emu.ppu.mode = if current_ly == 143 {
            // @todo - Sending will fail when exiting the app.
            // - A way to close emu thread and exit gracefully
            emu.frame_chan.send(emu.ppu.rt).unwrap();

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

    let ly_current = emu.bus_read(REG_LY);

    if emu.ppu.dots_mode != 0 && emu.ppu.dots_mode % 456 == 0 {
        let ly_next = ly_current + 1;
        emu.bus_write(REG_LY, ly_next);
    }

    let vblank_end = emu.ppu.dots_mode % 456 == 0 && ly_current == 153;

    if vblank_end {
        debug_assert!(dots == 0);
        debug_assert!(emu.ppu.dots_mode == DOTS_PER_VBLANK);

        emu.ppu.dots_mode = 0;
        emu.ppu.dots_scanline = 0;
        emu.ppu.mode = PpuMode::PpuOamScan;

        emu.bus_write(REG_LY, 0);

        std::thread::sleep(std::time::Duration::from_micros((16.0 * 1000.0) as u64));
        // println!("mode_vblank - NEXT FRAME {}", emu.bus_read(REG_SCX));
        return Some(0);
    }

    return Some(dots);
}

fn handle_stat_interrupt(emu: &mut Emu) {
    let stat = emu.bus_read(REG_STAT);

    emu.ppu.stat_interrupt = 0;

    if stat & STAT_SELECT_LYC != 0 {
        todo!("test this");
        emu.ppu.stat_interrupt |= if stat & (1 << 2) != 0 { STAT_SELECT_LYC } else { 0 };
    }

    let mode_bits = stat & 0x3;

    // @todo - Write in a less confusing way

    if stat & STAT_SELECT_MODE2 != 0 {
        emu.ppu.stat_interrupt |= if mode_bits == PpuMode::PpuOamScan as u8 { STAT_SELECT_MODE2 } else { 0 };
    }

    if stat & STAT_SELECT_MODE1 != 0 {
        emu.ppu.stat_interrupt |= if mode_bits == PpuMode::PpuVBlank as u8 { STAT_SELECT_MODE1 } else { 0 };
    }

    if stat & STAT_SELECT_MODE0 != 0 {
        emu.ppu.stat_interrupt |= if mode_bits == PpuMode::PpuHBlank as u8 { STAT_SELECT_MODE0 } else { 0 };
    }

    // low to high transition
    if emu.ppu.stat_interrupt_prev == 0 && emu.ppu.stat_interrupt != 0 {
        // set stat interrupt
        // println!("STAT interrupt rising edge {:#x?} -> {:#x?}", emu.ppu.stat_interrupt_prev, emu.ppu.stat_interrupt);
        let flags_if = emu.bus_read(cpu::HREG_IF);
        emu.bus_write(cpu::HREG_IF, flags_if | cpu::INTERRUPT_BIT_LCD);
    }

    emu.ppu.stat_interrupt_prev = emu.ppu.stat_interrupt;
}

impl Pixelfetcher {
    fn reset(&mut self) {
        self.current_step = PixelfetcherStep::FetchTile;
        self.fetcher_x = 0;
        self.tile_number = 0;
        self.tile_lsb = 0xFF;
        self.tile_msb = 0xFF;
        self.fresh_scanline = true;
    }

    fn step(emu: &mut Emu, dots_to_run: u16) -> bool {
        if dots_to_run < 2 {
            return false;
        }

        match emu.ppu.pixelfetcher.current_step {
            PixelfetcherStep::FetchTile => {
                emu.ppu.pixelfetcher.tile_number = Pixelfetcher::fetch_tile_number(emu);
                emu.ppu.pixelfetcher.current_step = PixelfetcherStep::TileDataLow;
            }
            PixelfetcherStep::TileDataLow => {
                emu.ppu.pixelfetcher.tile_lsb = Pixelfetcher::fetch_tile_byte(emu, emu.ppu.pixelfetcher.tile_number, 0);
                emu.ppu.pixelfetcher.current_step = PixelfetcherStep::TileDataHigh;
            }
            PixelfetcherStep::TileDataHigh => {
                if emu.ppu.pixelfetcher.fresh_scanline {
                    emu.ppu.pixelfetcher.reset();
                    emu.ppu.pixelfetcher.fresh_scanline = false;
                    return true;
                }

                emu.ppu.pixelfetcher.tile_msb = Pixelfetcher::fetch_tile_byte(emu, emu.ppu.pixelfetcher.tile_number, 1);
                emu.ppu.pixelfetcher.current_step = PixelfetcherStep::PushFifo;
            }
            PixelfetcherStep::PushFifo => {
                if emu.ppu.bg_fifo.len() > 0 {
                    todo!("push fifo should restart 2 times");
                    return true;
                }

                for bit_idx in (0..8).rev() {
                    let hb = (emu.ppu.pixelfetcher.tile_msb >> bit_idx) & 0x1;
                    let lb = (emu.ppu.pixelfetcher.tile_lsb >> bit_idx) & 0x1;
                    let color = lb | (hb << 1);

                    emu.ppu.bg_fifo.push_back(GbPixel{ color, palette: 0, bg_priority: 0 });
                }

                emu.ppu.pixelfetcher.fetcher_x += 1;
                emu.ppu.pixelfetcher.current_step = PixelfetcherStep::FetchTile;
            }
        }

        true
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
            let e: i8 = tile_number as i8;
            let base: u16 = 0x9000;
            let o = i16::from(2 * ((ly + scy) % 8));
            emu.bus_read(base.wrapping_add_signed(e as i16 * 16 + o) + u16::from(offset))
        };

        tile_byte
    }
}