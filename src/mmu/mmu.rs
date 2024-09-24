use crate::cartridge::cartridge::*;
use crate::cpu::cpu;
use crate::emu::emu::{self, GbButton::*};
use crate::util::util;

use super::hw_reg::*;
use super::mbc1;
use super::mbc2;
use super::mbc3;

pub trait MBC {
    fn load(&mut self, cartridge: &Cartridge);
    fn read(&self, address: u16) -> u8;
    fn write(&mut self, address: u16, data: u8);
    fn step(&mut self, cycles: u8);
}

struct MbcRomOnly {
    rom: [u8; 0x8000],
}

#[repr(u8)]
#[derive(Copy, Clone, PartialEq)]
pub enum MemoryRegion {
    MemRegionOAM = (1 << 0),
    MemRegionVRAM = (1 << 1),
}

#[repr(u8)]
#[derive(Copy, Clone, PartialEq)]
pub enum AccessOrigin {
    AccessOriginNone,
    AccessOriginPPU,
    AccessOriginCPU
}

pub struct DmaTransfer {
    pub src: u16,
    pub count: u8,
    pub delay: u8,
    pub cycles: u16, // 160 + 1
}

pub struct MMU {
    memory: [u8; 0x10000],
    access_flags: u8,
    access_origin: AccessOrigin,
    supported_carttype: bool,

    mbc: Box<dyn MBC>,

    dma_request: Option<u8>,
    pub active_dma: Option<DmaTransfer>,

    buttons: [bool; emu::GbButton::GbButtonMax as usize],
}

impl MMU {
    pub fn new(cartridge: &Cartridge) -> MMU {
        let mut mmu = Self {
            memory: [0; 0x10000],
            access_flags: 0,
            access_origin: AccessOrigin::AccessOriginNone,
            mbc: Box::new(MbcRomOnly::new()),
            active_dma: None,
            dma_request: None,
            buttons: [false; emu::GbButton::GbButtonMax as usize],
            supported_carttype: true,
        };
        mmu.load(cartridge);
        mmu
    }

    pub fn load(&mut self, cartridge: &Cartridge) {
        self.memory = [0; 0x10000];

        // println!("Cart type={}", cartridge.header.cart_type);

        // @todo - If cartridge header has +BATTERY, MBCs should store
        // state of their RAM to a file and load it on boot

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

    pub fn update_input(&mut self, input_event: emu::InputEvent) {
        let was_down = self.buttons[input_event.button as usize];

        self.buttons[input_event.button as usize] = input_event.down;

        if !was_down && input_event.down {
            let flags_if = self.r#if().get();
            self.r#if().set(flags_if | cpu::INTERRUPT_BIT_JOYPAD);
        }
    }

    pub fn lock_region(&mut self, region: u8) {
        self.access_flags |= region;
    }

    pub fn unlock_region(&mut self, region: u8) {
        self.access_flags &= !region;
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
            0x8000..=0x9FFF => (self.access_flags & MemoryRegion::MemRegionVRAM as u8) == 0,
            0xFE00..=0xFE9F => (self.access_flags & MemoryRegion::MemRegionOAM as u8) == 0 && !self.active_dma.is_some(),
            _ => true
        }
    }

    pub fn set_access_origin(&mut self, origin: AccessOrigin) {
        self.access_origin = origin;
    }

