use crate::cartridge::cartridge::Cartridge;

use super::mmu;

const BYTES_8KIB: usize = 8 * 1024;
const BYTES_32KIB: usize = 32 * 1024;

pub struct MBC3 {
    rom: Vec<u8>,
    ram: Vec<u8>,

    // 7-bit rom bank number
    rom_bank: u8,
    // 2-bit ram bank number
    ram_bank: u8,
    ram_enabled: bool,

    has_rtc: bool,
    num_rom_banks: u8,

    cart_type: u8,
}

impl MBC3 {
    pub fn new() -> MBC3 {
        Self {
            rom: Vec::new(),
            ram: Vec::new(),
            rom_bank: 1,
            ram_bank: 0,
            num_rom_banks: 1,
            ram_enabled: false,
            has_rtc: false,
            cart_type: 0,
        }
    }
}

impl mmu::MBC for MBC3 {
    fn load(&mut self, cartridge: &Cartridge) {
        let hdr = &cartridge.header;

        self.cart_type = hdr.cart_type;

        let rom_size_bytes = (32 * 1024) * (1 << hdr.rom_size);
        debug_assert!(rom_size_bytes == cartridge.data.len());

        self.rom = cartridge.data[0..rom_size_bytes].to_vec();
        self.num_rom_banks = 1 << (hdr.rom_size + 1);

        let ram_size_bytes: usize = match hdr.ram_size {
            0..=1 => 0,
            2 => BYTES_8KIB,
            3 => BYTES_32KIB,
            _ => unreachable!(),
        };
        self.ram = vec![0; ram_size_bytes];
        self.has_rtc = [0x0F, 0x10].contains(&cartridge.header.cart_type);
    }

    fn read(&self, address: u16) -> u8 {
        match address {
            0x0..=0x3FFF => {
                return self.rom[usize::from(address)];
            }
            0x4000..=0x7FFF => {
                let rom_addr = 0x4000 * usize::from(self.rom_bank) + usize::from(address - 0x4000);
                return self.rom[rom_addr];
            }
            0xA000..=0xBFFF => {
                if !self.ram_enabled {
                    return 0xFF;
                }

                // @todo - Reading from a RTC register when its mapped

                let ram_addr = 0x2000 * usize::from(self.ram_bank) + usize::from(address - 0xA000);
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
                // Note: Also enables access to RTC
                self.ram_enabled = data & 0xF == 0xA;
            }
            0x2000..=0x3FFF => {
                let bank_num = data & 0x7F;

                if bank_num == 0 {
                    self.rom_bank = 1;
                } else {
                    self.rom_bank = bank_num % self.num_rom_banks;
                }
            }
            0x4000..=0x5FFF => {
                match data {
                    0..=0x3 => {
                        self.ram_bank = data;
                    }
                    0x8..=0x0C => {
                        // @todo - MAP RTC
                        unreachable!();
                    }
                    _ => {
                        // println!("write {address} - {data}");
                        // unreachable!();
                    }
                }
            }
            0x6000..=0x7FFF => {
                return;
               // println!("write {address} - {data}");
                // @todo - RTC Data Latch
               // unreachable!();
                // if data == 0 {

                // }
            }
            0xA000..=0xBFFF => {
                if !self.ram_enabled {
                    return;
                }

                // @todo - Writing to a RTC register when its mapped

                let ram_addr = 0x2000 * usize::from(self.ram_bank) + usize::from(address - 0xA000);
                self.ram[ram_addr] = data;
            }
            _ => {}
        }
    }
}