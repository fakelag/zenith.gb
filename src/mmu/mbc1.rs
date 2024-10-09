use crate::{cartridge::cartridge::Cartridge, mmu::mbc};

const BYTES_8KIB: usize = 8 * 1024;
const BYTES_32KIB: usize = 32 * 1024;
const BYTES_1MIB: usize = 1024 * 1024;
const BYTES_2MIB: usize = 2 * BYTES_1MIB;

pub struct MBC1 {
    rom: Vec<u8>,
    ram: Vec<u8>,

    rom_mask: u8,
    // 5-bit rom bank number
    rom_bank: u8,
    // 2-bit ram bank number
    ram_bank: u8,
    mode_flag: bool,
    ram_enabled: bool,

    save_path: Option<String>,
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
            save_path: None,
        }
    }

    fn ram_address(&self, address: u16) -> usize {
        let ram_size = self.ram.len();
        return match (ram_size, self.mode_flag) {
            (BYTES_8KIB, _) => usize::from(address - 0xA000) % ram_size,
            (BYTES_32KIB, true) => {
                usize::from(0x2000 * u16::from(self.ram_bank) + (address - 0xA000))
            }
            (BYTES_32KIB, false) => usize::from(address - 0xA000),
            (0, _) => panic!("ram size 0"),
            _ => unreachable!(),
        };
    }
}

impl mbc::MBC for MBC1 {
    fn load(&mut self, cartridge: &Cartridge) {
        let hdr = &cartridge.header;

        let rom_banks = mbc::rom_banks(hdr);
        debug_assert!(rom_banks.size_bytes == cartridge.data.len());

        self.rom_mask = 0xFF >> std::cmp::max(7 - hdr.rom_size, 3);
        self.rom = cartridge.data[0..rom_banks.size_bytes].to_vec();

        if hdr.cart_type == 2 || hdr.cart_type == 3 {
            let ram_banks = mbc::ram_banks(hdr);
            self.ram = vec![0; ram_banks.size_bytes];
        } else if hdr.ram_size != 0 {
            panic!(
                "cart_type={} should have ram size 0 (has {})",
                hdr.cart_type, hdr.ram_size
            );
        }

        match hdr.cart_type {
            0x3 => {
                if let Ok(save_path) = mbc::save_file_from_rom_path(&cartridge.rom_path) {
                    mbc::read_save(&save_path, &mut self.ram);
                    self.save_path = Some(save_path);
                }
            }
            _ => {}
        }
    }

    fn read(&self, address: u16) -> u8 {
        match address {
            0x0..=0x3FFF => {
                if self.mode_flag {
                    let zero_bank_number: usize = match self.rom.len() {
                        // @todo - Multicart support (MBC1M)
                        BYTES_1MIB => (usize::from(self.ram_bank) & 0x1) << 5,
                        BYTES_2MIB => (usize::from(self.ram_bank) & 0x3) << 5,
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
                    BYTES_1MIB => {
                        (usize::from(self.rom_bank & self.rom_mask))
                            | ((usize::from(self.ram_bank) & 0x1) << 5)
                    }
                    BYTES_2MIB => {
                        (usize::from(self.rom_bank & self.rom_mask))
                            | ((usize::from(self.ram_bank) & 0x3) << 5)
                    }
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

    fn clock(&mut self) {
        // noop
    }

    fn save(&mut self) {
        if let Some(save_path) = &self.save_path {
            _ = mbc::write_save(save_path, &self.ram);
        }
    }
}
