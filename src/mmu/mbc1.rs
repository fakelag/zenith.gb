use crate::cartridge::cartridge::Cartridge;

use super::mmu;

const BYTES_8KIB: usize = 8 * 1024;
const BYTES_32KIB: usize = 32 * 1024;
const BYTES_1MIB: usize = 1024 * 1024;
const BYTES_2MIB: usize = 2 * BYTES_1MIB;

pub struct MBC1 {
    rom: Vec<u8>,
    ram: Vec<u8>,

    rom_mask: u8,
    rom_bank: u8,
    ram_bank: u8,
    mode_flag: bool,
    ram_enabled: bool,

    cart_type: u8,
}

impl MBC1 {
    pub fn new() -> MBC1 {
        Self {
            rom: Vec::new(),
            ram: Vec::new(),
            rom_mask: 0,
            rom_bank: 1,
            ram_bank: 0,
            mode_flag: false,
            ram_enabled: false,
            cart_type: 0,
        }
    }

    fn ram_address(&self, address: u16) -> usize {
        let ram_size = self.ram.len();
        return match (ram_size, self.mode_flag) {
            (BYTES_8KIB, _) => usize::from(address - 0xA000) % ram_size,
            (BYTES_32KIB, true) => usize::from(0x2000 * u16::from(self.ram_bank) + (address - 0xA000)),
            (BYTES_32KIB, false) => usize::from(address - 0xA000),
            (0, _) => panic!("ram size 0"),
            _ => unreachable!(),
        };
    }
}

impl mmu::MBC for MBC1 {
    fn load(&mut self, cartridge: &Cartridge) {
        let hdr = &cartridge.header;

        self.cart_type = hdr.cart_type;

        let rom_size_bytes = (32 * 1024) * (1 << hdr.rom_size);
        debug_assert!(rom_size_bytes == cartridge.data.len());

        self.rom_mask = 0xFF >> std::cmp::max(7 - hdr.rom_size, 3);
        self.rom = cartridge.data[0..rom_size_bytes].to_vec();

        if hdr.cart_type == 2 || hdr.cart_type == 3 {
            let ram_size_bytes: usize = match hdr.ram_size {
                0..=1 => 0,
                2 => BYTES_8KIB,
                3 => BYTES_32KIB,
                _ => unreachable!(),
            };
            self.ram = vec![0; ram_size_bytes];
        } else if hdr.ram_size != 0 {
            panic!("cart_type={} should have ram size 0 (has {})", hdr.cart_type, hdr.ram_size);
        }
    }

    fn read(&self, address: u16) -> u8 {
        match address {
            0x0..=0x3FFF => {
                if self.mode_flag {
                    let zero_bank_number: usize = match self.rom.len() {
                        // @todo - Multicart support (MBC1M)
                        BYTES_1MIB => { (usize::from(self.ram_bank) & 0x1) << 5 }
                        BYTES_2MIB => { (usize::from(self.ram_bank) & 0x3) << 5 }
                        _ => 0,
                    };

                    let addr: usize = 0x4000 * zero_bank_number + usize::from(address);
                    return self.rom[addr];
                } else {
                    return self.rom[usize::from(address)];
                }
            }
            0x4000..=0x7FFF => {
                let high_bank_number: usize = match self.rom.len() {
                    BYTES_1MIB => { (usize::from(self.rom_bank & self.rom_mask)) | ((usize::from(self.ram_bank) & 0x1) << 5) }
                    BYTES_2MIB => { (usize::from(self.rom_bank & self.rom_mask)) | ((usize::from(self.ram_bank) & 0x3) << 5) }
                    _ => usize::from(self.rom_bank),
                };

                return self.rom[0x4000 * high_bank_number + usize::from(address - 0x4000)];
            }
            0xA000..=0xBFFF => {
                if !self.ram_enabled {
                    return 0xFF;
                }

                let ram_addr = self.ram_address(address);
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
            0x2000..=0x3FFF => {
                let bank_num = data & 0x1F;

                if bank_num == 0 {
                    self.rom_bank = 1;
                } else {
                    self.rom_bank = bank_num & self.rom_mask;
                }
            }
            0x4000..=0x5FFF => {
                self.ram_bank = data & 0x3;
            }
            0x6000..=0x7FFF => {
                self.mode_flag = data & 0x1 != 0;
            }
            0xA000..=0xBFFF => {
                if !self.ram_enabled {
                    return;
                }

                let ram_addr = self.ram_address(address);
                self.ram[ram_addr] = data;
            }
            _ => {}
        }
    }
}