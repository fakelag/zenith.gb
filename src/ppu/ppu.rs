use std::{fmt::{self, Display}, sync::mpsc::SyncSender};

use crate::{cpu::*, emu::emu::FrameBuffer, mmu::mmu::{MMU, MemoryRegion}};

const DOTS_PER_OAM_SCAN: u16 = 80;
const DOTS_PER_VBLANK: u16 = 4560;
const DOTS_PER_SCANLINE: u16 = 456;

const ADDR_TILEMAP_9800: u16 = 0x9800;
const ADDR_TILEMAP_9C00: u16 = 0x9C00;
const ADDR_OAM: u16 = 0xFE00;

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

#[derive(Debug, Copy, Clone)]
struct Sprite {
    y: u8,
    x: u8, // x can be 0 and object is not drawn
    tile: u8,
    attr: u8,
}

struct SpriteWithTile {
    oam_entry: Sprite,
    tile_lsb: u8,
    tile_msb: u8,
}

pub struct PPU {
    is_disabled: bool,
    dots_mode: u16,
    dots_frame: u32,
    dots_leftover: u16,

    draw_length: u16,

    stat_interrupt: u8,
    stat_interrupt_prev: u8,

    oam_cursor: u8,
    sprite_buffer: Vec<Sprite>,

    // WY = LY has been true at some point during current frame
    // (checked at the start of Mode 2)
    draw_window: bool,
    window_line_counter: u16,

    fetcher_x: u16,

    bg_scanline_mask: [u8; 160],

    rt: [[u8; 160]; 144],
}

impl Display for PPU {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "PPU")?;
        writeln!(f, "dots_mode={:?}", self.dots_mode)?;
        Ok(())
    }
}

impl PPU {
    pub fn new() -> Self {
        Self {
            is_disabled: false,
            draw_window: false,
            bg_scanline_mask: [0; 160],
            window_line_counter: 0,
            dots_mode: 0,
            fetcher_x: 0,
            dots_frame: 0,
            dots_leftover: 0,
            draw_length: 0,
            stat_interrupt: 0,
            stat_interrupt_prev: 0,
            oam_cursor: 0,
            sprite_buffer: Vec::with_capacity(10),
            rt: [[0; 160]; 144],
        }
    }

    fn get_mode(&mut self, mmu: &mut MMU) -> PpuMode {
        unsafe { std::mem::transmute::<u8, PpuMode>(mmu.stat().get() & 0x3) }
    }

    fn set_mode(&mut self, mmu: &mut MMU, mode: PpuMode) {
        let stat_upper_bits = mmu.stat().get() & 0xFC;
        mmu.stat().set(stat_upper_bits | (mode as u8 & 0x3));
    }

    pub fn reset(&mut self, mmu: &mut MMU) {
        mmu.unlock_region(MemoryRegion::MemRegionOAM as u8 | MemoryRegion::MemRegionVRAM as u8);
        mmu.ly().set(0);

        self.sprite_buffer.clear();
        self.oam_cursor = 0;
        self.dots_frame = 0;
        self.dots_leftover = 0;
        self.dots_mode = 0;
        self.draw_length = 0;
        self.is_disabled = true;

        self.set_mode(mmu, PpuMode::PpuOamScan);
    }

