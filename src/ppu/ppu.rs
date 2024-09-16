use std::{collections::VecDeque, fmt::{self, Display}, sync::mpsc::SyncSender};

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

struct GbPixel {
    pub color: u8,

    // DMG palette [Non CGB Mode only]: 0 = OBP0, 1 = OBP1
    pub palette: u8,
    // pub sprite_priority: u8, // CGB

    // Priority: 0 = No, 1 = BG and Window colors 1–3 are drawn over this OBJ
    pub bg_priority: u8,
}

#[derive(Debug)]
enum PixelfetcherStep {
    FetchTile,
    TileDataLow,
    TileDataHigh,
    PushFifo,
}

struct BgFetcher {
    current_step: PixelfetcherStep,
    fetcher_x: u8,
    window_line_counter: Option<u8>,
    tile_number: u8,
    tile_lsb: u8,
    tile_msb: u8,
    fresh_scanline: bool,
    is_window: bool,
    fifo: VecDeque<GbPixel>,
}

struct SpriteFetcher {
    current_step: PixelfetcherStep,
    tile_number: u8,
    tile_lsb: u8,
    tile_msb: u8,
    fifo: VecDeque<GbPixel>,
}

#[derive(Debug)]
struct Sprite {
    y: u8,
    x: u8, // x can be 0 and object is not drawn
    tile: u8,
    attr: u8,
}

pub struct PPU {
    is_disabled: bool,
    dots_mode: u16,
    dots_frame: u32,
    dots_leftover: u16,

    hblank_length: u16,

    stat_interrupt: u8,
    stat_interrupt_prev: u8,

    oam_cursor: u8,
    sprite_buffer: Vec<Sprite>,
    fetching_sprites: bool,

    // WY = LY has been true at some point during current frame
    // (checked at the start of Mode 2)
    draw_window: bool,

    foo: u8,

    bg_scroll_count: u8,

    bg_fetcher: BgFetcher,
    sprite_fetcher: SpriteFetcher,

    current_x: u8,

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
            fetching_sprites: false,
            dots_mode: 0,
            foo: 0,
            dots_frame: 0,
            dots_leftover: 0,
            hblank_length: 0,
            stat_interrupt: 0,
            stat_interrupt_prev: 0,
            bg_fetcher: BgFetcher{
                current_step: PixelfetcherStep::FetchTile,
                fetcher_x: 0,
                window_line_counter: None,
                tile_number: 0,
                tile_lsb: 0xFF,
                tile_msb: 0xFF,
                fresh_scanline: true,
                is_window: false,
                fifo: VecDeque::new(),
            },
            sprite_fetcher: SpriteFetcher{
                current_step: PixelfetcherStep::FetchTile,
                tile_number: 0,
                tile_lsb: 0xFF,
                tile_msb: 0xFF,
                fifo: VecDeque::new(),
            },
            oam_cursor: 0,
            sprite_buffer: Vec::with_capacity(10),
            current_x: 0,
            bg_scroll_count: 0,
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

        self.bg_fetcher.reset();
        self.sprite_fetcher.scanline_reset();
        self.sprite_buffer.clear();
        self.oam_cursor = 0;
        self.dots_frame = 0;
        self.dots_leftover = 0;
        self.dots_mode = 0;
        self.bg_scroll_count = 0;
        self.current_x = 0;
        self.hblank_length = 0;
        self.is_disabled = true;
        self.fetching_sprites = false;

