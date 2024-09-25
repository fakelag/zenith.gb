use crate::cartridge::cartridge::Cartridge;

use super::mmu;

const BYTES_1KIB: usize = 1024;

pub struct MBC5 {
    rom: Vec<u8>,
    ram: Vec<u8>,

    // 9-bit rom bank number
    rom_bank: usize,
    // 4-bit ram bank number
    ram_bank: usize,
    ram_enabled: bool,

    num_rom_banks: usize,
    num_ram_banks: u8,
    
    cart_type: u8,
    has_rumble: bool,
}

impl MBC5 {
    pub fn new() -> MBC5 {
        Self {
            rom: Vec::new(),
            ram: Vec::new(),
            rom_bank: 1,
            ram_bank: 0,
            num_rom_banks: 1,
            num_ram_banks: 1,
            ram_enabled: false,
            cart_type: 0,
            has_rumble: false,
        }
    }
}

impl mmu::MBC for MBC5 {
    fn load(&mut self, cartridge: &Cartridge) {
        let hdr = &cartridge.header;

        self.cart_type = hdr.cart_type;

        let rom_size_bytes = (32 * 1024) * (1 << hdr.rom_size);
        debug_assert!(rom_size_bytes == cartridge.data.len());

        self.rom = cartridge.data[0..rom_size_bytes].to_vec();
        self.num_rom_banks = 1 << (hdr.rom_size + 1);

        self.num_ram_banks = match hdr.ram_size {
            0..=1 => 0,
            2 => 1,
            3 => 4,
            4 => 16,
            5 => 8,
            _ => unreachable!(),
        };

        let ram_size_bytes = usize::from(self.num_ram_banks) * 8 * BYTES_1KIB;
        self.ram = vec![0; ram_size_bytes];

        self.has_rumble = [0x1C, 0x1D, 0x1E].contains(&cartridge.header.cart_type);
    }

    fn read(&self, address: u16) -> u8 {
        match address {
            0x0..=0x3FFF => {
                return self.rom[usize::from(address)];
            }
            0x4000..=0x7FFF => {
                let rom_addr = 0x4000 * self.rom_bank + usize::from(address - 0x4000);
                return self.rom[rom_addr];
            }
            0xA000..=0xBFFF => {
                if !self.ram_enabled {
                    return 0xFF;
                }

                let ram_addr = 0x2000 * self.ram_bank + usize::from(address - 0xA000);
                return self.ram[ram_addr];
            }
            _ => {
                unreachable!();
            }
        }
    }

    fn write(&mut self, address: u16, data: u8) {
        match address {
            0x0..=0x1FFF => {
                self.ram_enabled = data & 0xF == 0xA;
            }
            0x2000..=0x2FFF => {
                self.rom_bank = ((self.rom_bank & 0x100) | usize::from(data)) % self.num_rom_banks;
            }
            0x3000..=0x3FFF => {
                self.rom_bank = ((self.rom_bank & 0xFF) | (usize::from(data & 1) << 8)) % self.num_rom_banks;
            }
            0x4000..=0x5FFF => {
                self.ram_bank = (usize::from(data) & 0xF) % usize::from(self.num_ram_banks);
            }
            0xA000..=0xBFFF => {
                if !self.ram_enabled {
                    return;
                }

                let ram_addr = 0x2000 * self.ram_bank + usize::from(address - 0xA000);
                self.ram[ram_addr] = data;
            }
            _ => {
                unreachable!();
            }
        }
    }

    fn step(&mut self, _cycles: u8) {
        // noop
    }
}