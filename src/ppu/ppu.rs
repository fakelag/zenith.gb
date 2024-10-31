use std::{
    fmt::{self, Display},
    sync::mpsc::TrySendError,
};

use crate::{
    soc::{
        interrupt,
        soc::{self, ClockContext},
    },
    GbCtx,
};

pub type FrameBuffer = [[u8; 160]; 144];
pub type PpuFrameSender = std::sync::mpsc::SyncSender<FrameBuffer>;

const CYCLES_PER_OAM_SCAN: u16 = 20;
const CYCLES_PER_DRAW: u16 = 43;
const CYCLES_PER_VBLANK_LINE: u16 = 114;

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

        pub fn $write_name(&mut self, data: u8, ctx: &mut soc::ClockContext) {
            self.clock(ctx);
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

pub struct PPU {
    cycles_mode: u16,
    cycles_frame: u32,

    // @todo - Benchmark runtime cost of Rc. Some state (such as .cgb)
    // could be replicated for performance
    ctx: std::rc::Rc<GbCtx>,

    draw_length: u16,
    stat_interrupt: bool,

    vram_0: Box<[u8; 0x2000]>,
    vram_1: Box<[u8; 0x2000]>,
    oam: Box<[u8; 0xA0]>,
    sprite_buffer: Vec<Sprite>,

    // WY = LY has been true at some point during current frame
    // (checked at the start of Mode 2)
    draw_window: bool,
    window_line_counter: u16,
    fetcher_x: u16,
    bg_scanline_mask: [u8; 160],

    // @todo - 160*144=23040, allocate on the heaps
    rt: FrameBuffer,
    frame_chan: Option<PpuFrameSender>,

    sync_video: bool,

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

    // CGB palette memory
    cgb_palettes: [u8; 0x40],

    // CGB registers
    vbk: bool,

    /*
        This register is used to address a byte in the CGB’s background palette RAM.
        Since there are 8 palettes, 8 palettes × 4 colors/palette × 2 bytes/color = 64 bytes can be addressed.
        First comes BGP0 color number 0, then BGP0 color number 1, BGP0 color number 2, BGP0 color number 3, BGP1 color number 0, and so on.
        Thus, address $03 allows accessing the second (upper) byte of BGP0 color #1 via BCPD, which contains the color’s blue and upper green bits.
        https://gbdev.io/pandocs/Palettes.html#ff68--bcpsbgpi-cgb-mode-only-background-color-palette-specification--background-palette-index
    */
    bcps: u8,
    ocps: u8,
}

impl Display for PPU {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "PPU")?;
        Ok(())
    }
}

impl PPU {
    pub fn new(
        frame_chan: Option<PpuFrameSender>,
        sync_video: bool,
        ctx: std::rc::Rc<GbCtx>,
    ) -> Self {
        Self {
            ctx,
            frame_chan,
            sync_video,
            draw_window: false,
            bg_scanline_mask: [0; 160],
            window_line_counter: 0,
            fetcher_x: 0,
            cycles_mode: CYCLES_PER_OAM_SCAN,
            cycles_frame: 0,
            draw_length: 0,
            stat_interrupt: false,
            vbk: false,
            vram_0: vec![0; 0x2000].into_boxed_slice().try_into().unwrap(),
            vram_1: vec![0; 0x2000].into_boxed_slice().try_into().unwrap(),
            oam: vec![0; 0xA0].into_boxed_slice().try_into().unwrap(),
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
            cgb_palettes: [0; 0x40],
            bcps: 0x88,
            ocps: 0x90,
        }
    }

    pub fn get_framebuffer(&self) -> &FrameBuffer {
        &self.rt
    }

    pub fn reset(&mut self) {
        self.ly = 0;

        self.sprite_buffer.clear();
        self.cycles_frame = 0;
        self.cycles_mode = CYCLES_PER_OAM_SCAN;
        self.draw_length = 0;

        // Reset mode bits to 0 to signal that
        // its safe to write to vram/oam
        self.stat_mode = PpuMode::PpuHBlank;
    }

