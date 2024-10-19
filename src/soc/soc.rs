use std::time;

use crate::{
    apu::apu,
    cartridge::cartridge::Cartridge,
    mbc::{
        mbc::{MbcRomOnly, MBC},
        mbc1, mbc2, mbc3, mbc5,
    },
    ppu::ppu::{self, FrameBuffer, PPU},
    serial::serial,
    timer::timer::Timer,
    util::util,
    GbButton, InputReceiver,
};

use super::{hw_reg::*, interrupt::INTERRUPT_BIT_JOYPAD};

pub enum SocEventBits {
    SocEventNone = 0,
    SocEventVSync = 1 << 0,
    SocEventsExit = 1 << 1,
}

struct DmaTransfer {
    src: u16,
    count: u8,
    cycles: u16, // 160 + 1
}

pub struct ClockContext<'a> {
    interrupts: &'a mut u8,
    events: &'a mut u8,
    pub cycles: u64,
}

impl ClockContext<'_> {
    pub fn set_interrupt(&mut self, interrupt_bit: u8) {
        *self.interrupts |= interrupt_bit;
    }

    pub fn set_events(&mut self, event_bit: SocEventBits) {
        *self.events |= event_bit as u8;
    }
}

pub struct SOC {
    pub cycles: u64,
    apu: apu::APU,
    timer: Timer,
    ppu: ppu::PPU,
    serial: serial::Serial,

    wram: Vec<u8>,
    hram: Vec<u8>,

    p1_select_buttons: bool,
    p1_select_dpad: bool,
    r#if: u8,
    ie: u8,
    dma: u8,

    mbc: Box<dyn MBC>,

    dma_request: Option<u8>,
    active_dma: Option<DmaTransfer>,

    buttons: [bool; 8],

    event_bits: u8,
    input_recv: Option<InputReceiver>,

    enable_saving: bool,
    last_saved_at: std::time::Instant,

    run_for_cycles: Option<u64>,
}

impl SOC {
    pub fn new(
        cartridge: &Cartridge,
        input_recv: Option<InputReceiver>,
        sound_chan: Option<apu::ApuSoundSender>,
        frame_chan: Option<ppu::PpuFrameSender>,
        enable_saving: bool,
        sync_audio: bool,
        sync_video: bool,
        run_for_cycles: Option<u64>,
    ) -> SOC {
        let mut soc = Self {
            input_recv,
            enable_saving,
            run_for_cycles,
            cycles: 0,
            wram: vec![0; 0x4000],
            hram: vec![0; 0x7F],
            mbc: Box::new(MbcRomOnly::new()),
            active_dma: None,
            dma_request: None,

            buttons: [false; GbButton::GbButtonMax as usize],
            event_bits: 0,

            p1_select_buttons: false,
            p1_select_dpad: false,
            r#if: 0x1,
            ie: 0x0,
            dma: 0xFF,

            apu: apu::APU::new(sound_chan, sync_audio),
            ppu: ppu::PPU::new(frame_chan, sync_video),
            timer: Timer::new(),
            serial: serial::Serial::new(),

            last_saved_at: time::Instant::now(),
        };

        soc.load(&cartridge);
        soc
    }

    pub fn load(&mut self, cartridge: &Cartridge) {
        self.mbc = match cartridge.header.cart_type {
            0x1..=0x3 => Box::new(mbc1::MBC1::new()),
            0x5..=0x6 => Box::new(mbc2::MBC2::new()),
            0x0F..=0x13 => Box::new(mbc3::MBC3::new()),
            0x19..=0x1E => Box::new(mbc5::MBC5::new()),
            _ => {
                #[cfg(test)]
                if cartridge.header.cart_type != 0 {
                    println!(
                        "WARN: Unsupported cartridge/MBC: {}",
                        cartridge.header.cart_type
                    );
                }
                Box::new(MbcRomOnly::new())
            }
        };

        self.mbc.load(cartridge);
    }

