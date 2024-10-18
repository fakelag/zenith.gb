use std::fmt::{self, Display};

#[cfg(test)]
use std::sync::mpsc::Sender;

use crate::{
    apu::apu,
    cartridge::cartridge::*,
    cpu::cpu,
    ppu::ppu::{self, FrameBuffer},
    soc::soc,
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

pub type InputReceiver = std::sync::mpsc::Receiver<InputEvent>;
pub type InputSender = std::sync::mpsc::SyncSender<InputEvent>;

pub struct EmulatorConfig {
    pub sound_chan: Option<apu::ApuSoundSender>,
    pub frame_chan: Option<ppu::PpuFrameSender>,
    pub input_recv: Option<InputReceiver>,

    pub enable_saving: bool,
    pub sync_audio: bool,
    pub sync_video: bool,
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
    pub fn new(cartridge: Cartridge, config: Box<EmulatorConfig>) -> Gameboy {
        let gb = Self {
            soc: soc::SOC::new(&cartridge, config),
            cpu: cpu::CPU::new(),
            cartridge,
        };

        gb
    }

    pub fn run(&mut self) {
        loop {
            let _cycles = self.cpu.step(&mut self.soc);
            if self.soc.process_events() {
                break;
            }
        }
    }

    pub fn dmg_boot(&mut self) {
        self.cpu.init(&mut self.soc, &self.cartridge);
    }

    pub fn close(&mut self) {
        self.soc.close();
    }

    pub fn save(&mut self) {
        self.soc.save();
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
}
