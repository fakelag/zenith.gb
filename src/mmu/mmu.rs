use crate::apu::apu;
use crate::cartridge::cartridge::*;
use crate::cpu::cpu;
use crate::emu::emu::{self, GbButton::*};
use crate::ppu::ppu::{self, FrameBuffer};
use crate::timer::timer;
use crate::util::util;

use super::hw_reg::*;
use super::mbc1;
use super::mbc2;
use super::mbc3;
use super::mbc5;

pub trait MBC {
    fn load(&mut self, cartridge: &Cartridge);
    fn read(&self, address: u16) -> u8;
    fn write(&mut self, address: u16, data: u8);
    fn step(&mut self, cycles: u8);
    fn save(&mut self);
}

struct MbcRomOnly {
    rom: Vec<u8>,
}

#[repr(u8)]
#[derive(Copy, Clone, PartialEq)]
pub enum AccessOrigin {
    AccessOriginNone,
    AccessOriginCPU
}

pub struct DmaTransfer {
    pub src: u16,
    pub count: u8,
    pub delay: u8,
    pub cycles: u16, // 160 + 1
}

pub struct MMU {
    memory: Vec<u8>,
    access_origin: AccessOrigin,
    supported_carttype: bool,

    mbc: Box<dyn MBC>,

    dma_request: Option<u8>,
    active_dma: Option<DmaTransfer>,

    buttons: [bool; emu::GbButton::GbButtonMax as usize],

    // @todo - These are here for now to trigger memory writes directly
    apu: apu::APU,
    timer: timer::Timer,
    ppu: ppu::PPU,
}

impl MMU {
    pub fn new(cartridge: &Cartridge) -> MMU {
        let mut mmu = Self {
            memory: vec![0; 0x10000],
            access_origin: AccessOrigin::AccessOriginNone,
            mbc: Box::new(MbcRomOnly::new()),
            active_dma: None,
            dma_request: None,
            buttons: [false; emu::GbButton::GbButtonMax as usize],
            supported_carttype: true,

            apu: apu::APU::new(),
            timer: timer::Timer::new(),
            ppu: ppu::PPU::new(),
        };
        mmu.load(cartridge);
        mmu
    }

    pub fn load(&mut self, cartridge: &Cartridge) {
        self.memory = vec![0; 0x10000];

        self.memory[0xFF50] = 0x1;
        self.memory[HWR_P1 as usize] = 0xCF;
        self.memory[HWR_SB as usize] = 0x00;
        self.memory[HWR_SC as usize] = 0x7E;
        self.memory[HWR_IF as usize] = 0xE1;
        self.memory[HWR_DMA as usize] = 0xFF;

        self.mbc = match cartridge.header.cart_type {
            0x1..=0x3 => {
                Box::new(mbc1::MBC1::new())
            }
            0x5..=0x6 => {
                Box::new(mbc2::MBC2::new())
            }
            0x0F..=0x13 => {
                Box::new(mbc3::MBC3::new())
            }
            0x19..=0x1E => {
                Box::new(mbc5::MBC5::new())
            }
            _ => {
                if cartridge.header.cart_type != 0 {
                    println!("WARN: Unsupported cartridge/MBC: {}", cartridge.header.cart_type);
                    self.supported_carttype = false;
                }
                Box::new(MbcRomOnly::new())
            }
        };

        self.mbc.load(cartridge);
    }

    pub fn close(&mut self) {
        self.apu.close();
    }

    pub fn save(&mut self) {
        self.mbc.save();
    }

    pub fn get_apu(&mut self) -> &mut apu::APU {
        &mut self.apu
    }

    pub fn update_input(&mut self, input_event: &emu::InputEvent) {
        let was_down = self.buttons[input_event.button as usize];

        self.buttons[input_event.button as usize] = input_event.down;

        if !was_down && input_event.down {
            let flags_if = self.r#if().get();
            self.r#if().set(flags_if | cpu::INTERRUPT_BIT_JOYPAD);
        }
    }

