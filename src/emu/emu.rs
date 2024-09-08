use std::{fmt::{self, Display}, sync::mpsc::SyncSender, time};

use crate::{
    cartridge::cartridge::*, cpu::cpu, mmu::mmu, ppu::ppu, util::util
};

pub type FrameBuffer = [[u8; 160]; 144];

pub struct Emu {
    cartridge: Cartridge,
    pub cpu: cpu::CPU,
    pub ppu: ppu::PPU,
    pub mmu: mmu::MMU,

    // debug
    pub start_at: time::Instant,

    pub frame_chan: SyncSender<FrameBuffer>,
}

impl Display for Emu {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.cpu.fmt(f)?;
        self.ppu.fmt(f)?;
        println!("LY={}", self.bus_read(0xFF44));
        println!("took {}ms", self.start_at.elapsed().as_millis());
        Ok(())
    }
}

impl Emu {
    pub fn new(cartridge: Cartridge, frame_chan: SyncSender<FrameBuffer>) -> Emu {
        Self {
            mmu: mmu::MMU::new(&cartridge),
            cpu: cpu::CPU::new(),
            ppu: ppu::PPU::new(),
            start_at: time::Instant::now(),
            cartridge,
            frame_chan,
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
            let cycles = self.cpu.step(&mut self.mmu);

            self.ppu.step(&mut self.mmu, &mut self.frame_chan, cycles);

            // let elapsed_ns: u64 = cycle_start_at.elapsed().as_nanos().try_into().unwrap();
            // let ns_to_sleep = (u64::from(cycles) * nanos_per_cycle).checked_sub(elapsed_ns);

            // if let Some(ns) = ns_to_sleep {
            //     thread::sleep(time::Duration::from_nanos(ns));
            // }
       }
   }

    pub fn bus_read(self: &Emu, address: u16) -> u8 {
        self.mmu.bus_read(address)
    }

    pub fn bus_write(self: &mut Emu, address: u16, data: u8) {
       self.mmu.bus_write(address, data)
    }

    fn dmg_boot(&mut self) {
        // https://gbdev.io/pandocs/Power_Up_Sequence.html#monochrome-models-dmg0-dmg-mgb
        self.bus_write(0xFF50, 0x1);

        // 0x91 -> LCDC
        self.bus_write(0xFF40, 0x91);

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
    }
}
