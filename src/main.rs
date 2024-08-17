use std::{
    fmt, fs::{self, File},
};

fn high(value: u16) -> u8 {
    (value >> 8) as u8
}

fn low(value: u16) -> u8 {
    (value & 0xFF) as u8
}

struct CPU {
    af: u16,
    bc: u16,
    de: u16,
    hl: u16,
    sp: u16,
    pc: u16,
}

impl CPU {
    fn new() -> Self {
        Self {
            af: 0,
            bc: 0,
            de: 0,
            hl: 0,
            sp: 0,
            pc: 0,
        }
    }
}

struct Emu {
    cart: Cartridge,
    cpu: CPU,
}

impl Emu {
    fn new(cart: Cartridge) -> Self {
        Self { cart, cpu: CPU::new() }
    }
    fn bus_read(self: &Emu, address: u16) -> u8 {
        // https://gbdev.io/pandocs/Memory_Map.html
        match address {
            0x0000..=0x7FFF => {
                // @todo - Check cartridge type
                return self.cart.data[usize::from(address)];
            }
            _ => {
                todo!();
            }
        }
    }

    fn bus_write(self: &Emu, address: u16, data: u8) -> u8 {
        match address {
            0x0000..=0x7FFF => {
                todo!()
            }
            _ => {
                todo!();
            }
        }
    }

    fn run(self: &mut Emu) {

    }
}

struct Cartridge {
    data: Vec<u8>,
    header: CartridgeHeader,
}

impl Cartridge {
    fn new(file_path: &str) -> Self {
        let data = fs::read(file_path).unwrap();
        let header = read_cartridge_header(&data).unwrap();
        Self {
            data,
            header,
        }
    }
}

#[derive(Debug)]
struct CartridgeHeader {
    entrypoint: [u8; 4],
    logo: [u8; 48],
    title: [u8; 16],
    lic_code_new: [u8; 2],
    sgb_flag: u8,
    cart_type: u8,
    rom_size: u8,
    ram_size: u8,
    dst_code: u8,
    lic_code_old: u8,
    rom_version_mask: u8,
    header_checksum: u8,
    global_checksum: [u8; 2],
    header_checksum_verified: u8,
}

impl Default for CartridgeHeader {
    fn default() -> Self {
        Self {
            entrypoint: [0; 4],
            logo: [0; 48],
            title: [0; 16],
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

fn read_cartridge_header(data: &Vec<u8>) -> std::io::Result<CartridgeHeader> {
    let mut hdr = CartridgeHeader::default();

    hdr.entrypoint = data[0x100..0x104].try_into().unwrap();
    hdr.logo = data[0x104..0x134].try_into().unwrap();
    hdr.title = data[0x134..0x144].try_into().unwrap();
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

    Ok(hdr)
}

fn main() {
    let cart = Cartridge::new("rom/gb_helloworld.gb");

    let mut emu = Emu::new(cart);

    println!("bus_read={:?}", emu.bus_read(0x14D));

    emu.run();

}