    pub fn address_accessible(&self, address: u16) -> bool {
        if self.access_origin != AccessOrigin::AccessOriginCPU {
            return true;
        }

        // @todo Optimise memory access checking
        // @todo Bus conflicts - during DMA RAM/ROM should be inaccessible

        // if self.active_dma.is_some() {
        //     match address {
        //         0x0000..=0x7FFF => return false,
        //         0xA000..=0xBFFF => return false,
        //         0xC000..=0xDFFF => return false,
        //         _ => {}
        //     }
        // }

        match address {
            0xFE00..=0xFE9F => !self.active_dma.is_some(),
            _ => true
        }
    }

    pub fn set_access_origin(&mut self, origin: AccessOrigin) {
        self.access_origin = origin;
    }

    pub fn get_framebuffer(&self) -> &FrameBuffer {
        self.ppu.get_framebuffer()
    }

    pub fn bus_read(&mut self, address: u16) -> u8 {
        if !self.address_accessible(address) {
            return 0xFF;
        }

        // https://gbdev.io/pandocs/Memory_Map.html
        match address {
            0x0000..=0x7FFF => {
                return self.mbc.read(address);
            }
            0x8000..=0x9FFF => {
                // 8 KiB Video RAM (VRAM)
                return self.ppu.read_vram(address);
            }
            0xA000..=0xBFFF => {
                return self.mbc.read(address);
            }
            0xC000..=0xCFFF => {
                // 4 KiB Work RAM (WRAM)
                return self.memory[usize::from(address)];
            }
            0xD000..=0xDFFF => {
                // @todo CGB: Work ram banking
                // 4 KiB Work RAM (WRAM) - In CGB mode, switchable bank 1–7
                return self.memory[usize::from(address)];
            }
            0xE000..=0xFDFF => {
                // Echo RAM
                // @todo "In some flash cartridges, echo RAM interferes with SRAM normally at A000-BFFF"
                // https://gbdev.io/pandocs/Memory_Map.html#echo-ram
                return self.memory[usize::from(address - 0x2000)];
            }
            0xFE00..=0xFE9F => {
                // OAM - https://gbdev.io/pandocs/OAM.html#object-attribute-memory-oam
                return self.ppu.read_oam(address);
            }
            0xFEA0..=0xFEFF => {
                // Prohibited memory.
                // @todo - On DMG triggers OAM corruption
                return 0;
            }
            0xFF00..=0xFF7F => {
                match address {
                    HWR_P1 => {
                        let p1 = self.memory[address as usize];

                        let button_bits = self.calc_button_bits(p1);
                        return button_bits | (p1 & 0xF0);
                    }
                    HWR_DIV             => { self.timer.read_div() }
                    HWR_TAC             => { self.timer.read_tac() }
                    HWR_TIMA            => { self.timer.read_tima() }
                    HWR_TMA             => { self.timer.read_tma() }
                    HWR_DIV_LSB         => { return 0xFF; }
                    0xFF08..=0xFF0E     => { return 0xFF; }
                    0xFF15              => { return 0xFF; }
                    0xFF1F              => { return 0xFF; }
                    0xFF27..=0xFF2F     => { return 0xFF; }
                    0xFF4C              => { return 0xFF; }
                    0xFF4D..=0xFF7F     => { return 0xFF; } // Non-dmg regs
                    HWR_NR10            => { return self.apu.read_nr10(); }
                    HWR_NR11            => { return self.apu.read_nr11(); }
                    HWR_NR12            => { return self.apu.read_nr12(); }
                    HWR_NR13            => { return self.apu.read_nr13(); }
                    HWR_NR14            => { return self.apu.read_nr14(); }
                    HWR_NR21            => { return self.apu.read_nr21(); }
                    HWR_NR22            => { return self.apu.read_nr22(); }
                    HWR_NR23            => { return self.apu.read_nr23(); }
                    HWR_NR24            => { return self.apu.read_nr24(); }
                    0xFF30..=0xFF3F     => { return self.apu.read_wave_ram(address); }
                    HWR_NR30            => { return self.apu.read_nr30(); }
                    HWR_NR31            => { return self.apu.read_nr31(); }
                    HWR_NR32            => { return self.apu.read_nr32(); }
                    HWR_NR33            => { return self.apu.read_nr33(); }
                    HWR_NR34            => { return self.apu.read_nr34(); }
                    HWR_NR41            => { return self.apu.read_nr41(); }
                    HWR_NR42            => { return self.apu.read_nr42(); }
                    HWR_NR43            => { return self.apu.read_nr43(); }
                    HWR_NR44            => { return self.apu.read_nr44(); }
                    HWR_NR50            => { return self.apu.read_nr50(); }
                    HWR_NR51            => { return self.apu.read_nr51(); }
                    HWR_NR52            => { return self.apu.read_nr52(); }
                    HWR_LCDC            => { return self.ppu.read_lcdc(); }
                    HWR_STAT            => { return self.ppu.read_stat(); }
                    HWR_LY              => { return self.ppu.read_ly(); }
                    HWR_SCY             => { return self.ppu.read_scy(); }
                    HWR_SCX             => { return self.ppu.read_scx(); }
                    HWR_LYC             => { return self.ppu.read_lyc(); }
                    HWR_BGP             => { return self.ppu.read_bgp(); }
                    HWR_OBP0            => { return self.ppu.read_obp0(); }
                    HWR_OBP1            => { return self.ppu.read_obp1(); }
                    HWR_WY              => { return self.ppu.read_wy(); }
                    HWR_WX              => { return self.ppu.read_wx(); }
                    _ => {
                        // IO ranges
                        return self.memory[usize::from(address)];
                    }
                }
            }
            0xFF80..=0xFFFE => {
                // HRAM
                return self.memory[usize::from(address)];
            }
            0xFFFF => {
                // IE
                return self.memory[usize::from(address)];
            }
        }
    }

