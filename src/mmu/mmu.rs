use crate::cartridge::cartridge::*;

use super::hw_reg::HwReg;

pub struct MMU {
    pub memory: [u8; 0x10000],
}

impl MMU {
    pub fn new(cartridge: &Cartridge) -> MMU {
        let mut mmu = Self { memory: [0; 0x10000] };
        mmu.load(cartridge);
        mmu
    }

    pub fn load(&mut self, cartridge: &Cartridge) {
        debug_assert!(cartridge.data.len() == 0x8000);
        self.memory[0..0x8000].copy_from_slice(&cartridge.data);
    }

    pub fn bus_read(&self, address: u16) -> u8 {
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
                todo!("Implement echo ram");
            }
            0xFE00..=0xFE9F => {
                // OAM - https://gbdev.io/pandocs/OAM.html#object-attribute-memory-oam
                return self.memory[usize::from(address)];
            }
            0xFEA0..=0xFEFF => {
                // unused
                unreachable!();
            }
            0xFF00..=0xFF7F => {
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
        match address {
            0x0000..=0x7FFF => {
                // println!("address={} data={}", address, data);
                // unreachable!();
                // self.memory[usize::from(address)] = data;
            }
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
                todo!("Implement echo ram");
            }
            0xFE00..=0xFE9F => {
                self.memory[usize::from(address)] = data;
            }
            0xFEA0..=0xFEFF => {
                // unused
                // unreachable!();
            }
            0xFF00..=0xFF7F => {
                if address == 0xFF46 {
                    // println!("dma transfer: {}", data);
                } else if address == 0xFF4F {
                    todo!("vram bank select cgb");
                } else if (address < 0xFF40 || address > 0xFF45) && address != 65317 /* nr44 */ {
                    // println!("address={} data={}", address, data);
                }
                self.memory[usize::from(address)] = data;
            }
            0xFF80..=0xFFFE => {
                self.memory[usize::from(address)] = data;
            }
            0xFFFF => {
                self.memory[usize::from(address)] = data;
            }
        }
    }

    pub fn lcdc<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(0xFF40, self) }
    pub fn stat<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(0xFF41, self) }
    pub fn ly<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(0xFF44, self) }
    pub fn scy<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(0xFF42, self) }
    pub fn scx<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(0xFF43, self) }
    pub fn lyc<'a>(&'a mut self) -> HwReg<'a> { HwReg::<'a>::new(0xFF45, self) }
}