    #[rustfmt::skip]
    pub fn clock_read(&mut self, address: u16) -> u8 {
        let active_dma = self.active_dma.is_some();
        self.clock();

        // https://gbdev.io/pandocs/Memory_Map.html
        return match address {
            0x0000..=0x7FFF => {
                self.mbc.read(address)
            }
            0x8000..=0x9FFF => {
                self.ppu.read_vram(address)
            }
            0xA000..=0xBFFF => {
                self.mbc.read(address)
            }
            0xC000..=0xDFFF => {
                self.wram[usize::from(address - 0xC000)]
            }
            0xE000..=0xFDFF => {
                // Echo RAM
                self.wram[usize::from(address - 0xE000)]
            }
            0xFE00..=0xFE9F => {
                if active_dma {
                    // @todo OAM DMA Bus conflicts
                    return 0xFF;
                }
                self.ppu.read_oam(address)
            }
            0xFEA0..=0xFEFF => {
                // @todo - Prohibited memory, on DMG triggers OAM corruption
                0
            }
            0xFF00..=0xFF7F => {
                match address {
                    HWR_P1 => {
                        let button_bits = util::calc_button_bits(
                            &self.buttons,
                            !self.p1_select_buttons,
                            !self.p1_select_dpad,
                        );
                        return button_bits
                            | ((self.p1_select_buttons as u8) << 5)
                            | ((self.p1_select_dpad as u8) << 4)
                            | 0xC0;
                    }
                    HWR_SB              => self.serial.read_sb(),
                    HWR_SC              => self.serial.read_sc(),
                    HWR_DIV             => self.timer.read_div(),
                    HWR_TIMA            => self.timer.read_tima(),
                    HWR_TMA             => self.timer.read_tma(),
                    HWR_TAC             => self.timer.read_tac(),
                    HWR_IF              => self.r#if | 0xE0,
                    HWR_NR10            => self.apu.read_nr10(),
                    HWR_NR11            => self.apu.read_nr11(),
                    HWR_NR12            => self.apu.read_nr12(),
                    HWR_NR13            => self.apu.read_nr13(),
                    HWR_NR14            => self.apu.read_nr14(),
                    HWR_NR21            => self.apu.read_nr21(),
                    HWR_NR22            => self.apu.read_nr22(),
                    HWR_NR23            => self.apu.read_nr23(),
                    HWR_NR24            => self.apu.read_nr24(),
                    0xFF30..=0xFF3F     => self.apu.read_wave_ram(address),
                    HWR_NR30            => self.apu.read_nr30(),
                    HWR_NR31            => self.apu.read_nr31(),
                    HWR_NR32            => self.apu.read_nr32(),
                    HWR_NR33            => self.apu.read_nr33(),
                    HWR_NR34            => self.apu.read_nr34(),
                    HWR_NR41            => self.apu.read_nr41(),
                    HWR_NR42            => self.apu.read_nr42(),
                    HWR_NR43            => self.apu.read_nr43(),
                    HWR_NR44            => self.apu.read_nr44(),
                    HWR_NR50            => self.apu.read_nr50(),
                    HWR_NR51            => self.apu.read_nr51(),
                    HWR_NR52            => self.apu.read_nr52(),
                    HWR_LCDC            => self.ppu.read_lcdc(),
                    HWR_STAT            => self.ppu.read_stat(),
                    HWR_LY              => self.ppu.read_ly(),
                    HWR_SCY             => self.ppu.read_scy(),
                    HWR_SCX             => self.ppu.read_scx(),
                    HWR_LYC             => self.ppu.read_lyc(),
                    HWR_DMA             => self.dma,
                    HWR_BGP             => self.ppu.read_bgp(),
                    HWR_OBP0            => self.ppu.read_obp0(),
                    HWR_OBP1            => self.ppu.read_obp1(),
                    HWR_WY              => self.ppu.read_wy(),
                    HWR_WX              => self.ppu.read_wx(),
                    _                   => 0xFF,
                }
            }
            0xFF80..=0xFFFE => {
                self.hram[usize::from(address) - 0xFF80]
            }
            0xFFFF => {
                self.ie
            }
        };
    }