    pub fn bus_read(&self, address: u16) -> u8 {
        let cpu_access = self.access_origin == AccessOrigin::AccessOriginCPU;

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
                return self.memory[usize::from(address)];
            }
            0xA000..=0xBFFF => {
                return self.mbc.read(address);
            }
            0xC000..=0xCFFF => {
                // 4 KiB Work RAM (WRAM)
                return self.memory[usize::from(address)];
            }
            0xD000..=0xDFFF => {
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
                return self.memory[usize::from(address)];
            }
            0xFEA0..=0xFEFF => {
                // Prohibited memory.
                // @todo - On DMG triggers OAM corruption
                return 0;
            }
            0xFF00..=0xFF7F => {
                if !cpu_access {
                    return self.memory[usize::from(address)];
                }

                match address {
                    HWR_P1 => {
                        let p1 = self.memory[address as usize];

                        let button_bits = self.calc_button_bits(p1);
                        return button_bits | (p1 & 0xF0);
                    }
                    HWR_DIV_LSB => { return 0xFF; }
                    0xFF08..=0xFF0E => { return 0xFF; }
                    HWR_NR13 => { return 0xFF; }
                    0xFF15 => { return 0xFF; }
                    HWR_NR23 => { return 0xFF; }
                    HWR_NR31 => { return 0xFF; }
                    HWR_NR33 => { return 0xFF; }
                    0xFF1F => { return 0xFF; }
                    HWR_NR41 => { return 0xFF; }
                    0xFF27..=0xFF2F => { return 0xFF; }
                    0xFF4C => { return 0xFF; }
                    0xFF4D..=0xFF7F => {
                        // Reads ignored for non-dmg registers
                        return 0xFF;
                    }
                    _ => {
                        // IO ranges
                        return self.memory[usize::from(address)];
                    }
                }
            }
            // IO ranges & HRAM https://gbdev.io/pandocs/Hardware_Reg_List.html
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
                self.memory[usize::from(address)] = data;
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
                self.memory[usize::from(address)] = data;
            }
            0xFEA0..=0xFEFF => {
                // unused
                // unreachable!();
            }
            0xFF00..=0xFF7F => {
                self.bus_write_hwreg(address, data);
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

    pub fn step(&mut self, cycles_passed: u8) {
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
                    let byte = self.bus_read(active_dma.src + c);
                    self.bus_write(0xFE00 + c, byte);
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
    }

    pub fn is_supported_cart_type(&self) -> bool {
        self.supported_carttype
    }
    
    fn bus_write_hwreg(&mut self, address: u16, data: u8) {
        if self.access_origin != AccessOrigin::AccessOriginCPU {
            self.memory[usize::from(address)] = data;
            return;
        }

        match address {
            HWR_P1 => {
                // Lower nibble RO
                let ro_bits = self.memory[usize::from(address)] & 0xCF;
                self.memory[usize::from(address)] = (data & 0x30) | ro_bits;
            }
            HWR_SC => {}
            HWR_DIV_LSB => {
                // RO
            }
            HWR_DIV => {
                // Writing to 0xFF04 resets whole DIV clock
                self.memory[usize::from(HWR_DIV_LSB)] = 0;
                self.memory[usize::from(HWR_DIV)] = 0;
            }
            HWR_TAC => {
                // Top 5 bits unused
                let ro_bits = self.memory[usize::from(address)] & 0xF8;
                self.memory[usize::from(address)] = (data & 0x7) | ro_bits;
            }
            HWR_IF => {
                // Top 3 bits unused
                let ro_bits = self.memory[usize::from(address)] & 0xE0;
                self.memory[usize::from(address)] = (data & 0x1F) | ro_bits;
            }
            HWR_NR10 => {
                // bit 7 unused
                let ro_bits = self.memory[usize::from(address)] & 0x80;
                self.memory[usize::from(address)] = (data & 0x7F) | ro_bits;
            }
            HWR_NR30 => {
                // Lower 6 bits unused
                let ro_bits = self.memory[usize::from(address)] & 0x7F;
                self.memory[usize::from(address)] = (data & 0x80) | ro_bits;
            }
            HWR_NR32 => {
                // Bits 7 & lower 5 bits unused
                let ro_bits = self.memory[usize::from(address)] & 0x9F;
                self.memory[usize::from(address)] = (data & 0x60) | ro_bits;
            }
            HWR_NR44 => {
                // Lower 5 bits unused
                let ro_bits = self.memory[usize::from(address)] & 0x3F;
                self.memory[usize::from(address)] = (data & 0xC0) | ro_bits;
            }
            HWR_NR52 => {
                // Bits 6-4 unused, lower nibble RO
                // https://gbdev.io/pandocs/Audio_Registers.html#ff26--nr52-audio-master-control
                let ro_bits = self.memory[usize::from(address)] & 0x7F;
                self.memory[usize::from(address)] = (data & 0x80) | ro_bits;
            }
            HWR_STAT => {
                // Bit 7 unused, lower 3 bits RO
                let ro_bits = self.memory[usize::from(address)] & 0x87;
                self.memory[usize::from(address)] = (data & 0x78) | ro_bits;
            }
            HWR_LY => {
                // RO
                return;
            }
            HWR_DMA => {
                self.dma_request = Some(data);
                self.memory[usize::from(address)] = data;
            }
            // 0xFF4F => { todo!("select vram bank cgb"); }
            0xFF4D..=0xFF70 => {
                // Writes ignored for non DMG registers
            }
            _ => {
                self.memory[usize::from(address)] = data;
            }
        }
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

    pub fn p1<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_P1, self) }
    pub fn sb<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_SB, self) }
    pub fn sc<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_SC, self) }
    pub fn div_lsb<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_DIV_LSB, self) }
    pub fn div<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_DIV, self) }
    pub fn tima<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_TIMA, self) }
    pub fn tma<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_TMA, self) }
    pub fn tac<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_TAC, self) }
    pub fn r#if<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_IF, self) }
    pub fn nr10<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR10, self) }
    pub fn nr11<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR11, self) }
    pub fn nr12<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR12, self) }
    pub fn nr13<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR13, self) }
    pub fn nr14<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR14, self) }
    pub fn nr21<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR21, self) }
    pub fn nr22<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR22, self) }
    pub fn nr23<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR23, self) }
    pub fn nr24<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR24, self) }
    pub fn nr30<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR30, self) }
    pub fn nr31<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR31, self) }
    pub fn nr32<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR32, self) }
    pub fn nr33<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR33, self) }
    pub fn nr34<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR34, self) }
    pub fn nr41<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR41, self) }
    pub fn nr42<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR42, self) }
    pub fn nr43<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR43, self) }
    pub fn nr44<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR44, self) }
    pub fn nr50<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR50, self) }
    pub fn nr51<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR51, self) }
    pub fn nr52<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_NR52, self) }
    pub fn lcdc<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_LCDC, self) }
    pub fn stat<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_STAT, self) }
    pub fn ly<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_LY, self) }
    pub fn scy<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_SCY, self) }
    pub fn scx<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_SCX, self) }
    pub fn lyc<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_LYC, self) }
    pub fn dma<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_DMA, self) }
    pub fn bgp<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_BGP, self) }
    pub fn obp0<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_OBP0, self) }
    pub fn obp1<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_OBP1, self) }
    pub fn wy<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_WY, self) }
    pub fn wx<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_WX, self) }
    pub fn ie<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_IE, self) }
}

impl MbcRomOnly {
    pub fn new() -> MbcRomOnly {
        Self { rom: [0; 0x8000] }
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
}
