use crate::{
    apu::apu,
    cartridge::cartridge::Cartridge,
    mbc::{
        mbc::{MbcRomOnly, MBC},
        mbc1, mbc2, mbc3, mbc5,
    },
    ppu::ppu::{self, FrameBuffer},
    serial::serial,
    timer::timer::Timer,
    util::util,
    GbButton, InputEvent,
};

use super::{hw_reg::*, interrupt::INTERRUPT_BIT_JOYPAD};

pub enum SocEventBits {
    SocEventNone = 0,
    SocEventVSync = 1 << 0,
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

    memory: Vec<u8>,

    mbc: Box<dyn MBC>,

    dma_request: Option<u8>,
    active_dma: Option<DmaTransfer>,

    buttons: [bool; GbButton::GbButtonMax as usize],

    event_bits: u8,

    #[cfg(test)]
    supported_carttype: bool,
    #[cfg(test)]
    rom_path: String,
}

impl SOC {
    pub fn new(cartridge: &Cartridge) -> SOC {
        let mut soc = Self {
            cycles: 0,
            memory: vec![0; 0],
            mbc: Box::new(MbcRomOnly::new()),
            active_dma: None,
            dma_request: None,
            buttons: [false; GbButton::GbButtonMax as usize],
            event_bits: 0,

            apu: apu::APU::new(),
            timer: Timer::new(),
            ppu: ppu::PPU::new(),
            serial: serial::Serial::new(),

            #[cfg(test)]
            supported_carttype: true,
            #[cfg(test)]
            rom_path: cartridge.rom_path.clone(),
        };

        soc.load(&cartridge);
        soc
    }

