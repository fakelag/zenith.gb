use crate::{cartridge::cartridge::Cartridge, cpu::cpu::CPU, util::util};

pub struct Emu {
    cart: Cartridge,
    pub cpu: CPU,
    // Main memory - indexed directly
    // https://gbdev.io/pandocs/Memory_Map.html
    memory: [u8; 0xFFFF],
}

impl Emu {
    pub fn new(cart: Cartridge) -> Self {
        Self { cart, cpu: CPU::new(), memory: [0; 0xFFFF] }
    }

    pub fn run(self: &mut Emu) {

        // boot https://gbdev.io/pandocs/Power_Up_Sequence.html#monochrome-models-dmg0-dmg-mgb
        util::set_high(&mut self.cpu.af, 0x1);
        self.bus_write(0xFF50, 0x1);

        loop {
            CPU::step(self);
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
                // Interrupt
                todo!();
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
                todo!();
            }
        }
    }
}