    pub fn bus_write(&mut self, address: u16, data: u8) {
        if !self.address_accessible(address) {
            return;
        }

        match address {
            0x0000..=0x7FFF => {
                self.mbc.write(address, data);
            }
            0x8000..=0x9FFF => {
                return self.ppu.write_vram(address, data);
            }
            0xA000..=0xBFFF => {
                self.mbc.write(address, data);
            }
            0xC000..=0xCFFF => {
                self.memory[usize::from(address)] = data;
            }
            0xD000..=0xDFFF => {
                self.memory[usize::from(address)] = data;
            }
            0xE000..=0xFDFF => {
                self.memory[usize::from(address - 0x2000)] = data;
            }
            0xFE00..=0xFE9F => {
                self.ppu.write_oam(address, data);
            }
            0xFEA0..=0xFEFF => {
                // unused
                // unreachable!();
            }
            0xFF00..=0xFF7F => {
                match address {
                    HWR_P1 => {
                        // Lower nibble RO
                        let ro_bits = self.memory[usize::from(address)] & 0xCF;
                        self.memory[usize::from(address)] = (data & 0x30) | ro_bits;
                    }
                    HWR_IF => {
                        // Top 3 bits unused
                        let ro_bits = self.memory[usize::from(address)] & 0xE0;
                        self.memory[usize::from(address)] = (data & 0x1F) | ro_bits;
                    }
                    HWR_DMA => {
                        self.dma_request = Some(data);
                        self.memory[usize::from(address)] = data;
                    }
                    HWR_SC              => {}
                    HWR_DIV_LSB         => { /* RO */ }
                    HWR_DIV             => { self.timer.write_div(data) }
                    HWR_TAC             => { self.timer.write_tac(data) }
                    HWR_TIMA            => { self.timer.write_tima(data) }
                    HWR_TMA             => { self.timer.write_tma(data) }
                    // 0xFF4F           => { todo!("select vram bank cgb"); } // @todo CGB: vram bank
                    0xFF4D..=0xFF70     => {} // Non-dmg regs
                    HWR_NR10            => { self.apu.write_nr10(data); }
                    HWR_NR11            => { self.apu.write_nr11(data); }
                    HWR_NR12            => { self.apu.write_nr12(data); }
                    HWR_NR13            => { self.apu.write_nr13(data); }
                    HWR_NR14            => { self.apu.write_nr14(data); }
                    HWR_NR21            => { self.apu.write_nr21(data); }
                    HWR_NR22            => { self.apu.write_nr22(data); }
                    HWR_NR23            => { self.apu.write_nr23(data); }
                    HWR_NR24            => { self.apu.write_nr24(data); }
                    0xFF30..=0xFF3F     => { self.apu.write_wave_ram(address, data); }
                    HWR_NR30            => { self.apu.write_nr30(data); }
                    HWR_NR31            => { self.apu.write_nr31(data); }
                    HWR_NR32            => { self.apu.write_nr32(data); }
                    HWR_NR33            => { self.apu.write_nr33(data); }
                    HWR_NR34            => { self.apu.write_nr34(data); }
                    HWR_NR41            => { self.apu.write_nr41(data); }
                    HWR_NR42            => { self.apu.write_nr42(data); }
                    HWR_NR43            => { self.apu.write_nr43(data); }
                    HWR_NR44            => { self.apu.write_nr44(data); }
                    HWR_NR50            => { self.apu.write_nr50(data); }
                    HWR_NR51            => { self.apu.write_nr51(data); }
                    HWR_NR52            => { self.apu.write_nr52(data); }
                    HWR_LCDC            => { self.ppu.write_lcdc(data); }
                    HWR_STAT            => { self.ppu.write_stat(data); }
                    HWR_LY              => { self.ppu.write_ly(data); }
                    HWR_SCY             => { self.ppu.write_scy(data); }
                    HWR_SCX             => { self.ppu.write_scx(data); }
                    HWR_LYC             => { self.ppu.write_lyc(data); }
                    HWR_BGP             => { self.ppu.write_bgp(data); }
                    HWR_OBP0            => { self.ppu.write_obp0(data); }
                    HWR_OBP1            => { self.ppu.write_obp1(data); }
                    HWR_WY              => { self.ppu.write_wy(data); }
                    HWR_WX              => { self.ppu.write_wx(data); }
                    _ => {
                        self.memory[usize::from(address)] = data;
                    }
                }
            }
            0xFF80..=0xFFFE => {
                self.memory[usize::from(address)] = data;
            }
            0xFFFF => {
                // @todo - IE flag top 3 bits are unused (still writable?)
                self.memory[usize::from(address)] = data;
            }
        }
    }

