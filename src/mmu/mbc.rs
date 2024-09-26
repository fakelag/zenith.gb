use crate::cartridge::cartridge::CartridgeHeader;

const BYTES_1KIB: usize = 1024;

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

    Banks { num_banks, size_bytes }
}

pub fn rom_banks(hdr: &CartridgeHeader) -> Banks {
    let num_banks = 1 << (hdr.rom_size + 1);
    let size_bytes = (32 * BYTES_1KIB) * (1 << hdr.rom_size);

    Banks { num_banks, size_bytes }
}
