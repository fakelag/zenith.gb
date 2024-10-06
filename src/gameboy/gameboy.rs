use std::fmt::{self, Display};

#[cfg(test)]
use std::sync::mpsc::Sender;

use crate::{apu::apu, cartridge::cartridge::*, cpu::cpu, ppu::ppu::FrameBuffer, soc::soc};

#[derive(PartialEq, Copy, Clone, Debug)]
pub enum GbButton {
    GbButtonUp = 0,
    GbButtonRight,
    GbButtonLeft,
    GbButtonDown,
    GbButtonA,
    GbButtonB,
    GbButtonStart,
    GbButtonSelect,
    GbButtonMax,
}

pub struct InputEvent {
    pub down: bool,
    pub button: GbButton,
}

pub struct Gameboy {
    cartridge: Cartridge,
    cpu: cpu::CPU,
    soc: soc::SOC,
}

impl Display for Gameboy {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.cpu.fmt(f)?;
        Ok(())
    }
}

impl Gameboy {
    pub fn new(cartridge: Cartridge) -> Gameboy {
        Self {
            soc: soc::SOC::new(&cartridge),
            cpu: cpu::CPU::new(),
            cartridge,
        }
    }

    pub fn run(&mut self, num_cycles: u64) -> (u64, bool) {
        let mut cycles_run: u64 = 0;

        while cycles_run < num_cycles {
            let cycles = self.cpu.step(&mut self.soc);

            let vsync = self.soc.hw_step(cycles);

            cycles_run += u64::from(cycles);

            if vsync {
                return (cycles_run, true);
            }
        }

        return (cycles_run, false);
    }

    pub fn dmg_boot(&mut self) {
        // https://gbdev.io/pandocs/Power_Up_Sequence.html#monochrome-models-dmg0-dmg-mgb
        self.cpu.a().set(0x1);
        self.cpu.b().set(0);
        self.cpu.c().set(0x13);
        self.cpu.d().set(0);
        self.cpu.e().set(0xD8);
        self.cpu.h().set(0x1);
        self.cpu.l().set(0x4D);

        self.cpu.sp().set(0xFFFE);
        self.cpu.pc().set(0x100);

        self.cpu.set_flag(cpu::FLAG_Z, true);
        self.cpu.set_flag(cpu::FLAG_N, false);
        self.cpu.set_flag(
            cpu::FLAG_H,
            if self.cartridge.header.header_checksum == 0x0 {
                false
            } else {
                true
            },
        );
        self.cpu.set_flag(
            cpu::FLAG_C,
            if self.cartridge.header.header_checksum == 0x0 {
                false
            } else {
                true
            },
        );
    }

    pub fn enable_external_audio(&mut self, sound_chan: apu::ApuSoundSender) {
        self.soc.enable_external_audio(sound_chan);
    }

    pub fn close(&mut self) {
        self.soc.close();
    }

    pub fn save(&mut self) {
        self.soc.save();
    }

    pub fn input_update(&mut self, input_vec: &Vec<InputEvent>) {
        for input_event in input_vec.iter() {
            self.soc.input_update(input_event);
        }
    }

    pub fn get_framebuffer(&self) -> &FrameBuffer {
        self.soc.get_framebuffer()
    }

    #[cfg(test)]
    pub fn set_breakpoint(&mut self, bp_send: Option<Sender<u8>>) {
        self.cpu.set_breakpoint(bp_send);
    }

    #[cfg(test)]
    pub fn get_cpu(&mut self) -> &mut cpu::CPU {
        &mut self.cpu
    }

    #[cfg(test)]
    pub fn is_supported_cart_type(&self) -> bool {
        self.soc.is_supported_cart_type()
    }
}
