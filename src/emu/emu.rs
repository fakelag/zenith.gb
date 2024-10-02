use std::fmt::{self, Display};

use crate::{
    apu::apu, cartridge::cartridge::*, cpu::cpu, mmu::mmu, ppu::ppu, timer::timer
};

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

pub struct Emu {
    cartridge: Cartridge,
    pub cpu: cpu::CPU,
    pub ppu: ppu::PPU,
    pub mmu: mmu::MMU,
    pub timer: timer::Timer,
}

impl Display for Emu {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.cpu.fmt(f)?;
        self.ppu.fmt(f)?;
        Ok(())
    }
}

impl Emu {
    pub fn new(cartridge: Cartridge, sound_chan: Option<apu::ApuSoundSender>) -> Emu {
        Self {
            mmu: mmu::MMU::new(&cartridge, sound_chan),
            cpu: cpu::CPU::new(),
            ppu: ppu::PPU::new(),
            timer: timer::Timer::new(),
            cartridge,
        }
    }

    pub fn run(&mut self, num_cycles: u64) -> (u64, bool) {
        let mut cycles_run: u64 = 0;

        while cycles_run < num_cycles {
            self.mmu.set_access_origin(mmu::AccessOrigin::AccessOriginCPU);
            let cycles = self.cpu.step(&mut self.mmu);

            self.mmu.set_access_origin(mmu::AccessOrigin::AccessOriginPPU);
            let vsync = self.ppu.step(&mut self.mmu, cycles);

            self.mmu.set_access_origin(mmu::AccessOrigin::AccessOriginNone);
            self.timer.step(&mut self.mmu, cycles);
            self.mmu.step(cycles);

            cycles_run += u64::from(cycles);

            if vsync {
                return (cycles_run, true);
            }
       }

       return (cycles_run, false);
   }

    pub fn close(&mut self) {
        self.mmu.close();
    }

    pub fn input_update(&mut self, input_vec: &Vec<InputEvent>) {
        for input_event in input_vec.iter() {
            self.mmu.update_input(input_event);
        }
   }

    pub fn dmg_boot(&mut self) {
        self.mmu.set_access_origin(mmu::AccessOrigin::AccessOriginNone);

        // https://gbdev.io/pandocs/Power_Up_Sequence.html#monochrome-models-dmg0-dmg-mgb
        self.mmu.bus_write(0xFF50, 0x1);
        
        self.mmu.p1().set(0xCF);
        self.mmu.sb().set(0x00);
        self.mmu.sc().set(0x7E);
        self.mmu.div_lsb().set(0xD4);
        self.mmu.div().set(0xAB);
        self.mmu.tima().set(0x00);
        self.mmu.tma().set(0x00);
        self.mmu.tac().set(0xF8);
        self.mmu.r#if().set(0xE1);
        self.mmu.lcdc().set(0x91);
        self.mmu.stat().set(0x85);
        self.mmu.scy().set(0x0);
        self.mmu.scx().set(0x0);
        self.mmu.ly().set(0x0);
        self.mmu.lyc().set(0x0);
        self.mmu.dma().set(0xFF);
        self.mmu.bgp().set(0xFC);
        self.mmu.obp0().set(0xFF);
        self.mmu.obp1().set(0xFF);
        self.mmu.wy().set(0x00);
        self.mmu.wx().set(0x00);

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
        self.cpu.set_flag(cpu::FLAG_H, if self.cartridge.header.header_checksum == 0x0 { false } else { true });
        self.cpu.set_flag(cpu::FLAG_C, if self.cartridge.header.header_checksum == 0x0 { false } else { true });

        self.ppu.reset(&mut self.mmu);
    }
}
