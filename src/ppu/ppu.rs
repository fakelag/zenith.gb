use core::panic;
use std::{cmp, collections::VecDeque, fmt::{self, Display}, sync::mpsc::SyncSender};

use crate::{cpu::*, emu::emu::FrameBuffer, mmu::mmu::MMU};

const DOTS_PER_OAM_SCAN: u16 = 80;
const DOTS_PER_VBLANK: u16 = 4560;
const DOTS_PER_SCANLINE: u16 = 456;

const ADDR_TILEMAP_9800: u16 = 0x9800;
const ADDR_TILEMAP_9C00: u16 = 0x9C00;

const STAT_SELECT_LYC_BIT:   u8 = 6;
const STAT_SELECT_MODE2_BIT: u8 = 5;
const STAT_SELECT_MODE1_BIT: u8 = 4;
const STAT_SELECT_MODE0_BIT: u8 = 3;
const STAT_LY_EQ_SCY_BIT: u8 = 2;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PpuMode {
    PpuOamScan = 2,
    PpuDraw = 3,
    PpuHBlank = 0,
    PpuVBlank = 1,
}

struct GbPixel {
    pub color: u8,
    // pub palette: u8, // OBP0 / OBP1 for DMG, a value between 0 and 7 on CGB
    // pub sprite_priority: u8, // CGB
    // pub bg_priority: u8,
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
    dots_frame: u32,
    dots_leftover: u16,

    hblank_length: u16,

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
        Ok(())
    }
}