    pub fn step(&mut self, mmu: &mut MMU, frame_chan: &mut SyncSender<FrameBuffer>, cycles_passed: u8) -> bool {
        let lcd_enable = mmu.lcdc().check_bit(7);

        if !lcd_enable {
            self.reset(mmu);
            return false;
        }

        if self.is_disabled {
            self.is_disabled = false;
            mmu.lock_region(MemoryRegion::MemRegionOAM as u8);
        }
    
        debug_assert!(mmu.ly().get() <= 153);
        
        let mut dots_budget = u16::from(cycles_passed * 4) + self.dots_leftover;
        let mut exit = false;
        self.dots_leftover = 0;

        while dots_budget > 0 {
            let mode_result = match self.get_mode(mmu) {
                PpuMode::PpuOamScan => { self.mode_oam_scan(mmu, dots_budget) }
                PpuMode::PpuDraw => { self.mode_draw(mmu, dots_budget) }
                PpuMode::PpuHBlank => { self.mode_hblank(mmu, dots_budget) }
                PpuMode::PpuVBlank => { self.mode_vblank(mmu, dots_budget) }
            };
    
            if let Some((dots_spent, mode_res)) = mode_result {
                dots_budget -= dots_spent;

                self.dots_frame += u32::from(dots_spent);
                self.dots_mode += dots_spent;

                if let Some(next_mode) = mode_res {
                    match (self.get_mode(mmu), next_mode) {
                        (PpuMode::PpuOamScan, PpuMode::PpuDraw) => {
                            debug_assert!(self.dots_mode == DOTS_PER_OAM_SCAN);
                            
                            if mmu.wy().get() == mmu.ly().get() {
                                self.draw_window = true;
                            } else if self.draw_window {
                                self.window_line_counter += 1;
                            }
                        }
                        (PpuMode::PpuDraw, PpuMode::PpuHBlank) => {
                            debug_assert!(self.dots_mode >= 172);
                            debug_assert!(self.dots_mode <= 289);

                            // println!("draw length={}", self.dots_mode);

                            // reset, move to hblank
                            self.fetcher_x = 0;
                        }
                        (PpuMode::PpuHBlank, PpuMode::PpuOamScan) => {
                            debug_assert!(self.dots_frame == u32::from(mmu.ly().get()) * 456);
                        }
                        (PpuMode::PpuHBlank, PpuMode::PpuVBlank) => {
                            debug_assert!(self.dots_mode >= 87);
                            debug_assert!(self.dots_mode <= 204);
                            debug_assert!(mmu.ly().get() == 144);

                            self.window_line_counter = 0;
                            self.draw_window = false;

                            if let Err(_err) = frame_chan.send(self.rt) {
                                exit = true;
                            }

                            // set vblank interrupt
                            let flags_if = mmu.bus_read(cpu::HREG_IF);
                            mmu.bus_write(cpu::HREG_IF, flags_if | cpu::INTERRUPT_BIT_VBLANK);
                        }
                        (PpuMode::PpuVBlank, PpuMode::PpuOamScan) => {
                            debug_assert!(self.dots_mode == DOTS_PER_VBLANK);
                            debug_assert!(self.dots_frame == 456*154);
                            debug_assert!(mmu.ly().get() == 154);

                            mmu.ly().set(0);
                            self.dots_frame = 0;
                        }
                        _=> { unreachable!() }
                    }

                    match next_mode {
                        PpuMode::PpuOamScan => {
                            self.sprite_buffer.clear();
                            self.oam_cursor = 0;
                            mmu.lock_region(MemoryRegion::MemRegionOAM as u8);
                        }
                        PpuMode::PpuDraw => {
                            mmu.lock_region(MemoryRegion::MemRegionOAM as u8 | MemoryRegion::MemRegionVRAM as u8);
                        }
                        _ => {
                            mmu.unlock_region(MemoryRegion::MemRegionOAM as u8 | MemoryRegion::MemRegionVRAM as u8);
                        }
                    }

                    self.dots_mode = 0;
                    self.set_mode(mmu, next_mode);
                }
            } else {
                self.dots_leftover = dots_budget;
                break;
            }
        }

        let ly = mmu.ly().get();
        let lyc = mmu.lyc().get();
        mmu.stat().set_bit(STAT_LY_EQ_SCY_BIT, ly == lyc);

        self.handle_stat_interrupt(mmu);
    
        return exit;
    }

        
    fn mode_oam_scan(&mut self, mmu: &mut MMU, dots_to_run: u16) -> Option<(u16, Option<PpuMode>)> {
        let change_mode = self.dots_mode + dots_to_run >= DOTS_PER_OAM_SCAN;
        let dots = if change_mode {
            DOTS_PER_OAM_SCAN - self.dots_mode
        } else {
            dots_to_run
        };

        let num_obj_scan = dots / 2;
        let ly = mmu.ly().get();
        let obj_height: u8 = if mmu.lcdc().check_bit(2) { 16 } else { 8 }; 

        for _ in 1..=num_obj_scan {
            debug_assert!(self.oam_cursor < 40);

            if self.sprite_buffer.len() < 10 {
                let obj_addr = ADDR_OAM + u16::from(self.oam_cursor) * 4;
                let x_coord = mmu.bus_read(obj_addr + 1);
                let y_coord = mmu.bus_read(obj_addr);
                
                if ly + 16 >= y_coord && ly + 16 < y_coord + obj_height {
                    self.sprite_buffer.push(Sprite{
                        y: y_coord,
                        x: x_coord,
                        tile: mmu.bus_read(obj_addr + 2),
                        attr: mmu.bus_read(obj_addr + 3),
                    });
                }
            }

            self.oam_cursor += 1;
        }

        if self.dots_mode + dots_to_run >= DOTS_PER_OAM_SCAN {
            debug_assert!(self.oam_cursor == 40);
            return Some((dots, Some(PpuMode::PpuDraw)));
        }

        Some((dots, None))
    }

