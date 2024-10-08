use std::fmt::{self, Display};

use crate::soc::{interrupt, soc};

pub type FrameBuffer = [[u8; 160]; 144];

const DOTS_PER_OAM_SCAN: u16 = 80;
const DOTS_PER_VBLANK: u16 = 4560;
const DOTS_PER_SCANLINE: u16 = 456;

const ADDR_TILEMAP_9800: u16 = 0x1800;
const ADDR_TILEMAP_9C00: u16 = 0x1C00;

const STAT_SELECT_LYC_BIT: u8 = 6;
const STAT_SELECT_MODE2_BIT: u8 = 5;
const STAT_SELECT_MODE1_BIT: u8 = 4;
const STAT_SELECT_MODE0_BIT: u8 = 3;

const OAM_BIT_Y_FLIP: u8 = 1 << 6;
const OAM_BIT_X_FLIP: u8 = 1 << 5;

macro_rules! read_write {
    ( $read_name:ident, $write_name:ident, $var_name:ident ) => {
        pub fn $read_name(&self) -> u8 {
            self.$var_name
        }

        pub fn $write_name(&mut self, data: u8) {
            self.$var_name = data;
        }
    };
}

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
    dots_mode: u16,
    dots_frame: u32,
    dots_leftover: u16,

    draw_length: u16,

    stat_interrupt: u8,
    stat_interrupt_prev: u8,

    vram: Box<[u8; 0x2000]>,
    oam: Box<[u8; 0xA0]>,
    oam_cursor: u8,
    sprite_buffer: Vec<Sprite>,

    // WY = LY has been true at some point during current frame
    // (checked at the start of Mode 2)
    draw_window: bool,
    window_line_counter: u16,

    fetcher_x: u16,

    bg_scanline_mask: [u8; 160],

    // @todo - 160*144=23040, allocate on the heaps
    rt: FrameBuffer,

    lcdc_enable: bool,
    lcdc_wnd_tilemap: bool,
    lcdc_wnd_enable: bool,
    lcdc_bg_wnd_tiles: bool,
    lcdc_bg_tilemap: bool,
    lcdc_obj_size: bool,
    lcdc_obj_enable: bool,
    lcdc_bg_wnd_enable: bool,

    stat_lyc_select: bool,
    stat_mode2_select: bool,
    stat_mode1_select: bool,
    stat_mode0_select: bool,
    stat_lyc_eq_ly: bool,
    stat_mode: PpuMode,

    ly: u8,
    lyc: u8,
    scy: u8,
    scx: u8,
    wx: u8,
    wy: u8,

    bgp: u8,
    obp0: u8,
    obp1: u8,
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
            vram: vec![0; 0x2000].into_boxed_slice().try_into().unwrap(),
            oam: vec![0; 0xA0].into_boxed_slice().try_into().unwrap(),
            oam_cursor: 0,
            sprite_buffer: Vec::with_capacity(10),
            rt: [[0; 160]; 144],
            lcdc_enable: true,
            lcdc_wnd_tilemap: false,
            lcdc_wnd_enable: false,
            lcdc_bg_wnd_tiles: true,
            lcdc_bg_tilemap: false,
            lcdc_obj_size: false,
            lcdc_obj_enable: false,
            lcdc_bg_wnd_enable: true,
            stat_lyc_select: false,
            stat_mode2_select: false,
            stat_mode1_select: false,
            stat_mode0_select: false,
            stat_lyc_eq_ly: true,
            stat_mode: PpuMode::PpuOamScan,
            ly: 0,
            lyc: 0,
            scy: 0,
            scx: 0,
            wx: 0,
            wy: 0,
            bgp: 0xFC,
            obp0: 0xFF,
            obp1: 0xFF,
        }
    }

    pub fn get_framebuffer(&self) -> &FrameBuffer {
        &self.rt
    }

    pub fn reset(&mut self) {
        self.ly = 0;

        self.sprite_buffer.clear();
        self.oam_cursor = 0;
        self.dots_frame = DOTS_PER_OAM_SCAN.into();
        self.dots_leftover = 0;
        self.dots_mode = 0;
        self.draw_length = 0;

        // Reset mode bits to 0 to signal that
        // its safe to write to vram/oam
        self.stat_mode = PpuMode::PpuHBlank;
    }

    pub fn clock(&mut self, ctx: &mut soc::ClockContext) {
        if !self.lcdc_enable {
            return;
        }

        debug_assert!(self.ly <= 153);

        let mut dots_budget = 4 + self.dots_leftover;

        self.dots_leftover = 0;

        while dots_budget > 0 {
            let mode_result = match self.stat_mode {
                PpuMode::PpuOamScan => self.mode_oam_scan(dots_budget),
                PpuMode::PpuDraw => self.mode_draw(dots_budget),
                PpuMode::PpuHBlank => self.mode_hblank(dots_budget),
                PpuMode::PpuVBlank => self.mode_vblank(dots_budget),
            };

            if let Some((dots_spent, mode_res)) = mode_result {
                dots_budget -= dots_spent;

                self.dots_frame += u32::from(dots_spent);
                self.dots_mode += dots_spent;

                if let Some(next_mode) = mode_res {
                    match (self.stat_mode, next_mode) {
                        (PpuMode::PpuOamScan, PpuMode::PpuDraw) => {
                            debug_assert!(self.dots_mode == DOTS_PER_OAM_SCAN);

                            if self.wy == self.ly {
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
                            debug_assert!(self.dots_frame == u32::from(self.ly) * 456);
                        }
                        (PpuMode::PpuHBlank, PpuMode::PpuVBlank) => {
                            debug_assert!(self.dots_mode >= 87);
                            debug_assert!(self.dots_mode <= 204);
                            debug_assert!(self.ly == 144);

                            self.window_line_counter = 0;
                            self.draw_window = false;

                            ctx.set_interrupt(interrupt::INTERRUPT_BIT_VBLANK);
                            ctx.set_events(soc::SocEventBits::SocEventVSync);
                        }
                        (PpuMode::PpuVBlank, PpuMode::PpuOamScan) => {
                            debug_assert!(self.dots_mode == DOTS_PER_VBLANK);

                            // @todo - These conditions should be true for any normal frame
                            // but may not be true for the first frame / when lcdc enable bit is toggled
                            // debug_assert!(self.dots_frame == 456 * 154);
                            // debug_assert!(self.ly == 154);

                            self.ly = 0;
                            self.dots_frame = 0;
                        }
                        _ => {
                            unreachable!()
                        }
                    }

                    match next_mode {
                        PpuMode::PpuOamScan => {
                            self.sprite_buffer.clear();
                            self.oam_cursor = 0;
                        }
                        _ => {}
                    }

                    self.dots_mode = 0;
                    self.stat_mode = next_mode;
                }
            } else {
                self.dots_leftover = dots_budget;
                break;
            }
        }

        self.stat_lyc_eq_ly = self.ly == self.lyc;

        if self.handle_stat_interrupt() {
            ctx.set_interrupt(interrupt::INTERRUPT_BIT_LCD);
        }
    }

    pub fn read_lcdc(&self) -> u8 {
        (self.lcdc_enable as u8) << 7
            | (self.lcdc_wnd_tilemap as u8) << 6
            | (self.lcdc_wnd_enable as u8) << 5
            | (self.lcdc_bg_wnd_tiles as u8) << 4
            | (self.lcdc_bg_tilemap as u8) << 3
            | (self.lcdc_obj_size as u8) << 2
            | (self.lcdc_obj_enable as u8) << 1
            | (self.lcdc_bg_wnd_enable as u8)
    }

    pub fn write_lcdc(&mut self, data: u8) {
        let enable_next = data & (1 << 7) != 0;

        if self.lcdc_enable && !enable_next {
            self.reset();
        }

        self.lcdc_enable = enable_next;
        self.lcdc_wnd_tilemap = data & (1 << 6) != 0;
        self.lcdc_wnd_enable = data & (1 << 5) != 0;
        self.lcdc_bg_wnd_tiles = data & (1 << 4) != 0;
        self.lcdc_bg_tilemap = data & (1 << 3) != 0;
        self.lcdc_obj_size = data & (1 << 2) != 0;
        self.lcdc_obj_enable = data & (1 << 1) != 0;
        self.lcdc_bg_wnd_enable = data & (1 << 0) != 0;
    }

    pub fn read_stat(&self) -> u8 {
        (self.stat_lyc_select as u8) << 6
            | (self.stat_mode2_select as u8) << 5
            | (self.stat_mode1_select as u8) << 4
            | (self.stat_mode0_select as u8) << 3
            | (self.stat_lyc_eq_ly as u8) << 2
            | (self.stat_mode as u8)
            | 0x80
    }

    pub fn write_stat(&mut self, data: u8) {
        self.stat_lyc_select = data & (1 << 6) != 0;
        self.stat_mode2_select = data & (1 << 5) != 0;
        self.stat_mode1_select = data & (1 << 4) != 0;
        self.stat_mode0_select = data & (1 << 3) != 0;
    }

    pub fn read_ly(&self) -> u8 {
        self.ly
    }

    pub fn write_ly(&mut self, _data: u8) {
        // noop
    }

    pub fn read_oam(&self, addr: u16) -> u8 {
        return match self.stat_mode {
            PpuMode::PpuOamScan | PpuMode::PpuDraw => 0xFF,
            _ => self.oam[(addr & 0xFF) as usize],
        };
    }

    pub fn write_oam(&mut self, addr: u16, data: u8) {
        match self.stat_mode {
            PpuMode::PpuOamScan | PpuMode::PpuDraw => {}
            _ => self.oam[(addr & 0xFF) as usize] = data,
        }
    }

    pub fn oam_dma(&mut self, addr: u16, data: u8) {
        self.oam[(addr & 0xFF) as usize] = data;
    }

    pub fn read_vram(&self, addr: u16) -> u8 {
        return match self.stat_mode {
            PpuMode::PpuDraw => 0xFF,
            _ => self.vram[(addr & 0x1FFF) as usize],
        };
    }

    pub fn write_vram(&mut self, addr: u16, data: u8) {
        match self.stat_mode {
            PpuMode::PpuDraw => {}
            _ => self.vram[(addr & 0x1FFF) as usize] = data,
        }
    }

    read_write!(read_lyc, write_lyc, lyc);
    read_write!(read_scy, write_scy, scy);
    read_write!(read_scx, write_scx, scx);
    read_write!(read_wy, write_wy, wy);
    read_write!(read_wx, write_wx, wx);
    read_write!(read_bgp, write_bgp, bgp);
    read_write!(read_obp0, write_obp0, obp0);
    read_write!(read_obp1, write_obp1, obp1);

    fn mode_oam_scan(&mut self, dots_to_run: u16) -> Option<(u16, Option<PpuMode>)> {
        let change_mode = self.dots_mode + dots_to_run >= DOTS_PER_OAM_SCAN;
        let dots = if change_mode {
            DOTS_PER_OAM_SCAN - self.dots_mode
        } else {
            dots_to_run
        };

        let num_obj_scan = dots / 2;
        let ly = self.ly;
        let obj_height: u8 = if self.lcdc_obj_size { 16 } else { 8 };

        for _ in 1..=num_obj_scan {
            debug_assert!(self.oam_cursor < 40);

            if self.sprite_buffer.len() < 10 {
                let obj_addr = (self.oam_cursor as usize) * 4;
                let x_coord = self.oam[obj_addr + 1];
                let y_coord = self.oam[obj_addr];

                if ly + 16 >= y_coord && ly + 16 < y_coord + obj_height {
                    self.sprite_buffer.push(Sprite {
                        y: y_coord,
                        x: x_coord,
                        tile: self.oam[obj_addr + 2],
                        attr: self.oam[obj_addr + 3],
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

    fn fetch_bg_tile_number(&mut self, is_window: bool) -> u8 {
        let current_tile_index: u16 = if is_window {
            let line_count = self.window_line_counter;
            (u16::from(self.fetcher_x) + 32 * (line_count / 8)) & 0x3FF
        } else {
            let x_coord = (u16::from(self.scx / 8).wrapping_add(self.fetcher_x)) & 0x1F;
            let y_coord = self.ly.wrapping_add(self.scy);
            (u16::from(x_coord) + 32 * (u16::from(y_coord) / 8)) & 0x3FF
        };

        self.fetcher_x += 1;

        let tilemap_bit = if is_window {
            self.lcdc_wnd_tilemap
        } else {
            self.lcdc_bg_tilemap
        };

        let tilemap_addr = if tilemap_bit {
            ADDR_TILEMAP_9C00
        } else {
            ADDR_TILEMAP_9800
        };

        let tilemap_data_addr = tilemap_addr + current_tile_index;

        return self.vram[tilemap_data_addr as usize];
    }

    fn fetch_sprite_tile_tuple(&self, sprite_oam: &Sprite) -> (u8, u8) {
        let ly = self.ly;
        let obj_height: u8 = if self.lcdc_obj_size { 16 } else { 8 };

        let y_with_flip = if sprite_oam.attr & OAM_BIT_Y_FLIP == 0 {
            ly.wrapping_sub(sprite_oam.y)
        } else {
            obj_height
                .wrapping_sub(ly.wrapping_sub(sprite_oam.y))
                .wrapping_sub(1)
        };

        let line_offset = u16::from((y_with_flip % obj_height) * 2);

        let tile_base = ((u16::from(sprite_oam.tile) * 16) + line_offset) as usize;

        return (self.vram[tile_base], self.vram[tile_base + 1]);
    }

    fn fetch_bg_tile_tuple(&mut self, tile_number: u8, is_window: bool) -> (u8, u8) {
        let addressing_mode_8000 = self.lcdc_bg_wnd_tiles;

        let line_offset = if is_window {
            u16::from(2 * (self.window_line_counter % 8))
        } else {
            u16::from(2 * (self.ly.wrapping_add(self.scy) % 8))
        };

        let (tile_lsb, tile_msb) = if addressing_mode_8000 {
            let tile_base = ((u16::from(tile_number) * 16) + line_offset) as usize;
            (self.vram[tile_base], self.vram[tile_base + 1])
        } else {
            let e: i8 = tile_number as i8;
            let tile_base =
                (0x1000 as u16).wrapping_add_signed(e as i16 * 16 + line_offset as i16) as usize;
            (self.vram[tile_base], self.vram[tile_base + 1])
        };

        return (tile_lsb, tile_msb);
    }

    fn draw_background(&mut self) {
        self.fetcher_x = 0;

        if !self.lcdc_bg_wnd_enable {
            for x in 0..160 {
                self.rt[self.ly as usize][x as usize] = 0;
            }
            return;
        }

        let mut skip_pixels = self.scx % 8;
        let mut x: u8 = 0;

        'outer: loop {
            let tile = self.fetch_bg_tile_number(false);
            let (tile_lsb, tile_msb) = self.fetch_bg_tile_tuple(tile, false);
            let scx_max = 8 - skip_pixels;

            for bit_idx in (0..scx_max).rev() {
                let hb = (tile_msb >> bit_idx) & 0x1;
                let lb = (tile_lsb >> bit_idx) & 0x1;
                let bg_pixel = lb | (hb << 1);

                let palette_color = (self.bgp >> (bg_pixel * 2)) & 0x3;

                self.bg_scanline_mask[x as usize] = bg_pixel;
                self.rt[self.ly as usize][x as usize] = palette_color;
                x += 1;

                if x == 160 {
                    break 'outer;
                }
            }

            skip_pixels = 0;
        }
    }

    fn draw_window(&mut self) -> Option<u8> {
        self.fetcher_x = 0;

        if !self.lcdc_wnd_enable {
            return None;
        }

        if !self.lcdc_bg_wnd_enable {
            return None;
        }

        if self.draw_window == false {
            return None;
        }

        let wx_sub7 = self.wx.saturating_sub(7);

        let mut x: u8 = wx_sub7;
        let mut skip_pixels = 7 - std::cmp::min(self.wx, 7);

        if x >= 160 {
            return None;
        }

        'outer: loop {
            let tile = self.fetch_bg_tile_number(true);
            let (tile_lsb, tile_msb) = self.fetch_bg_tile_tuple(tile, true);

            for bit_idx in (skip_pixels..8).rev() {
                let hb = (tile_msb >> bit_idx) & 0x1;
                let lb = (tile_lsb >> bit_idx) & 0x1;
                let win_pixel = lb | (hb << 1);

                let palette_color = (self.bgp >> (win_pixel * 2)) & 0x3;

                self.bg_scanline_mask[x as usize] = win_pixel;
                self.rt[self.ly as usize][x as usize] = palette_color;
                x += 1;

                if x == 160 {
                    break 'outer;
                }
            }
            skip_pixels = 0;
        }

        return Some(wx_sub7);
    }

    fn draw_sprites(&mut self) -> Option<Vec<u8>> {
        if !self.lcdc_obj_enable {
            return None;
        }

        self.sprite_buffer.reverse();
        self.sprite_buffer.sort_by(|a, b| b.x.cmp(&a.x));
        let sprites_with_tiles: Vec<SpriteWithTile> = self
            .sprite_buffer
            .iter()
            .map(|oam_entry| {
                let (tile_lsb, tile_msb) = self.fetch_sprite_tile_tuple(oam_entry);
                SpriteWithTile {
                    oam_entry: *oam_entry,
                    tile_lsb,
                    tile_msb,
                }
            })
            .collect();

        for sprite in sprites_with_tiles.iter() {
            let sprite_screen_x: i16 = i16::from(sprite.oam_entry.x) - 8;

            for bit_idx in 0..8 {
                let x: i16 = sprite_screen_x + (7 - bit_idx);

                if x >= 160 || x < 0 {
                    continue;
                }

                let x_flip = sprite.oam_entry.attr & OAM_BIT_X_FLIP != 0;

                let bit_idx_flip = if x_flip { 7 - bit_idx } else { bit_idx };

                let hb = (sprite.tile_msb >> bit_idx_flip) & 0x1;
                let lb = (sprite.tile_lsb >> bit_idx_flip) & 0x1;
                let sprite_color = lb | (hb << 1);

                if sprite_color == 0 {
                    // Color 0 is transparent for sprites
                    continue;
                }

                let sprite_palette = sprite.oam_entry.attr & (1 << 4);
                let sprite_bgpriority = sprite.oam_entry.attr & (1 << 7);

                let sprite_palette = if sprite_palette == 0 {
                    self.obp0
                } else {
                    self.obp1
                };

                if sprite_bgpriority != 0 && self.bg_scanline_mask[x as usize] != 0 {
                    // According to pandocs, sprites with higher priority sprite but bg-over-obj
                    // will "mask" lower priority sprites, and draw background over them. Copy background pixel
                    // back to the framebuffer to emulate this
                    // @todo - Check this
                    let bg_pixel = (self.bgp >> (self.bg_scanline_mask[x as usize] * 2)) & 0x3;
                    self.rt[self.ly as usize][x as usize] = bg_pixel;
                    continue;
                }

                let sprite_pixel = (sprite_palette >> (sprite_color * 2)) & 0x3;

                self.rt[self.ly as usize][x as usize] = sprite_pixel;
            }
        }

        let sprite_positions = sprites_with_tiles.iter().map(|sp| sp.oam_entry.x).collect();

        return Some(sprite_positions);
    }

    fn calc_mode3_len(&self, window_pos: Option<u8>, sprite_pos: Option<Vec<u8>>) -> u16 {
        // @todo Check timing when window and a sprite fetch overlap
        let scx_penalty = u16::from(self.scx % 8);

        let window_penalty: u16 = if window_pos.is_some() { 6 } else { 0 };

        let mut sprite_penalty: u16 = 0;
        if let Some(sprite_vec) = sprite_pos {
            let mut remaining_sprites = sprite_vec.iter().rev().copied().collect::<Vec<u8>>();

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

                // if sprite_penalty == 0 {
                //     sprite_penalty += 2;
                // }

                sprite_penalty += 6; // Sprite fetch cycles
                did_sprite_fetch = true;
            }
        }

        let mode3_length = 172 + scx_penalty + window_penalty + sprite_penalty;
        return mode3_length;
    }

    fn mode_draw(&mut self, dots_to_run: u16) -> Option<(u16, Option<PpuMode>)> {
        if self.dots_mode == 0 {
            for x in 0..160 {
                self.bg_scanline_mask[x] = 0;
            }
            self.draw_background();
            let window_pos = self.draw_window();
            let sprite_pos = self.draw_sprites();

            self.draw_length = self.calc_mode3_len(window_pos, sprite_pos);
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

    fn mode_hblank(&mut self, dots_to_run: u16) -> Option<(u16, Option<PpuMode>)> {
        let hblank_length = 376 - self.draw_length;
        debug_assert!(80 + self.draw_length + hblank_length == 456);

        if self.dots_mode + dots_to_run >= hblank_length {
            let dots = hblank_length - self.dots_mode;

            self.ly += 1;

            let next_mode = if self.ly == 144 {
                PpuMode::PpuVBlank
            } else {
                PpuMode::PpuOamScan
            };

            return Some((dots, Some(next_mode)));
        }
        Some((dots_to_run, None))
    }

    fn mode_vblank(&mut self, dots_to_run: u16) -> Option<(u16, Option<PpuMode>)> {
        let ly = self.ly;
        let current_line = (144 + ((self.dots_mode + dots_to_run) / DOTS_PER_SCANLINE))
            .try_into()
            .unwrap();

        if current_line > ly {
            self.ly = current_line;

            if current_line < 154 {
                return Some((dots_to_run, None));
            }

            let dots = DOTS_PER_SCANLINE - (self.dots_mode % DOTS_PER_SCANLINE);
            return Some((dots, Some(PpuMode::PpuOamScan)));
        }

        Some((dots_to_run, None))
    }

    fn handle_stat_interrupt(&mut self) -> bool {
        self.stat_interrupt = 0;

        if self.stat_lyc_select && self.stat_lyc_eq_ly {
            self.stat_interrupt |= 1 << STAT_SELECT_LYC_BIT;
        }

        if self.stat_mode2_select && self.stat_mode == PpuMode::PpuOamScan {
            self.stat_interrupt |= 1 << STAT_SELECT_MODE2_BIT;
        }

        if self.stat_mode1_select && self.stat_mode == PpuMode::PpuVBlank {
            self.stat_interrupt |= 1 << STAT_SELECT_MODE1_BIT;
        }

        if self.stat_mode0_select && self.stat_mode == PpuMode::PpuHBlank {
            self.stat_interrupt |= 1 << STAT_SELECT_MODE0_BIT;
        }

        let mut lcd_interrupt = false;

        // low to high transition
        if self.stat_interrupt_prev == 0 && self.stat_interrupt != 0 {
            lcd_interrupt = true;
        }

        self.stat_interrupt_prev = self.stat_interrupt;

        return lcd_interrupt;
    }
}
