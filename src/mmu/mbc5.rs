use crate::{cartridge::cartridge::Cartridge, mmu::mbc};

pub struct MBC5 {
    rom: Vec<u8>,
    ram: Vec<u8>,

    // 9-bit rom bank number
    rom_bank: usize,
    // 4-bit ram bank number
    ram_bank: usize,
    ram_enabled: bool,

    num_rom_banks: usize,
    num_ram_banks: usize,

    save_path: Option<String>,
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
            save_path: None,
        }
    }
}

impl mbc::MBC for MBC5 {
    fn load(&mut self, cartridge: &Cartridge) {
        let hdr = &cartridge.header;

        let rom_banks = mbc::rom_banks(hdr);
        let ram_banks = mbc::ram_banks(hdr);
        debug_assert!(rom_banks.size_bytes == cartridge.data.len());

        self.rom = cartridge.data[0..rom_banks.size_bytes].to_vec();
        self.num_rom_banks = rom_banks.num_banks;

        self.num_ram_banks = ram_banks.num_banks;
        self.ram = vec![0; ram_banks.size_bytes];

        match hdr.cart_type {
            0x1B | 0x1E => {
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
                self.rom_bank =
                    ((self.rom_bank & 0xFF) | (usize::from(data & 1) << 8)) % self.num_rom_banks;
            }
            0x4000..=0x5FFF => {
                self.ram_bank = (usize::from(data) & 0xF) % self.num_ram_banks;
            }
            0xA000..=0xBFFF => {
                if !self.ram_enabled {
                    return;
                }

                let ram_addr = 0x2000 * self.ram_bank + usize::from(address - 0xA000);
                self.ram[ram_addr] = data;
            }
            _ => {}
        }
    }

    fn step(&mut self, _cycles: u8) {
        // noop
    }

    fn save(&mut self) {
        if let Some(save_path) = &self.save_path {
            _ = mbc::write_save(save_path, &self.ram);
        }
    }
}