    fn fetch_bg_tile_number(&mut self, mmu: &mut MMU, is_window: bool) -> u8 {
        let current_tile_index: u16 = if is_window {
            let line_count = self.window_line_counter;
            (u16::from(self.fetcher_x) + 32 * (line_count / 8)) & 0x3FF
        } else {
            let ly = mmu.ly().get();
            let scy = mmu.scy().get();
            let scx = mmu.scx().get();

            let x_coord = (u16::from(scx / 8).wrapping_add(self.fetcher_x)) & 0x1F;
            let y_coord = ly.wrapping_add(scy);
            (u16::from(x_coord) + 32 * (u16::from(y_coord) / 8)) & 0x3FF
        };

        self.fetcher_x += 1;

        let tilemap_bit = if is_window { 6 } else { 3 };
        let tilemap_addr = if mmu.lcdc().check_bit(tilemap_bit) {
            ADDR_TILEMAP_9C00
        } else {
            ADDR_TILEMAP_9800
        };

        let tilemap_data_addr = tilemap_addr + current_tile_index;

        return mmu.bus_read(tilemap_data_addr);
    }

    fn fetch_sprite_tile_tuple(mmu: &mut MMU, tile_number: u8, sprite_y: u8) -> (u8, u8) {
        let ly = mmu.ly().get();

        let line_offset = u16::from(((ly.wrapping_add(sprite_y)) % 8) * 2);
        let tile_base = 0x8000 + (u16::from(tile_number) * 16) + line_offset;

        return (mmu.bus_read(tile_base), mmu.bus_read(tile_base + 1));
    }

    fn fetch_bg_tile_tuple(&mut self, mmu: &mut MMU, tile_number: u8, is_window: bool) -> (u8, u8) {
        let ly = mmu.ly().get();
        let scy = mmu.scy().get();
        
        let addressing_mode_8000 = mmu.lcdc().check_bit(4);

        let line_offset = if is_window {
            u16::from(2 * (self.window_line_counter % 8))
        } else {
            u16::from(2 * (ly.wrapping_add(scy) % 8))
        };

        let (tile_lsb, tile_msb) = if addressing_mode_8000 {
            let tile_base = 0x8000 + (u16::from(tile_number) * 16) + line_offset;
            (mmu.bus_read(tile_base), mmu.bus_read(tile_base + 1))
        } else {
            let e: i8 = tile_number as i8;
            let tile_base = (0x9000 as u16).wrapping_add_signed(e as i16 * 16 + line_offset as i16);
            (mmu.bus_read(tile_base), mmu.bus_read(tile_base + 1))
        };

        return (tile_lsb, tile_msb);
    }