    pub fn step(&mut self, cycles_passed: u8) -> bool {
        let (vsync, interrupts) = self.ppu.step(cycles_passed);

        let flags_if = self.r#if().get();
        self.r#if().set(flags_if | interrupts);

        if self.timer.step(cycles_passed) {
            let flags_if = self.r#if().get();
            self.r#if().set(flags_if | cpu::INTERRUPT_BIT_TIMER);
        }

        // @todo - Precise timings
        // @todo - When the CPU attempts to read a byte from ROM/RAM during a DMA transfer,
        // instead of the actual value at the given memory address,
        // the byte that is currently being transferred by the DMA transfer is returned.
        // This also affects the CPU when fetching opcodes, allowing for code execution through DMA transfers.
        // https://hacktix.github.io/GBEDG/dma/
        let mut cycles_left = cycles_passed;

        while cycles_left > 0 {
            let inc_count = if let Some(active_dma) = &self.active_dma {
                if active_dma.delay == 0 {
                    let c = u16::from(active_dma.count);

                    let dma_read_addr = match active_dma.src + c {
                        0xFE00..=0xFFFF => 0xC000 + ((active_dma.src + c) & 0x1FFF),
                        _ => active_dma.src + c,
                    };

                    debug_assert!(dma_read_addr < 0xFE00 || dma_read_addr > 0xFE9F);

                    let byte = self.bus_read(dma_read_addr);
                    self.ppu.oam_dma(0xFE00 + c, byte);
                }
                true
            } else {
                false
            };

            if inc_count {
                if let Some(active_dma) = &mut self.active_dma {
                    if active_dma.delay > 0 {
                        active_dma.delay -= 1;
                    } else {
                        active_dma.count += 1;
                        active_dma.cycles += 1;

                        if active_dma.count > 0x9F {
                            self.active_dma = None;
                        }
                    }
                }
            }

            if let Some(dma_request) = self.dma_request.take() {
                let dma = DmaTransfer{
                    src: util::value(dma_request, 0x0),
                    count: 0,
                     // @todo - Despite passing the tests, this timing is probably wrong.
                     // -> Sub-instruction clocking
                    delay: 2,
                    cycles: 1, // cycle 1 initialization delay
                };
                self.active_dma = Some(dma);
            }

            cycles_left -= 1;
        }

        self.mbc.step(cycles_passed);
        self.apu.step(cycles_passed);

        return vsync;
    }

