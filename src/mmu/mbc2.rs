use crate::cartridge::cartridge::Cartridge;

use super::mmu;

pub struct MBC2 {
    rom: Vec<u8>,
    ram: Vec<u8>,
    num_rom_banks: u8,

    // 4-bit rom bank number
    rom_bank: u8,
    ram_enabled: bool,

    cart_type: u8,
}

impl MBC2 {
    pub fn new() -> MBC2 {
        Self {
            rom: Vec::new(),
            ram: Vec::new(),
            rom_bank: 1,
            num_rom_banks: 1,
            cart_type: 0,
            ram_enabled: false,
        }
    }
}

impl mmu::MBC for MBC2 {
    fn load(&mut self, cartridge: &Cartridge) {
        let hdr = &cartridge.header;

        self.cart_type = hdr.cart_type;

        let rom_size_bytes = (32 * 1024) * (1 << hdr.rom_size);

        debug_assert!(cartridge.data.len() == rom_size_bytes);

        self.rom = vec![0; rom_size_bytes];
        self.rom[0..cartridge.data.len()].clone_from_slice(&cartridge.data[0..cartridge.data.len()]);
        self.num_rom_banks = 1 << (hdr.rom_size + 1);

        self.ram = vec![0; 0x200];
    }

    fn read(&self, address: u16) -> u8 {
        match address {
            0x0..=0x3FFF => {
                return self.rom[usize::from(address)];
            }
            0x4000..=0x7FFF => {
                let rom_addr: usize = 0x4000 * usize::from(self.rom_bank) + (usize::from(address) - 0x4000);
                return self.rom[rom_addr as usize];
            }
            0xA000..=0xBFFF => {
                if !self.ram_enabled {
                    return 0xFF;
                }

                let ram_addr = address & 0x1FF;
                return (self.ram[usize::from(ram_addr)] & 0xF) | 0xF0;
            }
            _ => {
                unreachable!();
            }
        }
    }

    fn write(&mut self, address: u16, data: u8) {
        match address {
            0x0..=0x3FFF => {
                if address & (1 << 8) == 0 {
                    self.ram_enabled = (data & 0xF) == 0xA
                } else {
                    let select_bank = data & 0xF;
                    self.rom_bank = if select_bank == 0 { 1 } else { select_bank % self.num_rom_banks };
                }
            }
            0xA000..=0xBFFF => {
                if !self.ram_enabled {
                    return;
                }

                let ram_addr = address & 0x1FF;

                // Only lower 4 bits are written to RAM on MBC2
                self.ram[usize::from(ram_addr)] = data & 0xF;
            }
            _ => {}
        }
    }
}