    fn draw_background(&mut self, mmu: &mut MMU) {
        self.fetcher_x = 0;

        let ly: u8 = mmu.ly().get();
        let bgp = mmu.bgp().get();
        let scx = mmu.scx().get();

        if !mmu.lcdc().check_bit(0) {
            for x in 0..160 {
                self.rt[ly as usize][x as usize] = 0;
            }
            return;
        }

        let mut skip_pixels = scx % 8;
        let mut x: u8 = 0;

        'outer: loop {
            let tile = self.fetch_bg_tile_number(mmu, false);
            let (tile_lsb, tile_msb) = self.fetch_bg_tile_tuple(mmu, tile, false);

            for bit_idx in (skip_pixels..8).rev() {
                let hb = (tile_msb >> bit_idx) & 0x1;
                let lb = (tile_lsb >> bit_idx) & 0x1;
                let bg_pixel = lb | (hb << 1);

                let palette_color = (bgp >> (bg_pixel * 2)) & 0x3;

                self.bg_scanline_mask[x as usize] = bg_pixel;
                self.rt[ly as usize][x as usize] = palette_color;
                x += 1;

                if x == 160 {
                    break 'outer;
                }
            }

            skip_pixels = 0;
        }
    }

    fn draw_window(&mut self, mmu: &mut MMU) -> Option<u8> {
        self.fetcher_x = 0;

        if !mmu.lcdc().check_bit(5) {
            return None;
        }

        if !mmu.lcdc().check_bit(0) {
            return None;
        }

        if self.draw_window == false {
            return None;
        }

        let wx = mmu.wx().get();
        let wx_sub7 = wx.saturating_sub(7);
        let ly = mmu.ly().get();
        let bgp = mmu.bgp().get();

        let mut x: u8 = wx_sub7;
        let mut skip_pixels = 7 - std::cmp::min(wx, 7);

        if x >= 160 {
            return None;
        }

        'outer: loop {
            let tile = self.fetch_bg_tile_number(mmu, true);
            let (tile_lsb, tile_msb) = self.fetch_bg_tile_tuple(mmu, tile, true);

            for bit_idx in (skip_pixels..8).rev() {
                let hb = (tile_msb >> bit_idx) & 0x1;
                let lb = (tile_lsb >> bit_idx) & 0x1;
                let win_pixel = lb | (hb << 1);

                let palette_color = (bgp >> (win_pixel * 2)) & 0x3;

                self.bg_scanline_mask[x as usize] = win_pixel;
                self.rt[ly as usize][x as usize] = palette_color;
                x += 1;

                if x == 160 {
                    break 'outer;
                }
            }
            skip_pixels = 0;
        }

        return Some(wx_sub7);
    }

    fn draw_sprites(&mut self, mmu: &mut MMU) -> Option<Vec<u8>> {
        if mmu.lcdc().check_bit(1) == false {
            return None;
        }

        let ly = mmu.ly().get();
        let bgp = mmu.bgp().get();

        self.sprite_buffer.reverse();
        self.sprite_buffer.sort_by(|a, b| b.x.cmp(&a.x));
        let sprites_with_tiles: Vec<SpriteWithTile> = self.sprite_buffer
            .iter()
            .map(|oam_entry| {
                let (tile_lsb, tile_msb) = PPU::fetch_sprite_tile_tuple(mmu, oam_entry.tile, oam_entry.y);
                SpriteWithTile{ oam_entry: *oam_entry, tile_lsb, tile_msb }
            })
            .collect();

        for sprite in sprites_with_tiles.iter() {
            let skip_pixels = 8 - std::cmp::min(sprite.oam_entry.x, 8);

            for bit_idx in skip_pixels..8 {
                let hb = (sprite.tile_msb >> bit_idx) & 0x1;
                let lb = (sprite.tile_lsb >> bit_idx) & 0x1;
                let sprite_color = lb | (hb << 1);

                if sprite_color == 0 {
                    // Color 0 is transparent for sprites
                    continue;
                }

                let sprite_palette = sprite.oam_entry.attr & (1 << 4);
                let sprite_bgpriority = sprite.oam_entry.attr & (1 << 7);

                let sprite_palette = if sprite_palette == 0 {
                    mmu.obp0().get()
                } else {
                    mmu.obp1().get()
                };

                let x = sprite.oam_entry.x.saturating_sub(8) + (7-bit_idx);

                if sprite_bgpriority == 1 && self.bg_scanline_mask[x as usize] != 0 {
                    // According to pandocs, sprites with higher priority sprite but bg-over-obj
                    // will "mask" lower priority sprites, and draw background over them. Copy background pixel
                    // back to the framebuffer to emulate this
                    // @todo - Check this
                    let bg_pixel = (bgp >> (self.bg_scanline_mask[x as usize] * 2)) & 0x3;
                    self.rt[ly as usize][x as usize] = bg_pixel;
                    continue;
                }

                let sprite_pixel = (sprite_palette >> (sprite_color * 2)) & 0x3;

                self.rt[ly as usize][x as usize] = sprite_pixel;
            }
        }

        let sprite_positions = sprites_with_tiles
            .iter()
            .map(|sp| sp.oam_entry.x)
            .collect();

        return Some(sprite_positions);
    }

    fn calc_mode3_len(mmu: &mut MMU, window_pos: Option<u8>, sprite_pos: Option<Vec<u8>>) -> u16 {
        // @todo Check timing when window and a sprite fetch overlap
        let scx = mmu.scx().get();
        let scx_penalty = u16::from(scx % 8);

        let window_penalty: u16 = if window_pos.is_some() {
            6
        } else {
            0
        };
        
        let mut sprite_penalty: u16 = 0;
        if let Some(sprite_vec) = sprite_pos {
            let mut remaining_sprites = sprite_vec
                .iter()
                .rev()
                .copied()
                .collect::<Vec<u8>>();

            let mut bg_fifo_count: u8 = 8;
            
            let mut did_sprite_fetch = false;
            let mut x = 0;
            while x < 160 {
                let sprite_opt = remaining_sprites
                    .iter()
                    .enumerate()
                    .find(|(_i, sp_x)| **sp_x < x + 8);

                if sprite_opt.is_none() {
                    if did_sprite_fetch {
                        did_sprite_fetch = false;
                        sprite_penalty += u16::from((6 as u8).saturating_sub(bg_fifo_count));
                        // println!("{} remaining pixel penalty {}", x, 6 - bg_fifo_count);
                    }
                    bg_fifo_count = 8 - (x % 8);
                    x += 1;
                    continue;
                }

                let sprite = sprite_opt.unwrap();

                // println!("{} [fifo {bg_fifo_count}] - sprite idx {} at {}", x, sprite.0, sprite.1);
                remaining_sprites.remove(sprite.0);

                sprite_penalty += 6; // Sprite fetch cycles
                did_sprite_fetch = true;
            }
        }

        let mode3_length = 172 + scx_penalty + window_penalty + sprite_penalty;
        return mode3_length;
    }

    fn mode_draw(&mut self, mmu: &mut MMU, dots_to_run: u16) -> Option<(u16, Option<PpuMode>)> {
        if self.dots_mode == 0 {
            for x in 0..160 {
                self.bg_scanline_mask[x] = 0;
            }
            self.draw_background(mmu);
            let window_pos = self.draw_window(mmu);
            let sprite_pos = self.draw_sprites(mmu);

            self.draw_length = PPU::calc_mode3_len(mmu, window_pos, sprite_pos);
            return Some((1, None));
        }

        debug_assert!(self.dots_mode > 0);
        debug_assert!(self.draw_length >= 172);
        debug_assert!(self.draw_length <= 289);

        let change_mode = self.dots_mode + dots_to_run >= self.draw_length;

        let dots = if change_mode {
            self.draw_length - self.dots_mode
        } else {
            dots_to_run
        };

        if change_mode {
            return Some((dots, Some(PpuMode::PpuHBlank)));
        }

        return Some((dots, None));
    }

    fn mode_hblank(&mut self, mmu: &mut MMU, dots_to_run: u16) -> Option<(u16, Option<PpuMode>)> {
        let hblank_length = 376 - self.draw_length;

        if self.dots_mode + dots_to_run >= hblank_length {
            let dots = hblank_length - self.dots_mode;

            let completed_ly = mmu.ly().inc();

            let next_mode = if completed_ly == 143 {
                PpuMode::PpuVBlank
            } else {
                PpuMode::PpuOamScan
            };

            return Some((dots, Some(next_mode)));
        }
        Some((dots_to_run, None))
    }

    fn mode_vblank(&mut self, mmu: &mut MMU, dots_to_run: u16) -> Option<(u16, Option<PpuMode>)> {
        let ly = mmu.ly().get();
        let current_line = (144 + ((self.dots_mode + dots_to_run) / DOTS_PER_SCANLINE)).try_into().unwrap();

        if current_line > ly {
            mmu.ly().set(current_line);

            if current_line < 154 {
                return Some((dots_to_run, None));
            }

            let dots = DOTS_PER_SCANLINE - (self.dots_mode % DOTS_PER_SCANLINE);
            return Some((dots, Some(PpuMode::PpuOamScan)));
        }

        Some((dots_to_run, None))
    }

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
