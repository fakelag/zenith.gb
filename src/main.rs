use std::{
    fmt::{self, Display}, fs::{self},
};

fn get_high(value: u16) -> u8 {
    (value >> 8) as u8
}

fn get_low(value: u16) -> u8 {
    (value & 0xFF) as u8
}

fn set_high(dst: &mut u16, value: u8) -> u16 {
    *dst = (*dst & 0xFF) | (u16::from(value) << 8);
    *dst
}

fn set_low(dst: &mut u16, value: u8) -> u16 {
    *dst = (*dst & 0xFF00) | u16::from(value);
    *dst
}

fn value(high: u8, low: u8) -> u16 {
    (u16::from(high) << 8) | u16::from(low)
}

#[derive(Debug)]
struct CPU {
    af: u16,
    bc: u16,
    de: u16,
    hl: u16,
    sp: u16,
    pc: u16,
    cycles: u64,
}

impl Display for CPU {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "CPU")?;
        writeln!(f, "af={value:#06x} [{value:016b}], a={high:#x?} [{high:08b}], f={low:#x?} [{low:08b}]",
            value = self.af, high = get_high(self.af), low = get_low(self.af))?;
        writeln!(f, "bc={value:#06x} [{value:016b}], b={high:#x?} [{high:08b}], c={low:#x?} [{low:08b}]",
            value = self.bc, high = get_high(self.bc), low = get_low(self.bc))?;
        writeln!(f, "de={value:#06x} [{value:016b}], d={high:#x?} [{high:08b}], e={low:#x?} [{low:08b}]",
            value = self.de, high = get_high(self.de), low = get_low(self.de))?;
        writeln!(f, "hl={value:#06x} [{value:016b}], h={high:#x?} [{high:08b}], l={low:#x?} [{low:08b}]",
            value = self.hl, high = get_high(self.hl), low = get_low(self.hl))?;
        writeln!(f, "sp={value:#06x} [{value:016b}]", value = self.sp)?;
        writeln!(f, "pc={value:#06x} [{value:016b}]", value = self.pc)?;
        writeln!(f, "cycles={value}", value = self.cycles)?;
        Ok(())
    }
}

impl CPU {
    fn new() -> Self {
        Self {
            af: 0,
            bc: 0,
            de: 0,
            hl: 0,
            sp: 0,
            pc: 0x100,
            cycles: 0,
        }
    }
}

struct Emu {
    cart: Cartridge,
    cpu: CPU,
    // Main memory - indexed directly
    // https://gbdev.io/pandocs/Memory_Map.html
    memory: [u8; 0xFFFF],
}

impl Emu {
    fn new(cart: Cartridge) -> Self {
        Self { cart, cpu: CPU::new(), memory: [0; 0xFFFF] }
    }
    fn bus_read(self: &Emu, address: u16) -> u8 {
        // https://gbdev.io/pandocs/Memory_Map.html
        match address {
            0x0000..=0x7FFF => {
                // @todo - Check cartridge type
                return self.cart.data[usize::from(address)];
            }
            0x8000..=0x9FFF => {
                // 8 KiB Video RAM (VRAM)
                return self.memory[usize::from(address)];
            }
            0xA000..=0xBFFF => {
                // 8 KiB External RAM - From cartridge, switchable bank if any
                todo!("External ram bank (if cart_type supports it)");
            }
            0xC000..=0xCFFF => {
                // 4 KiB Work RAM (WRAM)
                return self.memory[usize::from(address)];
            }
            0xD000..=0xDFFF => {
                // 4 KiB Work RAM (WRAM) - In CGB mode, switchable bank 1–7
                return self.memory[usize::from(address)];
            }
            0xE000..=0xFDFF => {
                // Echo RAM
                todo!("Implement echo ram");
            }
            0xFE00..=0xFE9F => {
                // OAM - https://gbdev.io/pandocs/OAM.html#object-attribute-memory-oam
                return self.memory[usize::from(address)];
            }
            0xFEA0..=0xFEFF => {
                // unused
                unreachable!();
            }
            0xFF00..=0xFF7F => {
                // IO ranges
                return self.memory[usize::from(address)];
            }
            // IO ranges & HRAM https://gbdev.io/pandocs/Hardware_Reg_List.html
            0xFF80..=0xFFFE => {
                // HRAM
                return self.memory[usize::from(address)];
            }
            0xFFFF => {
                // Interrupt
                todo!();
            }
        }
    }