    #[rustfmt::skip]
    pub fn clock_write(&mut self, address: u16, data: u8) {
        match address {
            0x0000..=0x7FFF => {
                self.clock();
                self.mbc.write(address, data);
            }
            0x8000..=0x9FFF => {
                self.clock();
                self.ppu.write_vram(address, data);
            }
            0xA000..=0xBFFF => {
                self.clock();
                self.mbc.write(address, data);
            }
            0xC000..=0xDFFF => {
                self.clock();
                self.wram[usize::from(address - 0xC000)] = data;
            }
            0xE000..=0xFDFF => {
                // Echo RAM
                self.clock();
                self.wram[usize::from(address - 0xE000)] = data;
            }
            0xFE00..=0xFE9F => {
                let active_dma = self.active_dma.is_some();
                self.clock();
                if active_dma {
                    // @todo OAM DMA Bus conflicts
                    return;
                }
                self.ppu.write_oam(address, data);
            }
            0xFEA0..=0xFEFF => {
                // unused
                // unreachable!();
                self.clock();
            }
            0xFF00..=0xFF7F => {
                match address {
                    HWR_P1 => {
                        self.clock();
                        self.p1_select_buttons = data & 0x20 != 0;
                        self.p1_select_dpad = data & 0x10 != 0;
                    }
                    HWR_SB => { self.clock(); self.serial.write_sb(data) },
                    HWR_SC => { self.clock(); self.serial.write_sc(data) },
                    HWR_IF => {
                        self.clock();
                        self.r#if = data & 0x1F;
                    }
                    HWR_DMA => {
                        self.clock();
                        self.dma_request = Some(data);
                        self.dma = data;
                    }
                    HWR_DIV                 => self.clock_timer_write(Timer::clock_write_div, data),
                    HWR_TAC                 => self.clock_timer_write(Timer::clock_write_tac, data),
                    HWR_TIMA                => self.clock_timer_write(Timer::clock_write_tima, data),
                    HWR_TMA                 => self.clock_timer_write(Timer::clock_write_tma, data),
                    HWR_NR10                => { self.clock(); self.apu.write_nr10(data) }
                    HWR_NR11                => { self.clock(); self.apu.write_nr11(data) }
                    HWR_NR12                => { self.clock(); self.apu.write_nr12(data) }
                    HWR_NR13                => { self.clock(); self.apu.write_nr13(data) }
                    HWR_NR14                => { self.clock(); self.apu.write_nr14(data) }
                    HWR_NR21                => { self.clock(); self.apu.write_nr21(data) }
                    HWR_NR22                => { self.clock(); self.apu.write_nr22(data) }
                    HWR_NR23                => { self.clock(); self.apu.write_nr23(data) }
                    HWR_NR24                => { self.clock(); self.apu.write_nr24(data) }
                    0xFF30..=0xFF3F         => { self.clock(); self.apu.write_wave_ram(address, data) }
                    HWR_NR30                => { self.clock(); self.apu.write_nr30(data) },
                    HWR_NR31                => { self.clock(); self.apu.write_nr31(data) },
                    HWR_NR32                => { self.clock(); self.apu.write_nr32(data) },
                    HWR_NR33                => { self.clock(); self.apu.write_nr33(data) },
                    HWR_NR34                => { self.clock(); self.apu.write_nr34(data) },
                    HWR_NR41                => { self.clock(); self.apu.write_nr41(data) },
                    HWR_NR42                => { self.clock(); self.apu.write_nr42(data) },
                    HWR_NR43                => { self.clock(); self.apu.write_nr43(data) },
                    HWR_NR44                => { self.clock(); self.apu.write_nr44(data) },
                    HWR_NR50                => { self.clock(); self.apu.write_nr50(data) },
                    HWR_NR51                => { self.clock(); self.apu.write_nr51(data) },
                    HWR_NR52                => { self.clock(); self.apu.write_nr52(data) },
                    HWR_LCDC                => self.clock_ppu_write(PPU::clock_write_lcdc, data),
                    HWR_STAT                => self.clock_ppu_write(PPU::clock_write_stat, data),
                    HWR_LY                  => self.clock_ppu_write(PPU::clock_write_ly, data),
                    HWR_SCY                 => self.clock_ppu_write(PPU::clock_write_scy, data),
                    HWR_SCX                 => self.clock_ppu_write(PPU::clock_write_scx, data),
                    HWR_LYC                 => self.clock_ppu_write(PPU::clock_write_lyc, data),
                    HWR_BGP                 => self.clock_ppu_write(PPU::clock_write_bgp, data),
                    HWR_OBP0                => self.clock_ppu_write(PPU::clock_write_obp0, data),
                    HWR_OBP1                => self.clock_ppu_write(PPU::clock_write_obp1, data),
                    HWR_WY                  => self.clock_ppu_write(PPU::clock_write_wy, data),
                    HWR_WX                  => self.clock_ppu_write(PPU::clock_write_wx, data),
                    _                       => { self.clock(); /* unused */},
                }
            }
            0xFF80..=0xFFFE => {
                self.clock();
                self.hram[usize::from(address) - 0xFF80] = data;
            }
            0xFFFF => {
                self.clock();
                self.ie = data;
            }
        }
    }

    pub fn clock(&mut self) {
        self.clock_oam_dma();

        let mut ctx = ClockContext {
            interrupts: &mut self.r#if,
            events: &mut self.event_bits,
            cycles: self.cycles,
        };

        self.ppu.clock(&mut ctx);
        self.timer.clock(&mut ctx);

        self.mbc.clock();
        self.apu.clock();
        self.serial.clock(&mut ctx);

        self.cycles += 1;
    }

