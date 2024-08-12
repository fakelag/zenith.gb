use std::{
    fs::File,
    io::{self, Seek},
};

use byteorder::ReadBytesExt;

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
    global_checksum: [u8; 2], // note: big endian
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

fn read_cartridge_header(mut rdr: impl ReadBytesExt + Seek) -> std::io::Result<CartridgeHeader> {
    rdr.seek(io::SeekFrom::Start(0x100)).unwrap();

    let mut header = CartridgeHeader::default();
    rdr.read_exact(&mut header.entrypoint).unwrap();
    rdr.read_exact(&mut header.logo).unwrap();
    rdr.read_exact(&mut header.title).unwrap();
    rdr.read_exact(&mut header.lic_code_new).unwrap();
    header.sgb_flag = rdr.read_u8().unwrap();
    header.cart_type = rdr.read_u8().unwrap();
    header.rom_size = rdr.read_u8().unwrap();
    header.ram_size = rdr.read_u8().unwrap();
    header.dst_code = rdr.read_u8().unwrap();
    header.lic_code_old = rdr.read_u8().unwrap();
    header.rom_version_mask = rdr.read_u8().unwrap();
    header.header_checksum = rdr.read_u8().unwrap();
    rdr.read_exact(&mut header.global_checksum).unwrap();

    let mut checksum: u8 = 0;
    rdr.seek(io::SeekFrom::Start(0x0134)).unwrap();
    for _address in 0..0x19 {
        let val = rdr.read_u8().unwrap();
        checksum = checksum.wrapping_sub(val).wrapping_sub(1);
    }
    header.header_checksum_verified = checksum;

    Ok(header)
}

fn main() {
    let file = File::open("rom/gb_helloworld.gb").unwrap();

    let hdr = read_cartridge_header(file).unwrap();

    println!("==== CARTRIDGE HEADER ====");
    println!("entrypoint={:x?}", hdr.entrypoint);
    println!("logo={:x?}", hdr.logo);
    println!(
        "title={:?}",
        String::from_utf8(hdr.title.to_vec())
            .unwrap_or_else(|err| format!("<Invalid UTF-8 title: {:?}>", err))
    );
    println!("lic_code_new={:x?}", hdr.lic_code_new);
    println!("sgb_flag={:x?}", hdr.sgb_flag);
    println!("cart_type={:x?}", hdr.cart_type);
    println!("rom_size={:x?}", hdr.rom_size);
    println!("ram_size={:x?}", hdr.ram_size);
    println!("dst_code={:x?}", hdr.dst_code);
    println!("lic_code_old={:x?}", hdr.lic_code_old);
    println!("rom_version_mask={:#02x}", hdr.rom_version_mask);
    println!("header_checksum={:#02x}", hdr.header_checksum);
    println!("global_checksum={:x?}", hdr.global_checksum);
    println!("==========================");
    println!("Verified checksum={:#02x}", hdr.header_checksum_verified);
    println!(
        "match: {:?}",
        if hdr.header_checksum_verified == hdr.header_checksum {
            "OK"
        } else {
            "MISMATCH"
        }
    );
}
