use std::{fs, path::Path};

use crate::cartridge::cartridge::{Cartridge, CartridgeHeader};

const BYTES_1KIB: usize = 1024;

pub trait MBC {
    fn load(&mut self, cartridge: &Cartridge);
    fn read(&self, address: u16) -> u8;
    fn write(&mut self, address: u16, data: u8);
    fn step(&mut self, cycles: u8);
    fn save(&mut self);
}

pub struct Banks {
    pub num_banks: usize,
    pub size_bytes: usize,
}

pub fn ram_banks(hdr: &CartridgeHeader) -> Banks {
    let num_banks: usize = match hdr.ram_size {
        0..=1 => 0,
        2 => 1,
        3 => 4,
        4 => 16,
        5 => 8,
        _ => unreachable!(),
    };

    let size_bytes = num_banks * 8 * BYTES_1KIB;

    Banks {
        num_banks,
        size_bytes,
    }
}

pub fn rom_banks(hdr: &CartridgeHeader) -> Banks {
    let num_banks = 1 << (hdr.rom_size + 1);
    let size_bytes = (32 * BYTES_1KIB) * (1 << hdr.rom_size);

    Banks {
        num_banks,
        size_bytes,
    }
}

pub fn read_save(save_path: &str, mbc_ram: &mut Vec<u8>) {
    if let Ok(data) = fs::read(save_path) {
        debug_assert!(data.len() == mbc_ram.len());
        *mbc_ram = data;
    }
}

pub fn write_save(save_path: &str, mbc_ram: &Vec<u8>) -> std::io::Result<()> {
    fs::write(save_path, mbc_ram)
}

pub fn save_file_from_rom_path(rom_path: &str) -> std::io::Result<String> {
    let path = Path::new(rom_path);

    if !path.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Rom path is not a file",
        ));
    }

    if let Some(ext) = path.extension() {
        if ext != "gb" {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Rom path is not a valid gb file",
            ));
        }
    } else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Invalid rom path",
        ));
    }

    if let Some(rom_file_name) = path.file_name() {
        let rom_file_name_string = rom_file_name
            .to_str()
            .expect("file name should be valid unicode string");

        if let Some(parent) = path.parent().clone() {
            let mut save_path = parent.to_path_buf();
            save_path.push(format!("{rom_file_name_string}.sav"));
            return Ok(save_path
                .into_os_string()
                .to_str()
                .expect("path must be valid unicode string")
                .to_string());
        } else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Invalid rom path",
            ));
        }
    }

    return Err(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        "Invalid rom path",
    ));
}

pub struct MbcRomOnly {
    rom: Vec<u8>,
}

impl MbcRomOnly {
    pub fn new() -> MbcRomOnly {
        Self {
            rom: vec![0; 0x8000],
        }
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

    fn save(&mut self) {
        // noop
    }
}
