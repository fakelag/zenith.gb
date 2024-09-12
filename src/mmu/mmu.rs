use crate::cartridge::cartridge::*;

use super::hw_reg::*;
use super::mbc1;

pub trait MBC {
    fn load(&mut self, cartridge: &Cartridge);
    fn read(&self, address: u16) -> u8;
    fn write(&mut self, address: u16, data: u8);
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

pub struct MMU {
    memory: [u8; 0x10000],
    access_flags: u8,
    access_origin: AccessOrigin,

    mbc: Box<dyn MBC>,
}

impl MMU {
    pub fn new(cartridge: &Cartridge) -> MMU {
        let mut mmu = Self {
            memory: [0; 0x10000],
            access_flags: 0,
            access_origin: AccessOrigin::AccessOriginNone,
            mbc: Box::new(MbcRomOnly::new()),
        };
        mmu.load(cartridge);
        mmu
    }

    pub fn load(&mut self, cartridge: &Cartridge) {
        self.memory = [0; 0x10000];

        match cartridge.header.cart_type {
            1..=3 => {
                self.mbc = Box::new(mbc1::MBC1::new());
                self.mbc.load(cartridge);
            }
            _ => {
                if cartridge.header.cart_type != 0 {
                    println!("WARN: Unsupported cartridge/MBC: {}", cartridge.header.cart_type);
                }
                self.mbc = Box::new(MbcRomOnly::new());
                self.mbc.load(cartridge);
            }
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

        match address {
            0x8000..=0x9FFF => (self.access_flags & MemoryRegion::MemRegionVRAM as u8) == 0,
            0xFE00..=0xFE9F => (self.access_flags & MemoryRegion::MemRegionOAM as u8) == 0,
            _ => true
        }
    }

    pub fn set_access_origin(&mut self, origin: AccessOrigin) {
        self.access_origin = origin;
    }

    pub fn bus_read(&self, address: u16) -> u8 {
        if self.access_origin != AccessOrigin::AccessOriginCPU {
            return self.memory[usize::from(address)];
        }

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
                match address {
                    HWR_NR13 => { return 0xFF; }
                    HWR_NR23 => { return 0xFF; }
                    HWR_NR31 => { return 0xFF; }
                    HWR_NR33 => { return 0xFF; }
                    HWR_NR41 => { return 0xFF; }
                    0xFF4D..=0xFF70 => {
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
            HWR_DIV_LSB => {
                // RO
            }
            HWR_DIV => {
                // Writing to 0xFF04 resets whole DIV clock
                self.memory[usize::from(HWR_DIV_LSB)] = 0;
                self.memory[usize::from(HWR_DIV)] = 0;

            }
            HWR_NR52 => {
                // Lower nibble RO
                // https://gbdev.io/pandocs/Audio_Registers.html#ff26--nr52-audio-master-control
                let low_nibble = self.memory[usize::from(address)] & 0xF;
                self.memory[usize::from(address)] = (data & 0xF0) | low_nibble;
            }
            HWR_STAT => {
                // Lower 3 bits RO
                let low_3_bits = self.memory[usize::from(address)] & 0x7;
                self.memory[usize::from(address)] = (data & 0xF8) | low_3_bits;
            }
            HWR_LY => {
                // RO
                return;
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
}