        self.set_mode(mmu, PpuMode::PpuOamScan);
    }

    pub fn step(&mut self, mmu: &mut MMU, frame_chan: &mut SyncSender<FrameBuffer>, cycles_passed: u8) -> u8 {
        let lcd_enable = mmu.lcdc().check_bit(7);

        if !lcd_enable {
            self.reset(mmu);
            return 0;
        }

        if self.is_disabled {
            self.is_disabled = false;
            mmu.lock_region(MemoryRegion::MemRegionOAM as u8);
        }
    
        debug_assert!(mmu.ly().get() <= 153);
    
        let mut dots_budget = u16::from(cycles_passed * 4) + self.dots_leftover;
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

                // let is_logging = self.get_mode(mmu) == PpuMode::PpuDraw && mmu.ly().get() == 112 && self.current_x >= 60 && self.current_x <= 76;

                // if is_logging {
                //     println!("{} x={} bgfifo=[{}/{:?}] spfifo=[{}/{:?}]", self.dots_mode, self.current_x,
                //         self.bg_fetcher.fifo_len(),
                //         self.bg_fetcher.current_step,
                //         self.sprite_fetcher.fifo_len(),
                //         self.sprite_fetcher.current_step,
                //     );
                // }

                if let Some(next_mode) = mode_res {
                    match (self.get_mode(mmu), next_mode) {
                        (PpuMode::PpuOamScan, PpuMode::PpuDraw) => {
                            debug_assert!(self.dots_mode == DOTS_PER_OAM_SCAN);

                            // Start of scanline, discard scx % 8 pixels
                            debug_assert!(self.current_x == 0);
                            let scx = mmu.scx().get();
                            self.bg_scroll_count = scx % 8;
                            
                            if mmu.wy().get() == mmu.ly().get() {
                                self.draw_window = true;
                            }
                        }
                        (PpuMode::PpuDraw, PpuMode::PpuHBlank) => {
                            debug_assert!(self.dots_mode >= 172);
                            debug_assert!(self.dots_mode <= 289);

                            // println!("draw length={}", self.dots_mode);

                            // reset, move to hblank
                            self.hblank_length = 376 - self.dots_mode;
                            self.bg_fetcher.scanline_reset();
                            self.sprite_fetcher.scanline_reset();
                            self.bg_scroll_count = 0;
                            self.current_x = 0;
                            self.fetching_sprites = false;
                        }
                        (PpuMode::PpuHBlank, PpuMode::PpuOamScan) => {
                            debug_assert!(self.dots_frame == u32::from(mmu.ly().get()) * 456);
                        }
                        (PpuMode::PpuHBlank, PpuMode::PpuVBlank) => {
                            debug_assert!(self.dots_mode >= 87);
                            debug_assert!(self.dots_mode <= 204);
                            debug_assert!(mmu.ly().get() == 144);

                            self.draw_window = false;
                            self.bg_fetcher.window_line_counter = None;
                            self.foo = (self.foo + 1) % 26;

                            // @todo - Sending will fail when exiting the app.
                            // - A way to close emu thread and exit gracefully
                            frame_chan.send(self.rt).unwrap();

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

                            std::thread::sleep(std::time::Duration::from_millis(100 as u64));
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
    
        return 1;
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
            // println!("[{}] sprite_buffer={:?}", mmu.ly().get(), self.sprite_buffer);

            // @todo
            let obj_height = 8;
            let obj_addr = ADDR_OAM;
            let mut x_coord = 0;
            let y_coord = 50;
            self.sprite_buffer.clear();
            for i in 0..1 {
                if ly + 16 >= y_coord && ly + 16 < y_coord + obj_height {
                    self.sprite_buffer.push(Sprite{
                        y: y_coord,
                        x: x_coord + i * 8, // @todo sprite pos - 8
                        tile: 0x1f, // mmu.bus_read(obj_addr + 2),
                        attr: 0, // mmu.bus_read(obj_addr + 3),
                    });

                    // if i == 0 {
                    //     x_coord -= self.foo;
                    // }
                }
            }

            return Some((dots, Some(PpuMode::PpuDraw)));
        }

        Some((dots, None))
    }

    fn mode_draw(&mut self, mmu: &mut MMU, dots_to_run: u16) -> Option<(u16, Option<PpuMode>)> {
        /*
            The FIFO and Pixel Fetcher work together to ensure that the FIFO always contains at least 8 pixels at any given time,
            as 8 pixels are required for the Pixel Rendering operation to take place.
            Each FIFO is manipulated only during mode 3 (pixel transfer).
            https://gbdev.io/pandocs/pixel_fifo.html#get-tile
        */
        if self.dots_mode + dots_to_run > 289 {
            println!("dots={}", self.dots_mode);
        }

        let dots = 2;

        if dots_to_run < dots {
            return None;
        }

        if self.fetching_sprites {
            self.sprite_fetcher.step(mmu, &mut self.sprite_buffer);

            if self.sprite_fetcher.fifo_len() > 0 {
                self.fetching_sprites = false;
            }
        }

        if self.fetching_sprites {
            return Some((dots, None)); 
        }

        self.bg_fetcher.step(mmu);

        for dot in 1..=dots {
            if self.fetching_sprites == false && self.sprite_buffer.iter().any(|elem| elem.x <= self.current_x + 8) {
                self.fetching_sprites = true;
                self.bg_fetcher.reset_step();
                self.sprite_fetcher.scanline_reset();
                self.sprite_fetcher.step(mmu, &mut self.sprite_buffer);
                return Some((dots, None));
            }

            if let Some(bg_pixel) = self.bg_fetcher.fifo_pop() {
                if self.bg_scroll_count > 0 {
                    self.bg_scroll_count -= 1;
                    continue;
                }

                let ly = mmu.ly().get();
                let bgp = mmu.bgp().get();
                let mut palette_color = (bgp >> (bg_pixel.color * 2)) & 0x3;

                if let Some(sprite) = self.sprite_fetcher.fifo_pop() {
                    let mut push_sprite = true;

                    let sprite_palette = if sprite.palette == 0 {
                        mmu.obp0().get()
                    } else {
                        mmu.obp1().get()
                    };

                    let sprite_clr = (sprite_palette >> (sprite.color * 2)) & 0x3;

                    if mmu.lcdc().check_bit(1) == false {
                        push_sprite = false;
                    } else if sprite.color == 0 {
                        push_sprite = false;
                    } else if sprite.bg_priority == 1 && bg_pixel.color != 0 {
                        push_sprite = false;
                    }

                    if push_sprite {
                        palette_color = sprite_clr;
                    }
                }

                self.rt[ly as usize][self.current_x as usize] = palette_color;
                self.current_x += 1;

                let window_enabled_lcdc = mmu.lcdc().check_bit(5);
                let past_window_x = self.current_x >= mmu.wx().get().saturating_sub(7);

                // @todo - Emulate WX values 0-6
                if self.draw_window == true && window_enabled_lcdc && past_window_x {
                    self.bg_fetcher.switch_to_window_mode();
                }
            }

            if self.current_x == 160 {
                return Some((dot, Some(PpuMode::PpuHBlank)));
            }
        }

        Some((dots, None))
    }

    fn mode_hblank(&mut self, mmu: &mut MMU, dots_to_run: u16) -> Option<(u16, Option<PpuMode>)> {
        if self.dots_mode + dots_to_run >= self.hblank_length {
            let dots = self.hblank_length - self.dots_mode;

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

impl BgFetcher {
    fn reset(&mut self) {
        self.scanline_reset();
        self.window_line_counter = None;
    }

    fn scanline_reset(&mut self) {
        self.reset_step();
        self.fetcher_x = 0;
        self.is_window = false;
        self.fresh_scanline = true;
        self.fifo.clear();
    }

    fn reset_step(&mut self) {
        self.current_step = PixelfetcherStep::FetchTile;
        self.tile_number = 0;
        self.tile_lsb = 0xFF;
        self.tile_msb = 0xFF;
    }

    fn switch_to_window_mode(&mut self) {
        if self.is_window {
            // Already fetching window
            return;
        }

        self.scanline_reset();
        self.is_window = true;

        if let Some(window_line_count) = self.window_line_counter.as_mut() {
            *window_line_count += 1;
        } else {
            self.window_line_counter = Some(0);
        }
    }

    fn fifo_pop(&mut self) -> Option<GbPixel> {
        self.fifo.pop_front()
    }

    fn step(&mut self, mmu: &mut MMU) {
        match self.current_step {
            PixelfetcherStep::FetchTile => {
                self.fetch_bg_tile_number(mmu);
                self.current_step = PixelfetcherStep::TileDataLow;
            }
            PixelfetcherStep::TileDataLow => {
                self.fetch_bg_tile_byte(mmu, self.tile_number, false);
                self.current_step = PixelfetcherStep::TileDataHigh;
            }
            PixelfetcherStep::TileDataHigh => {
                if self.fresh_scanline && !self.is_window {
                    self.scanline_reset();
                    self.fresh_scanline = false;
                    return;
                }

                self.fetch_bg_tile_byte(mmu, self.tile_number, true);
                self.current_step = PixelfetcherStep::PushFifo;
            }
            PixelfetcherStep::PushFifo => {
                if self.fifo.len() > 8 {
                    todo!("push fifo should restart 2 times");
                    return;
                }

                for bit_idx in (0..8).rev() {
                    let hb = (self.tile_msb >> bit_idx) & 0x1;
                    let lb = (self.tile_lsb >> bit_idx) & 0x1;
                    let color = lb | (hb << 1);

                    self.fifo.push_back(GbPixel{ color, palette: 0, bg_priority: 0 });
                }

                self.fetcher_x += 1;
                self.current_step = PixelfetcherStep::FetchTile;
            }
        }
    }

    fn fetch_bg_tile_number(&mut self, mmu: &mut MMU) {
        let is_window = self.is_window && mmu.lcdc().check_bit(5);

        let tilemap_bit = if is_window { 6 } else { 3 };
        let tilemap_bit = mmu.lcdc().check_bit(tilemap_bit);
        let tilemap_addr = if tilemap_bit {
            ADDR_TILEMAP_9C00
        } else {
            ADDR_TILEMAP_9800
        };

        let current_tile_index: u16 = if is_window {
            let line_count = u16::from(self.window_line_counter.expect("line count should be set"));
            (u16::from(self.fetcher_x) + 32 * (line_count / 8)) & 0x3FF
        } else {
            let ly = mmu.ly().get();
            let scy = mmu.scy().get();
            let scx = mmu.scx().get();

            let x_coord = ((scx / 8) + self.fetcher_x) & 0x1F;
            let y_coord = ly.wrapping_add(scy);
            (u16::from(x_coord) + 32 * (u16::from(y_coord) / 8)) & 0x3FF
        };

        let tilemap_data_addr = tilemap_addr + current_tile_index;

        self.tile_number = mmu.bus_read(tilemap_data_addr);
    }

    fn fetch_bg_tile_byte(&mut self, mmu: &mut MMU, tile_number: u8, msb: bool) {
        let ly = mmu.ly().get();
        let scy = mmu.scy().get();
        let offset: u16 = if msb { 1 } else { 0 };
        
        let addressing_mode_8000 = mmu.lcdc().check_bit(4);
        let is_window = self.is_window && mmu.lcdc().check_bit(5);

        let line_offset = if is_window {
            u16::from(2 * (self.window_line_counter.expect("line count should be set") % 8))
        } else {
            u16::from(2 * (ly.wrapping_add(scy) % 8))
        };

        let tile_byte = if addressing_mode_8000 {
            mmu.bus_read(0x8000 + (u16::from(tile_number) * 16) + line_offset + offset)
        } else {
            let e: i8 = tile_number as i8;
            let base: u16 = 0x9000;
            mmu.bus_read(base.wrapping_add_signed(e as i16 * 16 + line_offset as i16) + offset)
        };

        if msb {
            self.tile_msb = tile_byte;
        } else {
            self.tile_lsb = tile_byte;
        }
    }
}

impl SpriteFetcher {
    fn scanline_reset(&mut self) {
        self.current_step = PixelfetcherStep::FetchTile;
        self.tile_lsb = 0xFF;
        self.tile_msb = 0xFF;
        self.tile_number = 0;
        self.fifo.clear();
    }

    fn fifo_pop(&mut self) -> Option<GbPixel> {
        self.fifo.pop_front()
    }

    fn fifo_len(&self) -> usize {
        self.fifo.len()
    }

    fn step(&mut self, mmu: &mut MMU, sprite_buffer: &mut Vec<Sprite>) {
        match self.current_step {
            PixelfetcherStep::FetchTile => {
                self.fetch_sprite_tile_number(mmu, sprite_buffer);
                self.current_step = PixelfetcherStep::TileDataLow;
            }
            PixelfetcherStep::TileDataLow => {
                self.fetch_sprite_tile_byte(mmu, false);
                self.current_step = PixelfetcherStep::TileDataHigh;
            }
            PixelfetcherStep::TileDataHigh => {
                self.fetch_sprite_tile_byte(mmu, true);
                self.current_step = PixelfetcherStep::PushFifo;
            }
            PixelfetcherStep::PushFifo => {
                for bit_idx in (0..8).rev() {
                    let hb = (self.tile_msb >> bit_idx) & 0x1;
                    let lb = (self.tile_lsb >> bit_idx) & 0x1;
                    let color = lb | (hb << 1);

                    let x = if (sprite_buffer.len() % 2) == 0 { 1 } else { 3 };

                    self.fifo.push_back(GbPixel{
                        // color,
                        color: x as u8,
                        palette: sprite_buffer[0].attr & (1 << 4),
                        bg_priority: sprite_buffer[0].attr & (1 << 7),
                    });
                }

                sprite_buffer.remove(0);

                self.current_step = PixelfetcherStep::FetchTile;
            }
        }
    }

    fn fetch_sprite_tile_number(&mut self, mmu: &mut MMU, sprite_buffer: &Vec<Sprite>) {
        self.tile_number = sprite_buffer[0].tile;
    }

    fn fetch_sprite_tile_byte(&mut self, mmu: &mut MMU, msb: bool) {
        let ly = mmu.ly().get();
        let offset: u16 = if msb { 1 } else { 0 };

        let line_offset = u16::from((ly % 8) * 2);
        let tile_byte = mmu.bus_read(0x8000 + (u16::from(self.tile_number) * 16) + line_offset + offset);

        if msb {
            self.tile_msb = tile_byte;
        } else {
            self.tile_lsb = tile_byte;
        }
    }
}