    fn bus_write(self: &mut Emu, address: u16, data: u8) {
        match address {
            0x0000..=0x7FFF => {
                unreachable!()
            }
            0x8000..=0x9FFF => {
                self.memory[usize::from(address)] = data;
            }
            0xA000..=0xBFFF => {
                todo!("External ram bank (if cart_type supports it)");
            }
            0xC000..=0xCFFF => {
                self.memory[usize::from(address)] = data;
            }
            0xD000..=0xDFFF => {
                self.memory[usize::from(address)] = data;
            }
            0xE000..=0xFDFF => {
                todo!("Implement echo ram");
            }
            0xFE00..=0xFE9F => {
                self.memory[usize::from(address)] = data;
            }
            0xFEA0..=0xFEFF => {
                // unused
                unreachable!();
            }
            0xFF00..=0xFF7F => {
                self.memory[usize::from(address)] = data;
            }
            0xFF80..=0xFFFE => {
                self.memory[usize::from(address)] = data;
            }
            0xFFFF => {
                todo!();
            }
        }
    }

    fn write_r8(self: &mut Emu, r8_encoded: u8, val: u8) {
        match r8_encoded {
            0x7 => { set_high(&mut self.cpu.af, val); }
            0x6 => {
                // 0b110 writes to [HL] instead of a register
                // https://gbdev.io/pandocs/CPU_Instruction_Set.html
                self.bus_write(self.cpu.hl, val);
            }
            0x5 => { set_low(&mut self.cpu.hl, val); }
            0x4 => { set_high(&mut self.cpu.hl, val); }
            0x3 => { set_low(&mut self.cpu.de, val); }
            0x2 => { set_high(&mut self.cpu.de, val); }
            0x1 => { set_low(&mut self.cpu.bc, val); }
            0x0 => { set_high(&mut self.cpu.bc, val); }
            _ => { unreachable!() }
        }
    }

    fn run(self: &mut Emu) {
         loop {
            let opcode = self.bus_read(self.cpu.pc);
            println!("opcode={:#x?} [{:08b}]", opcode, opcode);

            match opcode {
                0xC3 => {
                    let low = self.bus_read(self.cpu.pc + 1);
                    let high = self.bus_read(self.cpu.pc + 2);
                    self.cpu.pc = value(high, low);
                    self.cpu.cycles += 4;
                }
                /*
                    0xe  [00 001 110]
                    0x1e [00 011 110]
                    0x2e [00 101 110]
                    0x3e [00 111 110]
                    0x6  [00 000 110]
                    0x16 [00 010 110]
                    0x26 [00 100 110]
                    0x36 [00 110 110]
                */
                0xE | 0x1E | 0x2E | 0x3E | 0x06 | 0x16 | 0x26 | 0x36 => {
                    // LD r, n8
                    // note: technically & 0x7 is not required as high 2 bits are always 0 for LD r, n8
                    let reg = (opcode >> 3) & 0x7;
                    let n8 = self.bus_read(self.cpu.pc + 1);

                    self.write_r8(reg, n8);
                    self.cpu.pc += 2;
                    self.cpu.cycles += if opcode == 0x36 { 3 } else { 2 };
                }
                0xE0 => {
                    let addr = u16::from(self.bus_read(self.cpu.pc + 1)) | 0xFF00;
                    self.bus_write(addr, get_high(self.cpu.af));
                    self.cpu.pc += 2;
                    self.cpu.cycles += 3;
                }
                _ => {
                    eprintln!("instruction not implemented: {:#x?}", opcode);
                    break;
                }
            }
        }

        println!("{}", self.cpu);
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
    cgb_flag: u8,
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

fn read_cartridge_header(data: &Vec<u8>) -> std::io::Result<CartridgeHeader> {
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

    if hdr.cgb_flag == 0xC0 {
        panic!("Cartridge is CGB only");
    }

    Ok(hdr)
}

fn main() {
    let cart = Cartridge::new("dev/rgbds/gb_helloworld.gb");

    let mut emu = Emu::new(cart);

    emu.run();

}
