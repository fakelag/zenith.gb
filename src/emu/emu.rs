use std::{fmt::{self, Display}, thread, time};

use crate::{
    cartridge::cartridge::*,
    cpu::cpu,
    ppu::ppu,
    util::util,
};

pub struct Emu {
    cart: Cartridge,
    pub cpu: cpu::CPU,
    pub ppu: ppu::PPU,
    // Main memory - indexed directly
    // https://gbdev.io/pandocs/Memory_Map.html
    memory: [u8; 0x10000],

    // debug
    pub start_at: time::Instant,
}

impl Display for Emu {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.cpu.fmt(f)?;
        self.ppu.fmt(f)?;
        Ok(())
    }
}

impl Emu {
    pub fn new(cart: Cartridge) -> Self {
        Self {
            cart,
            cpu: cpu::CPU::new(),
            ppu: ppu::PPU::new(),
            memory: [0; 0x10000],
            start_at: time::Instant::now(),
        }
    }

    pub fn run(self: &mut Emu) {
        self.dmg_boot();

        // 4,194304 MHz
        // let cycles_in_one_nano = 0.004194304;
        // let nanos_per_cycle = (1.0 / cycles_in_one_nano) as u64;

        self.start_at = time::Instant::now();

        loop {
            // let cycle_start_at = time::Instant::now();
            let cycles = cpu::step(self);

            ppu::step(self, cycles);

            // let elapsed_ns: u64 = cycle_start_at.elapsed().as_nanos().try_into().unwrap();
            // let ns_to_sleep = (u64::from(cycles) * nanos_per_cycle).checked_sub(elapsed_ns);

            // if let Some(ns) = ns_to_sleep {
            //     thread::sleep(time::Duration::from_nanos(ns));
            // }
       }
   }

    pub fn bus_read(self: &Emu, address: u16) -> u8 {
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
                // IE
                return self.memory[usize::from(address)];
            }
        }
    }

    pub fn bus_write(self: &mut Emu, address: u16, data: u8) {
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
                self.memory[usize::from(address)] = data;
            }
        }
    }

    fn dmg_boot(&mut self) {
        // https://gbdev.io/pandocs/Power_Up_Sequence.html#monochrome-models-dmg0-dmg-mgb
        util::set_high(&mut self.cpu.af, 0x1);
        self.bus_write(0xFF50, 0x1);

        util::set_high(&mut self.cpu.bc, 0);
        util::set_low(&mut self.cpu.bc, 0x13);
        util::set_high(&mut self.cpu.de, 0);
        util::set_low(&mut self.cpu.de, 0xD8);
        util::set_high(&mut self.cpu.hl, 0x1);
        util::set_low(&mut self.cpu.hl, 0x4D);

        self.cpu.sp = 0xFFFE;
        self.cpu.pc = 0x100;

        self.cpu.set_flag(cpu::FLAG_Z, true);
        self.cpu.set_flag(cpu::FLAG_N, false);
        self.cpu.set_flag(cpu::FLAG_H, if self.cart.header.header_checksum == 0x0 { false } else { true });
        self.cpu.set_flag(cpu::FLAG_C, if self.cart.header.header_checksum == 0x0 { false } else { true });
    }
}
