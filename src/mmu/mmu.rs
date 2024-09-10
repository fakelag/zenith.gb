use crate::cartridge::cartridge::*;

use super::hw_reg::HwReg;

const HWR_P1: u16 = 0xFF00;
const HWR_LCDC: u16 = 0xFF40;
const HWR_STAT: u16 = 0xFF41;
const HWR_LY: u16 = 0xFF44;
const HWR_SCY: u16 = 0xFF42;
const HWR_SCX: u16 = 0xFF43;
const HWR_LYC: u16 = 0xFF45;

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

pub struct MMU {
    memory: [u8; 0x10000],
    access_flags: u8,
    access_origin: AccessOrigin,
}

impl MMU {
    pub fn new(cartridge: &Cartridge) -> MMU {
        let mut mmu = Self {
            memory: [0; 0x10000],
            access_flags: 0,
            access_origin: AccessOrigin::AccessOriginNone,
        };
        mmu.load(cartridge);
        mmu
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

        match address {
            0x8000..=0x9FFF => (self.access_flags & MemoryRegion::MemRegionVRAM as u8) == 0,
            0xFE00..=0xFE9F => (self.access_flags & MemoryRegion::MemRegionOAM as u8) == 0,
            _ => true
        }
    }

    pub fn set_access_origin(&mut self, origin: AccessOrigin) {
        self.access_origin = origin;
    }

    pub fn load(&mut self, cartridge: &Cartridge) {
        debug_assert!(cartridge.data.len() == 0x8000);
        self.memory[0..0x8000].copy_from_slice(&cartridge.data);
    }

    pub fn bus_read(&self, address: u16) -> u8 {
        if !self.address_accessible(address) {
            return 0xFF;
        }

        // https://gbdev.io/pandocs/Memory_Map.html
        match address {
            0x0000..=0x7FFF => {
                // @todo - Check cartridge type
                return self.memory[usize::from(address)];
            }
            0x8000..=0x9FFF => {
                // 8 KiB Video RAM (VRAM)
                return self.memory[usize::from(address)];
            }
            0xA000..=0xBFFF => {
                // 8 KiB External RAM - From cartridge, switchable bank if any
                todo!("External ram bank (if cart_type supports it)");
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
                if address == 0xFF00 {
                    println!("actual={}", self.memory[usize::from(address)]);
                    return 1;
                }
                // IO ranges
                return self.memory[usize::from(address)];
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
            0x0000..=0x7FFF => {}
            0x8000..=0x9FFF => {
                self.memory[usize::from(address)] = data;
            }
            0xA000..=0xBFFF => {
                todo!("External ram bank (if cart_type supports it)");
            }
            0xC000..=0xCFFF => {
                self.memory[usize::from(address)] = data;
            }
            0xD000..=0xDFFF => {
                self.memory[usize::from(address)] = data;
            }
            0xE000..=0xFDFF => {
                self.memory[usize::from(address - 0x2000)] = data;
                todo!("check echo ram");
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
                self.memory[usize::from(address)] = data;
            }
        }
    }

    fn bus_write_hwreg(&mut self, address: u16, data: u8) {
        if self.access_origin != AccessOrigin::AccessOriginCPU {
            self.memory[usize::from(address)] = data;
            return;
        }

        match address {
            HWR_P1 => {
                // Lower nibble RO
                let low_nibble = self.memory[usize::from(address)] & 0xF;
                self.memory[usize::from(address)] = (data & 0xF0) | low_nibble;
            }
            HWR_STAT => {
                // Lower 3 bits RO
                let low_3_bits = self.memory[usize::from(address)] & 0x7;
                self.memory[usize::from(address)] = (data & 0xF8) | low_3_bits;
            }
            0xFF4F => { todo!("select vram bank cgb"); }
            _ => {
                self.memory[usize::from(address)] = data;
            }
        }
    }

    pub fn p1<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_P1, self) }
    pub fn lcdc<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_LCDC, self) }
    pub fn stat<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_STAT, self) }
    pub fn ly<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_LY, self) }
    pub fn scy<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_SCY, self) }
    pub fn scx<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_SCX, self) }
    pub fn lyc<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(HWR_LYC, self) }
}
