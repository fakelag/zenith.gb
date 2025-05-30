use std::fs;

pub struct Cartridge {
    pub data: Vec<u8>,
    pub header: CartridgeHeader,
    pub rom_path: String,
}

impl Cartridge {
    pub fn new(file_path: &str) -> Self {
        let data = fs::read(file_path).unwrap();
        let header = CartridgeHeader::new(&data);
        Self {
            data,
            header,
            rom_path: file_path.to_string(),
        }
    }
}

#[derive(Debug)]
pub struct CartridgeHeader {
    pub entrypoint: [u8; 4],
    pub logo: [u8; 48],
    pub title: [u8; 16],
    pub cgb_flag: u8,
    pub lic_code_new: [u8; 2],
    pub sgb_flag: u8,
    pub cart_type: u8,
    pub rom_size: u8,
    pub ram_size: u8,
    pub dst_code: u8,
    pub lic_code_old: u8,
    pub rom_version_mask: u8,
    pub header_checksum: u8,
    pub global_checksum: [u8; 2],
    pub header_checksum_verified: u8,
}

impl Default for CartridgeHeader {
    fn default() -> Self {
        Self {
            entrypoint: [0; 4],
            logo: [0; 48],
            title: [0; 16],
            cgb_flag: 0,
            lic_code_new: [0; 2],
            sgb_flag: 0,
            cart_type: 0,
            rom_size: 0,
            ram_size: 0,
            dst_code: 0,
            lic_code_old: 0,
            rom_version_mask: 0,
            header_checksum: 0,
            global_checksum: [0, 2],
            header_checksum_verified: 0,
        }
    }
}

impl CartridgeHeader {
    pub fn new(data: &Vec<u8>) -> Self {
        let mut hdr = CartridgeHeader::default();

        hdr.entrypoint = data[0x100..0x104].try_into().unwrap();
        hdr.logo = data[0x104..0x134].try_into().unwrap();
        hdr.title = data[0x134..0x144].try_into().unwrap();
        hdr.cgb_flag = data[0x143].try_into().unwrap();
        hdr.lic_code_new = data[0x144..0x146].try_into().unwrap();
        hdr.sgb_flag = data[0x146].try_into().unwrap();
        hdr.cart_type = data[0x147].try_into().unwrap();
        hdr.rom_size = data[0x148].try_into().unwrap();
        hdr.ram_size = data[0x149].try_into().unwrap();
        hdr.dst_code = data[0x14A].try_into().unwrap();
        hdr.lic_code_old = data[0x14B].try_into().unwrap();
        hdr.rom_version_mask = data[0x14C].try_into().unwrap();
        hdr.header_checksum = data[0x14D].try_into().unwrap();
        hdr.global_checksum = data[0x14E..0x150].try_into().unwrap();

        let mut checksum: u8 = 0;
        for address in 0x0134..0x014D {
            checksum = checksum.wrapping_sub(data[address]).wrapping_sub(1);
        }
        hdr.header_checksum_verified = checksum;

        if hdr.header_checksum_verified != hdr.header_checksum {
            panic!("Invalid header checksum");
        }

        hdr
    }

    pub fn is_cgb(&self) -> bool {
        self.cgb_flag & 0x80 != 0
    }
}
