use std::{fmt::{self, Display}, sync::mpsc::{Receiver, SyncSender, TryRecvError}, time};

use crate::{
    cartridge::cartridge::*,
    cpu::cpu,
    mmu::mmu,
    ppu::ppu,
    timer::timer
};

pub type FrameBuffer = [[u8; 160]; 144];

#[derive(PartialEq)]
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

    // debug
    pub start_at: time::Instant,

    pub frame_chan: SyncSender<FrameBuffer>,
    pub input_chan: Receiver<InputEvent>,
}

impl Display for Emu {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.cpu.fmt(f)?;
        self.ppu.fmt(f)?;
        println!("took {}ms", self.start_at.elapsed().as_millis());
        Ok(())
    }
}

impl Emu {
    pub fn new(cartridge: Cartridge, frame_chan: SyncSender<FrameBuffer>, input_chan: Receiver<InputEvent>) -> Emu {
        Self {
            mmu: mmu::MMU::new(&cartridge),
            cpu: cpu::CPU::new(),
            ppu: ppu::PPU::new(),
            timer: timer::Timer::new(),
            start_at: time::Instant::now(),
            cartridge,
            frame_chan,
            input_chan,
        }
    }

    pub fn run(self: &mut Emu) {
        self.dmg_boot();

        // @todo - 4,194304 MHz

        self.start_at = time::Instant::now();

        loop {
            self.input_update();

            self.mmu.set_access_origin(mmu::AccessOrigin::AccessOriginCPU);
            let cycles = self.cpu.step(&mut self.mmu);

            self.mmu.set_access_origin(mmu::AccessOrigin::AccessOriginPPU);
            self.ppu.step(&mut self.mmu, &mut self.frame_chan, cycles);

            self.mmu.set_access_origin(mmu::AccessOrigin::AccessOriginNone);
            self.timer.step(&mut self.mmu, cycles);

            self.mmu.set_access_origin(mmu::AccessOrigin::AccessOriginNone);
            self.mmu.step(cycles);
       }
   }

   fn input_update(&mut self) {
        match self.input_chan.try_recv() {
            Ok(input_event) => {
                self.mmu.update_input(input_event);
            }
            Err(TryRecvError::Disconnected) => {
                return;
            }
            Err(TryRecvError::Empty) => {}
        }
   }

    fn dmg_boot(&mut self) {
        self.mmu.set_access_origin(mmu::AccessOrigin::AccessOriginNone);

        // https://gbdev.io/pandocs/Power_Up_Sequence.html#monochrome-models-dmg0-dmg-mgb
        self.mmu.bus_write(0xFF50, 0x1);
        
        self.mmu.p1().set(0xCF);
        self.mmu.sb().set(0x00);
        self.mmu.sc().set(0x7E);
        self.mmu.div().set(0x18);
        self.mmu.tima().set(0x00);
        self.mmu.tma().set(0x00);
        self.mmu.tac().set(0xF8);
        self.mmu.r#if().set(0xE1);
        self.mmu.nr10().set(0x80);
        self.mmu.nr11().set(0xBF);
        self.mmu.nr12().set(0xF3);
        self.mmu.nr13().set(0xFF);
        self.mmu.nr14().set(0xBF);
        self.mmu.nr21().set(0x3F);
        self.mmu.nr22().set(0x00);
        self.mmu.nr23().set(0xFF);
        self.mmu.nr24().set(0xBF);
        self.mmu.nr30().set(0x7F);
        self.mmu.nr31().set(0xFF);
        self.mmu.nr32().set(0x9F);
        self.mmu.nr33().set(0xFF);
        self.mmu.nr34().set(0xBF);
        self.mmu.nr41().set(0xFF);
        self.mmu.nr42().set(0x00);
        self.mmu.nr43().set(0x00);
        self.mmu.nr44().set(0xBF);
        self.mmu.nr50().set(0x77);
        self.mmu.nr51().set(0xF3);
        self.mmu.nr52().set(0xF1);
        self.mmu.lcdc().set(0x91);
        self.mmu.stat().set(0x81);
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