    pub fn load(&mut self, cartridge: &Cartridge) {
        self.memory = vec![0; 0x10000];

        self.memory[0xFF50] = 0x1;
        self.memory[HWR_P1 as usize] = 0xCF;
        self.memory[HWR_IF as usize] = 0xE1;
        self.memory[HWR_DMA as usize] = 0xFF;

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

                    self.supported_carttype = false;
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
            0xC000..=0xCFFF => {
                self.memory[usize::from(address)]
            }
            0xD000..=0xDFFF => {
                self.memory[usize::from(address)]
            }
            0xE000..=0xFDFF => {
                // Echo RAM
                self.memory[usize::from(address - 0x2000)]
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
                        let p1 = self.memory[address as usize];

                        let button_bits = util::calc_button_bits(&self.buttons, p1);
                        return button_bits | (p1 & 0xF0);
                    }
                    HWR_SB              => self.serial.read_sb(),
                    HWR_SC              => self.serial.read_sc(),
                    HWR_DIV             => self.timer.read_div(),
                    HWR_TAC             => self.timer.read_tac(),
                    HWR_TIMA            => self.timer.read_tima(),
                    HWR_TMA             => self.timer.read_tma(),
                    HWR_DIV_LSB         => 0xFF,
                    0xFF08..=0xFF0E     => 0xFF,
                    0xFF15              => 0xFF,
                    0xFF1F              => 0xFF,
                    0xFF27..=0xFF2F     => 0xFF,
                    0xFF4C              => 0xFF,
                    0xFF4D..=0xFF7F     => 0xFF, // Non-dmg regs
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
                    HWR_BGP             => self.ppu.read_bgp(),
                    HWR_OBP0            => self.ppu.read_obp0(),
                    HWR_OBP1            => self.ppu.read_obp1(),
                    HWR_WY              => self.ppu.read_wy(),
                    HWR_WX              => self.ppu.read_wx(),
                    _                   => self.memory[usize::from(address)],
                }
            }
            0xFF80..=0xFFFE => {
                // HRAM
                self.memory[usize::from(address)]
            }
            0xFFFF => {
                // IE
                self.memory[usize::from(address)]
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
            0xC000..=0xCFFF => {
                self.clock();
                self.memory[usize::from(address)] = data;
            }
            0xD000..=0xDFFF => {
                self.clock();
                self.memory[usize::from(address)] = data;
            }
            0xE000..=0xFDFF => {
                self.clock();
                self.memory[usize::from(address - 0x2000)] = data;
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
                        // Lower nibble RO
                        let ro_bits = self.memory[usize::from(address)] & 0xCF;
                        self.memory[usize::from(address)] = (data & 0x30) | ro_bits;
                    }
                    HWR_SB => { self.clock(); self.serial.write_sb(data) },
                    HWR_SC => { self.clock(); self.serial.write_sc(data) },
                    HWR_IF => {
                        self.clock();
                        // Top 3 bits unused
                        let ro_bits = self.memory[usize::from(address)] & 0xE0;
                        self.memory[usize::from(address)] = (data & 0x1F) | ro_bits;
                    }
                    HWR_DMA => {
                        self.clock();
                        self.dma_request = Some(data);
                        self.memory[usize::from(address)] = data;
                    }
                    HWR_DIV                 => self.clock_timer_write(Timer::clock_write_div, data),
                    HWR_TAC                 => self.clock_timer_write(Timer::clock_write_tac, data),
                    HWR_TIMA                => self.clock_timer_write(Timer::clock_write_tima, data),
                    HWR_TMA                 => self.clock_timer_write(Timer::clock_write_tma, data),
                    // 0xFF4F               => { todo!("select vram bank cgb"); } // @todo CGB: vram bank
                    0xFF4D..=0xFF70         => { self.clock(); }
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
                    HWR_LCDC                => { self.clock(); self.ppu.write_lcdc(data) },
                    HWR_STAT                => { self.clock(); self.ppu.write_stat(data) },
                    HWR_LY                  => { self.clock(); self.ppu.write_ly(data) },
                    HWR_SCY                 => { self.clock(); self.ppu.write_scy(data) },
                    HWR_SCX                 => { self.clock(); self.ppu.write_scx(data) },
                    HWR_LYC                 => { self.clock(); self.ppu.write_lyc(data) },
                    HWR_BGP                 => { self.clock(); self.ppu.write_bgp(data) },
                    HWR_OBP0                => { self.clock(); self.ppu.write_obp0(data) },
                    HWR_OBP1                => { self.clock(); self.ppu.write_obp1(data) },
                    HWR_WY                  => { self.clock(); self.ppu.write_wy(data) },
                    HWR_WX                  => { self.clock(); self.ppu.write_wx(data) },
                    HWR_DIV_LSB             => { self.clock(); },
                    _                       => { self.clock(); self.memory[usize::from(address)] = data },
                }
            }
            0xFF80..=0xFFFE => {
                self.clock();
                self.memory[usize::from(address)] = data;
            }
            0xFFFF => {
                self.clock();
                // @todo - IE flag top 3 bits are unused (still writable?)
                self.memory[usize::from(address)] = data;
            }
        }
    }

    pub fn clock(&mut self) {
        self.clock_oam_dma();

        let mut ctx = ClockContext {
            interrupts: &mut self.memory[HWR_IF as usize],
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
        clock_timer_cb: fn(&mut Timer, data: u8, ctx: &mut ClockContext),
        data: u8,
    ) {
        self.clock_oam_dma();

        let mut ctx = ClockContext {
            interrupts: &mut self.memory[HWR_IF as usize],
            events: &mut self.event_bits,
            cycles: self.cycles,
        };

        self.ppu.clock(&mut ctx);
        clock_timer_cb(&mut self.timer, data, &mut ctx);

        self.mbc.clock();
        self.apu.clock();
        self.serial.clock(&mut ctx);

        self.cycles += 1;
    }

    pub fn set_interrupt(&mut self, interrupt: u8) {
        let flags_if = self.memory[HWR_IF as usize];
        self.memory[HWR_IF as usize] = flags_if | interrupt;
    }

    pub fn clear_interrupt(&mut self, interrupt: u8) {
        let if_flags = self.memory[HWR_IF as usize];
        self.memory[HWR_IF as usize] = if_flags & !interrupt;
    }

    pub fn active_interrupts(&mut self) -> u8 {
        let ie_flags = self.memory[HWR_IE as usize];
        let if_flags = self.memory[HWR_IF as usize];

        return ie_flags & if_flags;
    }

    pub fn enable_external_audio(&mut self, sound_chan: apu::ApuSoundSender) {
        self.apu.enable_external_audio(sound_chan);
    }

    pub fn get_framebuffer(&self) -> &FrameBuffer {
        self.ppu.get_framebuffer()
    }

    pub fn close(&mut self) {
        self.mbc.save();
        self.apu.close();
    }

    pub fn save(&mut self) {
        self.mbc.save();
    }

    pub fn input_update(&mut self, input_event: &InputEvent) {
        let was_down = self.buttons[input_event.button as usize];

        self.buttons[input_event.button as usize] = input_event.down;

        if !was_down && input_event.down {
            self.set_interrupt(INTERRUPT_BIT_JOYPAD);
        }
    }

    pub fn flush_events(&mut self) -> u8 {
        let events = self.event_bits;
        self.event_bits = 0;
        events
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
                0xC000..=0xCFFF => self.memory[usize::from(address)],
                0xD000..=0xDFFF => self.memory[usize::from(address)],
                0xE000..=0xFDFF => self.memory[usize::from(address - 0x2000)],
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

    #[cfg(test)]
    pub fn is_supported_cart_type(&self) -> bool {
        self.supported_carttype
    }

    #[cfg(test)]
    pub fn get_rom_path(&self) -> &str {
        &self.rom_path
    }
}