    // vsync can have a big stack frame due to copying over framebuffer
    // never inline to avoid paying stack probes unless necessary
    #[inline(never)]
    pub fn vsync(&self) -> bool {
        if let Some(frame_chan) = &self.frame_chan {
            if self.sync_video {
                match frame_chan.send(self.rt) {
                    Ok(_) => {}
                    Err(_err) => {
                        return true;
                    }
                }
            } else {
                match frame_chan.try_send(self.rt) {
                    Ok(_) | Err(TrySendError::Full(_)) => {}
                    Err(TrySendError::Disconnected(_)) => {
                        return true;
                    }
                }
            }
        }
        return false;
    }

    pub fn clock(&mut self, ctx: &mut soc::ClockContext) {
        if !self.lcdc_enable {
            return;
        }

        debug_assert!(self.ly <= 153);

        self.cycles_frame += 1;
        self.cycles_mode -= 1;

        if self.cycles_mode > 0 {
            return;
        }

        self.update_mode(ctx);

        self.handle_stat_interrupt(ctx);

        debug_assert!(self.cycles_mode != 0);
        debug_assert!(self.cycles_frame == 0);
    }

    fn update_mode(&mut self, ctx: &mut soc::ClockContext) {
        match self.stat_mode {
            PpuMode::PpuOamScan => {
                if self.wy == self.ly {
                    self.draw_window = true;
                } else if self.draw_window {
                    self.window_line_counter += 1;
                }

                self.cycles_mode = self.mode_draw();
                self.draw_length = self.cycles_mode;
                self.stat_mode = PpuMode::PpuDraw;
            }
            PpuMode::PpuDraw => {
                self.fetcher_x = 0;
                self.cycles_mode = 94 - self.draw_length;
                self.stat_mode = PpuMode::PpuHBlank;
            }
            PpuMode::PpuHBlank => {
                self.ly += 1;
                self.update_lyc_eq_ly();

                self.stat_mode = if self.ly >= 144 {
                    if self.vsync() {
                        ctx.set_events(soc::SocEventBits::SocEventsVSyncAndExit);
                    }

                    ctx.set_interrupt(interrupt::INTERRUPT_BIT_VBLANK);
                    ctx.set_events(soc::SocEventBits::SocEventVSync);

                    self.draw_window = false;
                    self.window_line_counter = 0;

                    self.cycles_mode = CYCLES_PER_VBLANK_LINE;
                    PpuMode::PpuVBlank
                } else {
                    self.mode_oam_scan();
                    self.cycles_mode = CYCLES_PER_OAM_SCAN;
                    PpuMode::PpuOamScan
                };
            }
            PpuMode::PpuVBlank => {
                self.ly += 1;

                if self.ly >= 154 {
                    self.ly = 0;
                    self.mode_oam_scan();
                    self.stat_mode = PpuMode::PpuOamScan;
                    self.cycles_mode = CYCLES_PER_OAM_SCAN;
                } else {
                    self.cycles_mode = CYCLES_PER_VBLANK_LINE;
                }

                self.update_lyc_eq_ly();
            }
        }

        self.cycles_frame = 0;
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

    pub fn clock_write_lcdc(&mut self, data: u8, ctx: &mut ClockContext) {
        self.clock(ctx);

        let enable_next = data & (1 << 7) != 0;

        if self.lcdc_enable && !enable_next {
            self.reset();
            self.handle_stat_interrupt(ctx);
        } else if !self.lcdc_enable && enable_next {
            self.update_lyc_eq_ly();
            self.handle_stat_interrupt(ctx);
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

    pub fn clock_write_stat(&mut self, data: u8, ctx: &mut ClockContext) {
        self.clock(ctx);

        self.stat_lyc_select = data & (1 << 6) != 0;
        self.stat_mode2_select = data & (1 << 5) != 0;
        self.stat_mode1_select = data & (1 << 4) != 0;
        self.stat_mode0_select = data & (1 << 3) != 0;

        self.handle_stat_interrupt(ctx);
    }

    pub fn read_ly(&self) -> u8 {
        self.ly
    }

    pub fn clock_write_ly(&mut self, _data: u8, ctx: &mut ClockContext) {
        // noop
        // self.ly = 0;
        self.clock(ctx);
    }

    pub fn read_lyc(&self) -> u8 {
        self.lyc
    }

    pub fn clock_write_lyc(&mut self, data: u8, ctx: &mut ClockContext) {
        self.clock(ctx);

        self.lyc = data;
        if self.lcdc_enable {
            self.update_lyc_eq_ly();
            self.handle_stat_interrupt(ctx);
        }
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
            _ => self.read_vram_banked(addr & 0x1FFF),
        };
    }

    pub fn write_vram(&mut self, addr: u16, data: u8) {
        match self.stat_mode {
            PpuMode::PpuDraw => {}
            _ => {
                let vram_addr = (addr & 0x1FFF) as usize;
                if self.vbk {
                    self.vram_1[vram_addr] = data;
                } else {
                    self.vram_0[vram_addr] = data;
                }
            }
        }
    }

    pub fn read_vbk(&self) -> u8 {
        if self.ctx.cgb {
            (self.vbk as u8) | 0xFE
        } else {
            0xFF
        }
    }

    pub fn clock_write_vbk(&mut self, data: u8, ctx: &mut ClockContext) {
        self.clock(ctx);

        if self.ctx.cgb {
            self.vbk = data & 0x1 != 0;
        }
    }

    pub fn read_bcps(&self) -> u8 {
        if self.ctx.cgb {
            self.bcps | 0x40
        } else {
            0xFF
        }
    }

    pub fn clock_write_bcps(&mut self, data: u8, ctx: &mut ClockContext) {
        self.clock(ctx);

        if self.ctx.cgb {
            self.bcps = data & 0xBF;
        }
    }

    pub fn read_ocps(&self) -> u8 {
        if self.ctx.cgb {
            self.ocps | 0x40
        } else {
            0xFF
        }
    }

    pub fn clock_write_ocps(&mut self, data: u8, ctx: &mut ClockContext) {
        self.clock(ctx);

        if self.ctx.cgb {
            self.ocps = data & 0xBF;
        }
    }

    read_write!(read_scy, clock_write_scy, scy);
    read_write!(read_scx, clock_write_scx, scx);
    read_write!(read_wy, clock_write_wy, wy);
    read_write!(read_wx, clock_write_wx, wx);
    read_write!(read_bgp, clock_write_bgp, bgp);
    read_write!(read_obp0, clock_write_obp0, obp0);
    read_write!(read_obp1, clock_write_obp1, obp1);

    fn read_vram_banked(&self, addr: u16) -> u8 {
        if self.vbk {
            self.vram_1[addr as usize]
        } else {
            self.vram_0[addr as usize]
        }
    }

    fn update_lyc_eq_ly(&mut self) {
        self.stat_lyc_eq_ly = self.ly == self.lyc;
    }

    fn mode_oam_scan(&mut self) {
        let ly = self.ly;
        let obj_height: u8 = if self.lcdc_obj_size { 16 } else { 8 };

        self.sprite_buffer.clear();

        for oam_cursor in 0..40 {
            let obj_addr = oam_cursor * 4;
            let x_coord = self.oam[obj_addr + 1];
            let y_coord = self.oam[obj_addr];

            if ly + 16 >= y_coord && ly + 16 < y_coord + obj_height {
                self.sprite_buffer.push(Sprite {
                    y: y_coord,
                    x: x_coord,
                    tile: self.oam[obj_addr + 2],
                    attr: self.oam[obj_addr + 3],
                });

                if self.sprite_buffer.len() >= 10 {
                    break;
                }
            }
        }

        self.sprite_buffer.reverse();
        self.sprite_buffer.sort_by(|a, b| b.x.cmp(&a.x));
    }

    fn fetch_bg_tile_number(&mut self, is_window: bool) -> (u8, u8) {
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

        // @todo CGB ONLY STUF
        let bg_attr = self.vram_1[tilemap_data_addr as usize];

        return (bg_attr, self.vram_0[tilemap_data_addr as usize]);
    }

    fn fetch_sprite_tile_tuple(&self, sprite_oam: &Sprite) -> (u8, u8) {
        let ly = self.ly;
        let obj_mask: u8 = if self.lcdc_obj_size { 0xF } else { 0x7 };

        let y_with_flip = if sprite_oam.attr & OAM_BIT_Y_FLIP == 0 {
            ly.wrapping_sub(sprite_oam.y)
        } else {
            let obj_height = obj_mask + 1;
            obj_height
                .wrapping_sub(ly.wrapping_sub(sprite_oam.y))
                .wrapping_sub(1)
        };

        let line_offset = u16::from((y_with_flip & obj_mask) * 2);

        let tile_base = (u16::from(sprite_oam.tile) * 16) + line_offset;
        return (
            self.read_vram_banked(tile_base),
            self.read_vram_banked(tile_base + 1),
        );
    }

    fn fetch_bg_tile_tuple(&mut self, tile_number: u8, is_window: bool) -> (u8, u8) {
        let addressing_mode_8000 = self.lcdc_bg_wnd_tiles;

        let line_offset = if is_window {
            u16::from(2 * (self.window_line_counter & 0x7))
        } else {
            u16::from(2 * (self.ly.wrapping_add(self.scy) & 0x7))
        };

        let (tile_lsb, tile_msb) = if addressing_mode_8000 {
            let tile_base = (u16::from(tile_number) * 16) + line_offset;
            (
                self.read_vram_banked(tile_base),
                self.read_vram_banked(tile_base + 1),
            )
        } else {
            let e: i8 = tile_number as i8;
            let tile_base = (0x1000 as u16).wrapping_add_signed(e as i16 * 16 + line_offset as i16);
            (
                self.read_vram_banked(tile_base),
                self.read_vram_banked(tile_base + 1),
            )
        };

        return (tile_lsb, tile_msb);
    }

    fn draw_background(&mut self) {
        self.fetcher_x = 0;

        if !self.lcdc_bg_wnd_enable {
            for x in 0..160 {
                self.bg_scanline_mask[x] = 0;
                self.rt[self.ly as usize][x as usize] = 0;
            }
            return;
        }

        let skip_pixels: usize = (self.scx & 0x7) as usize;
        let mut scx_max = 8 - skip_pixels;
        let ly = self.ly as usize;

        let mut x: usize = 0;

        assert!(ly < 144);

        'outer: loop {
            let (attrs, tile) = self.fetch_bg_tile_number(false);

            // Hack to use correct bank for bg tiles
            let vbk_backup = self.vbk;
            self.vbk = attrs & 0x8 != 0;

            let (tile_lsb, tile_msb) = self.fetch_bg_tile_tuple(tile, false);

            self.vbk = vbk_backup;

            let rt_scanline = &mut self.rt[ly];

            assert!(x < 160);

            for bit_idx in (0..scx_max).rev() {
                let hb = (tile_msb >> bit_idx) & 0x1;
                let lb = (tile_lsb >> bit_idx) & 0x1;
                let bg_pixel = lb | (hb << 1);

                let palette_color = (self.bgp >> (bg_pixel * 2)) & 0x3;

                self.bg_scanline_mask[x] = bg_pixel;
                rt_scanline[x] = palette_color;
                x += 1;

                if x == 160 {
                    break 'outer;
                }
            }

            scx_max = 8;
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

        let mut x = wx_sub7 as usize;
        let mut skip_pixels = 7 - std::cmp::min(self.wx, 7);

        if x >= 160 {
            return None;
        }

        let ly = self.ly as usize;
        assert!(ly < 144);

        'outer: loop {
            let (_attrs, tile) = self.fetch_bg_tile_number(true); // @TODO CGB
            let (tile_lsb, tile_msb) = self.fetch_bg_tile_tuple(tile, true);

            let rt_scanline = &mut self.rt[ly];

            assert!(x < 160);

            for bit_idx in (skip_pixels..8).rev() {
                let hb = (tile_msb >> bit_idx) & 0x1;
                let lb = (tile_lsb >> bit_idx) & 0x1;
                let win_pixel = lb | (hb << 1);

                let palette_color = (self.bgp >> (win_pixel * 2)) & 0x3;

                self.bg_scanline_mask[x] = win_pixel;
                rt_scanline[x] = palette_color;
                x += 1;

                if x == 160 {
                    break 'outer;
                }
            }
            skip_pixels = 0;
        }

        return Some(wx_sub7);
    }

    fn draw_sprites(&mut self) {
        if !self.lcdc_obj_enable {
            return;
        }

        for sprite in self.sprite_buffer.iter() {
            let sprite_screen_x: i16 = i16::from(sprite.x) - 8;

            for bit_idx in 0..8 {
                let x: i16 = sprite_screen_x + (7 - bit_idx);

                if x >= 160 || x < 0 {
                    continue;
                }

                let (tile_lsb, tile_msb) = self.fetch_sprite_tile_tuple(sprite);

                let x_flip = sprite.attr & OAM_BIT_X_FLIP != 0;

                let bit_idx_flip = if x_flip { 7 - bit_idx } else { bit_idx };

                let hb = (tile_msb >> bit_idx_flip) & 0x1;
                let lb = (tile_lsb >> bit_idx_flip) & 0x1;
                let sprite_color = lb | (hb << 1);

                if sprite_color == 0 {
                    // Color 0 is transparent for sprites
                    continue;
                }

                let sprite_palette = sprite.attr & (1 << 4);
                let sprite_bgpriority = sprite.attr & (1 << 7);

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
    }

    fn calc_mode3_len(&self, window_pos: Option<u8>) -> u16 {
        // @todo Check timing when window and a sprite fetch overlap
        let scx_penalty = u16::from(self.scx & 0x7);

        let window_penalty: u16 = if window_pos.is_some() { 6 } else { 0 };

        let mut sprite_penalty: u16 = 0;
        if self.lcdc_obj_enable {
            let mut bg_fifo_count: u8 = 8;

            let mut did_sprite_fetch = false;
            let mut x = 0;

            let mut sprite_mask: u16 = 0;
            while x < 160 {
                let sprite_opt = self
                    .sprite_buffer
                    .iter()
                    .rev()
                    .enumerate()
                    .find_map(|(i, sp)| {
                        let sprite_bit = 1 << i;
                        if sprite_mask & sprite_bit == 0 && sp.x < x + 8 {
                            Some(sprite_bit)
                        } else {
                            None
                        }
                    });

                if let Some(sprite_bit) = sprite_opt {
                    // println!("{} [fifo {bg_fifo_count}] - sprite idx {} at {}", x, sprite.0, sprite.1);
                    sprite_mask |= sprite_bit;

                    // if sprite_penalty == 0 {
                    //     sprite_penalty += 2;
                    // }

                    sprite_penalty += 6; // Sprite fetch cycles
                    did_sprite_fetch = true;
                } else {
                    if did_sprite_fetch {
                        did_sprite_fetch = false;
                        sprite_penalty += u16::from((6 as u8).saturating_sub(bg_fifo_count));
                        // println!("{} remaining pixel penalty {}", x, 6 - bg_fifo_count);
                    }
                    bg_fifo_count = 8 - (x & 0x7);
                    x += 1;
                }
            }
        }

        let mode3_penalties = scx_penalty + window_penalty + sprite_penalty;
        return CYCLES_PER_DRAW + mode3_penalties / 4;
    }

    fn mode_draw(&mut self) -> u16 {
        self.draw_background();
        let window_pos = self.draw_window();
        self.draw_sprites();

        self.calc_mode3_len(window_pos)
    }

    fn handle_stat_interrupt(&mut self, ctx: &mut soc::ClockContext) {
        let mut stat_interrupt = 0;

        if self.stat_lyc_select && self.stat_lyc_eq_ly {
            stat_interrupt |= 1 << STAT_SELECT_LYC_BIT;
        }

        if self.stat_mode2_select && self.stat_mode == PpuMode::PpuOamScan {
            stat_interrupt |= 1 << STAT_SELECT_MODE2_BIT;
        }

        if self.stat_mode1_select && self.stat_mode == PpuMode::PpuVBlank {
            stat_interrupt |= 1 << STAT_SELECT_MODE1_BIT;
        }

        if self.stat_mode0_select && self.stat_mode == PpuMode::PpuHBlank {
            stat_interrupt |= 1 << STAT_SELECT_MODE0_BIT;
        }

        // low to high transition
        if self.stat_interrupt == false && stat_interrupt != 0 {
            ctx.set_interrupt(interrupt::INTERRUPT_BIT_LCD);
        }

        self.stat_interrupt = stat_interrupt != 0;
    }
}