impl PPU {
    pub fn new() -> Self {
        Self {
            mode: PpuMode::PpuOamScan,
            dots_mode: 0,
            dots_frame: 0,
            dots_leftover: 0,
            hblank_length: 0,
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

    pub fn step(&mut self, mmu: &mut MMU, frame_chan: &mut SyncSender<FrameBuffer>, cycles_passed: u8) -> u8 {
        let lcd_enable = mmu.lcdc().check_bit(7);

        if !lcd_enable {
            // println!("doneone disabled lcd");
            mmu.ly().set(0);

            self.bg_fifo.clear();
            self.pixelfetcher.reset();
            self.dots_frame = 0;
            self.dots_leftover = 0;
            self.dots_mode = 0;
            self.bg_scroll_count = 0;
            self.current_x = 0;
            self.hblank_length = 0;
            self.mode = PpuMode::PpuOamScan;

            mmu.stat().set_bit(0, self.mode as u8 & 0x1 != 0x0);
            mmu.stat().set_bit(1, self.mode as u8 & 0x2 != 0x0);
            return 0;
        }
    
        debug_assert!(mmu.ly().get() <= 153);
    
        let mut dots_budget = u16::from(cycles_passed * 4) + self.dots_leftover;
        self.dots_leftover = 0;
    
        while dots_budget > 0 {
            let m = self.mode;

            let mode_result = match self.mode {
                PpuMode::PpuOamScan => { self.mode_oam_scan(mmu, dots_budget) }
                PpuMode::PpuDraw => { self.mode_draw(mmu, dots_budget) }
                PpuMode::PpuHBlank => { self.mode_hblank(mmu, frame_chan, dots_budget) }
                PpuMode::PpuVBlank => { self.mode_vblank(mmu, dots_budget) }
            };
    
            if let Some(dots_spent) = mode_result {
                dots_budget -= dots_spent;
                self.dots_frame += u32::from(dots_spent);

                if m != self.mode && self.mode == PpuMode::PpuVBlank {
                    println!("ENTER VBLANK DOTS={}", self.dots_frame);
                }

                if m == PpuMode::PpuVBlank && self.mode == PpuMode::PpuOamScan {
                    self.dots_frame = 0;
                }

            } else {
                self.dots_leftover = dots_budget;
                break;
            }
        }
    
        // @todo - Use mode directly from REG_STAT

        mmu.stat().set_bit(0, self.mode as u8 & 0x1 != 0x0);
        mmu.stat().set_bit(1, self.mode as u8 & 0x2 != 0x0);

        let ly = mmu.ly().get();
        let lyc = mmu.lyc().get();
        mmu.stat().set_bit(STAT_LY_EQ_SCY_BIT, ly == lyc);

        self.handle_stat_interrupt(mmu);
    
        return 1;
    }

        
    fn mode_oam_scan(&mut self, mmu: &mut MMU, dots_to_run: u16) -> Option<u16> {
        if self.dots_mode == 0 {
            // @todo oam scan
        }
        
        if self.dots_mode + dots_to_run >= DOTS_PER_OAM_SCAN {
            let dots = DOTS_PER_OAM_SCAN - self.dots_mode;

            debug_assert!(self.dots_mode + dots == DOTS_PER_OAM_SCAN);
            self.dots_mode = 0;
            self.mode = PpuMode::PpuDraw;
    
            // Start of scanline, discard scx % 8 pixels
            debug_assert!(self.current_x == 0);
            let scx = mmu.scx().get();
            self.bg_scroll_count = scx % 8;

            return Some(dots);
        }

        self.dots_mode += dots_to_run;
        Some(dots_to_run)
    }

    fn mode_draw(&mut self, mmu: &mut MMU, dots_to_run: u16) -> Option<u16> {
        debug_assert!(self.dots_mode < 289);

        /*
            The FIFO and Pixel Fetcher work together to ensure that the FIFO always contains at least 8 pixels at any given time,
            as 8 pixels are required for the Pixel Rendering operation to take place.
            Each FIFO is manipulated only during mode 3 (pixel transfer).
            https://gbdev.io/pandocs/pixel_fifo.html#get-tile
        */
        if self.current_x == 160 {
            debug_assert!(self.dots_mode >= 172);
            debug_assert!(self.dots_mode <= 289);

            // println!("mode_draw - switching to PpuMode::PpuHBlank - dots total {}", self.dots_mode);

            // reset, move to hblank
            self.hblank_length = 376 - self.dots_mode;
            self.bg_fifo.clear();
            self.pixelfetcher.reset();
            self.bg_scroll_count = 0;
            self.current_x = 0;
            self.dots_mode = 0;
            self.mode = PpuMode::PpuHBlank;
            return Some(0);
        }

        // 1. Clock pixelfetchers
        let fetcher_run = self.pixelfetcher.step(mmu, &mut self.bg_fifo, dots_to_run);

        if !fetcher_run {
            return None;
        }

        let fetcher_dots = 2;

        for dot in 1..=fetcher_dots {
            if let Some(pixel) = self.bg_fifo.pop_front() {
                if self.bg_scroll_count > 0 {
                    self.bg_scroll_count -= 1;
                } else {
                    let ly = mmu.ly().get();

                    self.rt[ly as usize][self.current_x as usize] = pixel.color;
                    self.current_x += 1;
                }
            }

            if self.current_x == 160 {
                self.dots_mode += dot;
                return Some(dot);
            }
        }

        // 2. Check if any pixels in BG FIFO (if not, exit)
        // 3. Check if any pixels in Sprite FIFO (if yes, merge with BG pixel)
        // 4. Sprite fetching
        // 5. Window fetching
        // 6. End scanline

        self.dots_mode += fetcher_dots;
        return Some(fetcher_dots);
    }

    fn mode_hblank(&mut self, mmu: &mut MMU, frame_chan: &mut SyncSender<FrameBuffer>, dots_to_run: u16) -> Option<u16> {
        if self.dots_mode + dots_to_run >= self.hblank_length {
            let dots = self.hblank_length - self.dots_mode;

            debug_assert!(self.dots_mode + dots >= 87);
            debug_assert!(self.dots_mode + dots <= 204);

            let completed_ly = mmu.ly().inc();

            self.mode = if completed_ly == 143 {
                // @todo - Sending will fail when exiting the app.
                // - A way to close emu thread and exit gracefully
                frame_chan.send(self.rt).unwrap();

                // println!("mode_hblank - switching to PpuMode::PpuVBlank - dots total {} {}", self.dots_mode, dots);

                // set vblank interrupt
                let flags_if = mmu.bus_read(cpu::HREG_IF);
                mmu.bus_write(cpu::HREG_IF, flags_if | cpu::INTERRUPT_BIT_VBLANK);

                PpuMode::PpuVBlank
            } else {
                PpuMode::PpuOamScan
            };

            self.dots_mode = 0;
            return Some(dots);
        }

        self.dots_mode += dots_to_run;
        Some(dots_to_run)
    }

    fn mode_vblank(&mut self, mmu: &mut MMU, dots_to_run: u16) -> Option<u16> {
        let ly = mmu.ly().get();
        let current_line = (144 + ((self.dots_mode + dots_to_run) / DOTS_PER_SCANLINE)).try_into().unwrap();

        // println!("mode_vblank - ly={} curline {current_line} dots_mode={}, dots_to_run={}", ly, self.dots_mode, dots_to_run);

        if current_line > ly {
            if current_line < 154 {
                debug_assert!(current_line == ly + 1);

                mmu.ly().set(current_line);
                self.dots_mode += dots_to_run;
                return Some(dots_to_run);
            }

            let dots = DOTS_PER_SCANLINE - (self.dots_mode % DOTS_PER_SCANLINE);

            println!("dots_frame={}", self.dots_frame);
            println!("dots={}", dots);

            println!("mode_vblank - switching to PpuMode::PpuOamScan - dots total {} -f {}",
                self.dots_mode + dots, self.dots_frame + u32::from(dots));

            debug_assert!(current_line == 154);
            debug_assert!(dots <= dots_to_run);
            debug_assert!(self.dots_mode + dots == DOTS_PER_VBLANK);
            debug_assert!(self.dots_frame + u32::from(dots) == 456*154);

            self.dots_frame = 0;
            self.dots_mode = 0;
            self.mode = PpuMode::PpuOamScan;

            mmu.ly().set(0);

            std::thread::sleep(std::time::Duration::from_millis(12 as u64));

            return Some(dots);
        }

        self.dots_mode += dots_to_run;
        Some(dots_to_run)
    }

    fn count_dots(&mut self, dots_spent: u16) {
        self.dots_frame += u32::from(dots_spent);
        self.dots_mode += dots_spent;
    }

    // fn change_mode(&mut self) {

    // }

    fn handle_stat_interrupt(&mut self, mmu: &mut MMU) {
        let stat = mmu.stat();

        self.stat_interrupt = 0;

        if stat.check_bit(STAT_SELECT_LYC_BIT) && stat.check_bit(STAT_LY_EQ_SCY_BIT) {
            self.stat_interrupt |= 1 << STAT_SELECT_LYC_BIT;
        }

        let mode_bits = stat.get() & 0x3;

        if stat.check_bit(STAT_SELECT_MODE2_BIT) && mode_bits == PpuMode::PpuOamScan as u8 {
            self.stat_interrupt |= 1 << STAT_SELECT_MODE2_BIT;
        }

        if stat.check_bit(STAT_SELECT_MODE1_BIT) && mode_bits == PpuMode::PpuVBlank as u8 {
            self.stat_interrupt |= 1 << STAT_SELECT_MODE1_BIT;
        }
        
        if stat.check_bit(STAT_SELECT_MODE0_BIT) && mode_bits == PpuMode::PpuHBlank as u8 {
            self.stat_interrupt |= 1 << STAT_SELECT_MODE0_BIT;
        }

        // low to high transition
        if self.stat_interrupt_prev == 0 && self.stat_interrupt != 0 {
            let flags_if = mmu.bus_read(cpu::HREG_IF);
            mmu.bus_write(cpu::HREG_IF, flags_if | cpu::INTERRUPT_BIT_LCD);
        }

        self.stat_interrupt_prev = self.stat_interrupt;
    }

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

    fn step(&mut self, mmu: &mut MMU, bg_fifo: &mut VecDeque<GbPixel>, dots_to_run: u16) -> bool {
        if dots_to_run < 2 {
            return false;
        }

        match self.current_step {
            PixelfetcherStep::FetchTile => {
                self.fetch_tile_number(mmu);
                self.current_step = PixelfetcherStep::TileDataLow;
            }
            PixelfetcherStep::TileDataLow => {
                self.fetch_tile_byte(mmu, self.tile_number, false);
                self.current_step = PixelfetcherStep::TileDataHigh;
            }
            PixelfetcherStep::TileDataHigh => {
                if self.fresh_scanline {
                    self.reset();
                    self.fresh_scanline = false;
                    return true;
                }

                self.fetch_tile_byte(mmu, self.tile_number, true);
                self.current_step = PixelfetcherStep::PushFifo;
            }
            PixelfetcherStep::PushFifo => {
                if bg_fifo.len() > 0 {
                    todo!("push fifo should restart 2 times");
                    return true;
                }

                for bit_idx in (0..8).rev() {
                    let hb = (self.tile_msb >> bit_idx) & 0x1;
                    let lb = (self.tile_lsb >> bit_idx) & 0x1;
                    let color = lb | (hb << 1);

                    bg_fifo.push_back(GbPixel{ color, }); // palette: 0, bg_priority: 0 });
                }

                self.fetcher_x += 1;
                self.current_step = PixelfetcherStep::FetchTile;
            }
        }

        true
    }

    fn fetch_tile_number(&mut self, mmu: &mut MMU) {
        let fetch_bg = true;

        let tilemap_bit = mmu.lcdc().check_bit(3); // check_bit(6) for window
        let tilemap_addr = if tilemap_bit {
            ADDR_TILEMAP_9C00
        } else {
            ADDR_TILEMAP_9800
        };

        let mut x_coord: u8 = 0;
        let mut y_coord: u8 = 0;

        let ly = mmu.ly().get();
        let scy = mmu.scy().get();
        let scx = mmu.scx().get();

        if fetch_bg {
            // not in window
            x_coord = ((scx / 8) + self.fetcher_x) & 0x1F;
            y_coord = (ly + scy) & 0xFF;
        }

        let current_tile_index = (u16::from(x_coord) + 32 * (u16::from(y_coord) / 8)) & 0x3FF;

        let tilemap_data_addr = tilemap_addr + current_tile_index;

        self.tile_number = mmu.bus_read(tilemap_data_addr);
    }

    fn fetch_tile_byte(&mut self, mmu: &mut MMU, tile_number: u8, msb: bool) {
        let ly = mmu.ly().get();
        let scy = mmu.scy().get();
        let addressing_mode_8000 = mmu.lcdc().check_bit(4);
        let offset: u16 = if msb { 1 } else { 0 };

        // println!("lcdc.4={addressing_mode_8000}");

        let tile_byte = if addressing_mode_8000 {
            let o = u16::from(2 * ((ly + scy) % 8));
            mmu.bus_read(0x8000 + (u16::from(tile_number) * 16) + o + offset)
        } else {
            let e: i8 = tile_number as i8;
            let base: u16 = 0x9000;
            let o = i16::from(2 * ((ly + scy) % 8));
            mmu.bus_read(base.wrapping_add_signed(e as i16 * 16 + o) + offset)
        };

        if msb {
            self.tile_msb = tile_byte;
        } else {
            self.tile_lsb = tile_byte;
        }
    }
}