use std::{cmp, collections::VecDeque, fmt::{self, Display}, sync::mpsc::SyncSender};

use crate::{cpu::*, emu::emu::FrameBuffer, mmu::mmu::MMU};

const DOTS_PER_OAM_SCAN: u16 = 80;
const DOTS_PER_VBLANK: u16 = 4560;

const REG_LY: u16 = 0xFF44;

const REG_SCY: u16 = 0xFF42; // scroll y
const REG_SCX: u16 = 0xFF43; // scroll x

const ADDR_TILEMAP_1: u16 = 0x9800;
const ADDR_TILEMAP_2: u16 = 0x9C00;

const STAT_SELECT_LYC_BIT:   u8 = 6;
const STAT_SELECT_MODE2_BIT: u8 = 5;
const STAT_SELECT_MODE1_BIT: u8 = 4;
const STAT_SELECT_MODE0_BIT: u8 = 3;
const STAT_LY_BIT: u8 = 2;

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

    pub fn step(&mut self, mmu: &mut MMU, frame_chan: &mut SyncSender<FrameBuffer>, cycles_passed: u8) -> u8 {
        let lcd_enable = mmu.lcdc().check_bit(7);

        if !lcd_enable {
            return 0;
        }
    
        debug_assert!(mmu.bus_read(REG_LY) <= 153);
    
        let mut dots_budget = u16::from(cycles_passed * 4) + self.dots_leftover;
    
        while dots_budget > 0 {
            let mode_result = match self.mode {
                PpuMode::PpuOamScan => { self.mode_oam_scan(mmu, dots_budget) }
                PpuMode::PpuDraw => { self.mode_draw(mmu, dots_budget) }
                PpuMode::PpuHBlank => { self.mode_hblank(mmu, frame_chan, dots_budget) }
                PpuMode::PpuVBlank => { self.mode_vblank(mmu, dots_budget) }
            };
    
            // @todo better hooks and abstractions
            // @todo LY=LYC is constantly updated
            // let mut stat = emu.bus_read(REG_STAT);
    
            // let ly = emu.bus_read(REG_LY);
            // let lyc = emu.bus_read(REG_LYC);
    
            // emu.bus_write(REG_STAT, stat);
    
            if let Some(dots_spent) = mode_result {
                self.dots_scanline += dots_spent;
                self.dots_mode += dots_spent;
                dots_budget -= dots_spent;
            } else {
                self.dots_leftover = dots_budget;
                break;
            }
        }
    
        // @todo - Use mode directly from REG_STAT

        mmu.stat().set_bit(0, self.mode as u8 & 0x1 != 0x0);
        mmu.stat().set_bit(1, self.mode as u8 & 0x2 != 0x0);
    
        self.handle_stat_interrupt(mmu);
    
        return 1;
    }

        
    fn mode_oam_scan(&mut self, mmu: &mut MMU, dots_to_run: u16) -> Option<u16> {
        let remaining_budget = DOTS_PER_OAM_SCAN - self.dots_mode;
        let dots = cmp::min(remaining_budget, dots_to_run);

        // println!("mode_oam_scan - remaining_budget={}, dots_to_spend={}, budget={}", remaining_budget, dots_to_spend, dots);

        if dots == 0 {
            self.dots_mode = 0;
            self.mode = PpuMode::PpuDraw;

            // Start of scanline, discard scx % 8 pixels
            debug_assert!(self.current_x == 0);
            let scx = mmu.bus_read(REG_SCX);
            self.bg_scroll_count = scx % 8;

            // println!("mode_oam_scan - switching to PpuMode::PpuDraw");
            return Some(0);
        }

        if self.dots_mode == 0 {
            /*
                @todo - OAM scanning https://hacktix.github.io/GBEDG/ppu/#timing-diagram
                New entry from OAM is checked every 8 dots, 10 objects buffered = 80 dots total
            */
        }

        return Some(dots);
    }

    fn mode_draw(&mut self, mmu: &mut MMU, dots_to_run: u16) -> Option<u16> {
        debug_assert!(self.dots_mode < 289);

        if self.current_x == 160 {
            debug_assert!(self.dots_mode >= 172);
            debug_assert!(self.dots_mode <= 289);

            // reset, move to hblank
            self.bg_fifo.clear();
            self.pixelfetcher.reset();
            self.bg_scroll_count = 0;
            self.current_x = 0;
            self.dots_mode = 0;
            self.mode = PpuMode::PpuHBlank;
            return Some(0);
        }

        /*
            The FIFO and Pixel Fetcher work together to ensure that the FIFO always contains at least 8 pixels at any given time,
            as 8 pixels are required for the Pixel Rendering operation to take place.
            Each FIFO is manipulated only during mode 3 (pixel transfer).
            https://gbdev.io/pandocs/pixel_fifo.html#get-tile
        */

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
                    let ly = mmu.bus_read(REG_LY);

                    self.rt[ly as usize][self.current_x as usize] = pixel.color;
                    self.current_x += 1;
                }
            }

            if self.current_x == 160 {
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

    fn mode_hblank(&mut self, mmu: &mut MMU, frame_chan: &mut SyncSender<FrameBuffer>, dots_to_run: u16) -> Option<u16> {
        let mode3_duration = self.dots_scanline - DOTS_PER_OAM_SCAN;
        let remaining_dots = 376 - mode3_duration;
        let dots = cmp::min(remaining_dots, dots_to_run);

        if dots == 0 {
            debug_assert!(self.dots_mode >= 87);
            debug_assert!(self.dots_mode <= 204);
            debug_assert!(self.dots_scanline == 456);

            self.dots_mode = 0;
            self.dots_scanline = 0;

            let current_ly = mmu.bus_read(REG_LY);

            let ly_next = current_ly + 1;
            mmu.bus_write(REG_LY, ly_next);

            self.mode = if current_ly == 143 {
                // @todo - Sending will fail when exiting the app.
                // - A way to close emu thread and exit gracefully
                frame_chan.send(self.rt).unwrap();

                // set vblank interrupt
                let flags_if = mmu.bus_read(cpu::HREG_IF);
                mmu.bus_write(cpu::HREG_IF, flags_if | cpu::INTERRUPT_BIT_VBLANK);

                PpuMode::PpuVBlank
            } else {
                PpuMode::PpuOamScan
            };

            // println!("mode_hblank - switching to {:?}", self.mode);
            return Some(0);
        }

        return Some(dots);
    }

    fn mode_vblank(&mut self, mmu: &mut MMU, dots_to_run: u16) -> Option<u16> {
        let remaining_budget = DOTS_PER_VBLANK - self.dots_mode;
        let dots = cmp::min(remaining_budget, dots_to_run);

        let ly_current = mmu.bus_read(REG_LY);

        if self.dots_mode != 0 && self.dots_mode % 456 == 0 {
            let ly_next = ly_current + 1;
            mmu.bus_write(REG_LY, ly_next);
        }

        let vblank_end = self.dots_mode % 456 == 0 && ly_current == 153;

        if vblank_end {
            debug_assert!(dots == 0);
            debug_assert!(self.dots_mode == DOTS_PER_VBLANK);

            self.dots_mode = 0;
            self.dots_scanline = 0;
            self.mode = PpuMode::PpuOamScan;

            mmu.bus_write(REG_LY, 0);

            std::thread::sleep(std::time::Duration::from_micros((1.0 * 1000.0) as u64));
            // println!("mode_vblank - NEXT FRAME {}", emu.bus_read(REG_SCX));
            return Some(0);
        }

        return Some(dots);
    }

    fn handle_stat_interrupt(&mut self, mmu: &mut MMU) {
        let stat = mmu.stat(); // mmu.bus_read(REG_STAT);

        self.stat_interrupt = 0;

        if stat.check_bit(STAT_SELECT_LYC_BIT) && stat.check_bit(STAT_LY_BIT) {
            self.stat_interrupt |= 1 << STAT_SELECT_LYC_BIT;
            todo!("test this");
        }

        let mode_bits = stat.get() & 0x3;

        // @todo - Write in a less confusing way

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
            // set stat interrupt
            // println!("STAT interrupt rising edge {:#x?} -> {:#x?}", self.stat_interrupt_prev, self.stat_interrupt);
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
        let tilemap_addr = if tilemap_bit == false {
            ADDR_TILEMAP_1
        } else {
            ADDR_TILEMAP_2
        };

        let mut x_coord: u8 = 0;
        let mut y_coord: u8 = 0;

        let ly = mmu.bus_read(REG_LY);
        let scy = mmu.bus_read(REG_SCY);

        let scx = mmu.bus_read(REG_SCX);

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
        let ly = mmu.bus_read(REG_LY);
        let scy = mmu.bus_read(REG_SCY);
        let addressing_mode_8000 = mmu.lcdc().check_bit(4);
        let offset: u16 = if msb { 1 } else { 0 };

        let tile_byte = if addressing_mode_8000 == false {
            let o = u8::from(2 * ((ly + scy) % 8));
            mmu.bus_read(0x8000 + u16::from(tile_number * 16 + o) + offset)
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