    pub fn is_supported_cart_type(&self) -> bool {
        self.supported_carttype
    }

    fn calc_button_bits(&self, p1_val: u8) -> u8 {
        let select_buttons = (p1_val & (1 << 5)) == 0;
        let select_dpad = (p1_val & (1 << 4)) == 0;

        let button_bits: u8 = if select_buttons && select_dpad {
            0xF
        } else if select_buttons {
            let start   = (!self.buttons[GbButtonStart as usize] as u8) << 3;
            let select  = (!self.buttons[GbButtonSelect as usize] as u8) << 2;
            let b       = (!self.buttons[GbButtonB as usize] as u8) << 1;
            let a       = (!self.buttons[GbButtonA as usize] as u8) << 0;

            start | select | a | b
        } else if select_dpad {
            let down    = (!self.buttons[GbButtonDown as usize] as u8) << 3;
            let up      = (!self.buttons[GbButtonUp as usize] as u8) << 2;
            let left    = (!self.buttons[GbButtonLeft as usize] as u8) << 1;
            let right   = (!self.buttons[GbButtonRight as usize] as u8) << 0;

            down | up | left | right
        } else {
            0xF
        };

        return button_bits & 0xF;
    }

    // pub fn p1<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_P1, self) }
    // pub fn sb<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_SB, self) }
    // pub fn sc<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_SC, self) }
    // pub fn div_lsb<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_DIV_LSB, self) }
    // pub fn div<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_DIV, self) }
    // pub fn tima<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_TIMA, self) }
    // pub fn tma<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_TMA, self) }
    // pub fn tac<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_TAC, self) }
    pub fn r#if<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_IF, self) }
    // pub fn nr10<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR10, self) }
    // pub fn nr11<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR11, self) }
    // pub fn nr12<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR12, self) }
    // pub fn nr13<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR13, self) }
    // pub fn nr14<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR14, self) }
    // pub fn nr21<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR21, self) }
    // pub fn nr22<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR22, self) }
    // pub fn nr23<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR23, self) }
    // pub fn nr24<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR24, self) }
    // pub fn nr30<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR30, self) }
    // pub fn nr31<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR31, self) }
    // pub fn nr32<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR32, self) }
    // pub fn nr33<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR33, self) }
    // pub fn nr34<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR34, self) }
    // pub fn nr41<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR41, self) }
    // pub fn nr42<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR42, self) }
    // pub fn nr43<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR43, self) }
    // pub fn nr44<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR44, self) }
    // pub fn nr50<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR50, self) }
    // pub fn nr51<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR51, self) }
    // pub fn nr52<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR52, self) }
    // pub fn lcdc<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_LCDC, self) }
    // pub fn stat<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_STAT, self) }
    // pub fn ly<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_LY, self) }
    // pub fn scy<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_SCY, self) }
    // pub fn scx<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_SCX, self) }
    // pub fn lyc<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_LYC, self) }
    // pub fn dma<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_DMA, self) }
    // pub fn bgp<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_BGP, self) }
    // pub fn obp0<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_OBP0, self) }
    // pub fn obp1<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_OBP1, self) }
    // pub fn wy<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_WY, self) }
    // pub fn wx<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_WX, self) }
    pub fn ie<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_IE, self) }
}

impl MbcRomOnly {
    pub fn new() -> MbcRomOnly {
        Self { rom: vec![0; 0x8000] }
    }
}

impl MBC for MbcRomOnly {
    fn load(&mut self, cartridge: &Cartridge) {
        self.rom[0..0x8000].copy_from_slice(&cartridge.data[0..0x8000]);
    }

    fn read(&self, address: u16) -> u8 {
        return self.rom[usize::from(address)];
    }

    fn write(&mut self, _address: u16, _data: u8) {
        // noop
    }

    fn step(&mut self, _cycles: u8) {
        // noop
    }

    fn save(&mut self) {
        // noop
    }
}