    pub fn clock_timer_write(
        &mut self,
        clock_cb: fn(&mut Timer, data: u8, ctx: &mut ClockContext),
        data: u8,
    ) {
        self.clock_oam_dma();

        let mut ctx = ClockContext {
            interrupts: &mut self.r#if,
            events: &mut self.event_bits,
            cycles: self.cycles,
        };

        self.ppu.clock(&mut ctx);
        clock_cb(&mut self.timer, data, &mut ctx);

        self.mbc.clock();
        self.apu.clock();
        self.serial.clock(&mut ctx);

        self.cycles += 1;
    }

    pub fn clock_ppu_write(
        &mut self,
        clock_cb: fn(&mut PPU, data: u8, ctx: &mut ClockContext),
        data: u8,
    ) {
        self.clock_oam_dma();

        let mut ctx = ClockContext {
            interrupts: &mut self.r#if,
            events: &mut self.event_bits,
            cycles: self.cycles,
        };

        clock_cb(&mut self.ppu, data, &mut ctx);
        self.timer.clock(&mut ctx);

        self.mbc.clock();
        self.apu.clock();
        self.serial.clock(&mut ctx);

        self.cycles += 1;
    }

    pub fn set_interrupt(&mut self, interrupt: u8) {
        let flags_if = self.r#if;
        self.r#if = flags_if | interrupt;
    }

    pub fn clear_interrupt(&mut self, interrupt: u8) {
        let if_flags = self.r#if;
        self.r#if = if_flags & !interrupt;
    }

    pub fn active_interrupts(&mut self) -> u8 {
        let ie_flags = self.ie;
        let if_flags = self.r#if;

        return ie_flags & if_flags;
    }

    pub fn get_framebuffer(&self) -> &FrameBuffer {
        self.ppu.get_framebuffer()
    }

    pub fn close(&mut self) {
        if self.enable_saving {
            self.save();
        }
        self.apu.close();
    }

    pub fn save(&mut self) {
        self.mbc.save();
    }

    pub fn input_update(&mut self) {
        let mut interrupt = false;

        match &self.input_recv {
            Some(input_recv) => loop {
                match input_recv.try_recv() {
                    Ok(input_event) => {
                        let was_down = self.buttons[input_event.button as usize];

                        self.buttons[input_event.button as usize] = input_event.down;

                        if !was_down && input_event.down {
                            interrupt = true;
                        }
                    }
                    Err(_err) => break,
                }
            },
            None => {}
        }

        if interrupt {
            self.set_interrupt(INTERRUPT_BIT_JOYPAD);
        }
    }

    pub fn process_events(&mut self) -> bool {
        if self.event_bits & SocEventBits::SocEventVSync as u8 == 0 {
            self.event_bits = 0;
            return false;
        }

        let exit = self.event_bits & SocEventBits::SocEventsExit as u8 != 0
            || if let Some(max_cycles) = self.run_for_cycles {
                self.cycles > max_cycles
            } else {
                false
            };

        self.input_update();

        if self.enable_saving && self.last_saved_at.elapsed() > time::Duration::from_secs(60) {
            self.save();
            self.last_saved_at = time::Instant::now();
        }

        if exit {
            self.close();
        }

        self.event_bits = 0;
        exit
    }

    fn clock_oam_dma(&mut self) {
        // @todo - When the CPU attempts to read a byte from ROM/RAM during a DMA transfer,
        // instead of the actual value at the given memory address,
        // the byte that is currently being transferred by the DMA transfer is returned.
        // This also affects the CPU when fetching opcodes, allowing for code execution through DMA transfers.
        // https://hacktix.github.io/GBEDG/dma/

        if let Some(active_dma) = &mut self.active_dma {
            let c = u16::from(active_dma.count);

            let dst = active_dma.src + c;
            let address = match dst {
                0xFE00..=0xFFFF => 0xC000 + (dst & 0x1FFF),
                _ => dst,
            };

            debug_assert!(address < 0xFE00 || address > 0xFE9F);

            let byte = match address {
                0x0000..=0x7FFF => self.mbc.read(address),
                0x8000..=0x9FFF => self.ppu.read_vram(address), // not clocked
                0xA000..=0xBFFF => self.mbc.read(address),
                0xC000..=0xDFFF => self.wram[usize::from(address - 0xC000)],
                0xE000..=0xFDFF => self.wram[usize::from(address - 0xE000)],
                0xFE00..=0xFFFF => {
                    unreachable!()
                }
            };

            self.ppu.oam_dma(0xFE00 + c, byte);

            active_dma.cycles += 1;
            active_dma.count += 1;

            if active_dma.count > 0x9F {
                self.active_dma = None;
            }
        }

        if let Some(dma_request) = self.dma_request.take() {
            let dma = DmaTransfer {
                src: util::value(dma_request, 0x0),
                count: 0,
                cycles: 1, // cycle 1 initialization delay
            };
            self.active_dma = Some(dma);
        }
    }
}
