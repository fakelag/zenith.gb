use std::{
    fmt::{self, Display},
    rc::Rc,
};

use crate::{
    apu::apu,
    cartridge::cartridge::*,
    cpu::cpu,
    ppu::ppu::{self, FrameBuffer},
    soc::soc,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CompatibilityMode {
    CompCgb,
    CompCgbDmg,
    CompDmg,
}

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
    pub bp_chan: Option<cpu::BpSender>,
    pub input_recv: Option<InputReceiver>,

    pub enable_saving: bool,
    pub sync_audio: bool,
    pub sync_video: bool,
    pub max_cycles: Option<u64>,

    pub comp_mode: Option<CompatibilityMode>,
}

pub struct GbCtx {
    pub cgb: bool,
    pub comp_mode: CompatibilityMode,
    pub rom_path: String,
}

pub struct Gameboy {
    cartridge: Cartridge,
    cpu: cpu::CPU,
    soc: soc::SOC,
    ctx: Rc<GbCtx>,
}

impl Display for Gameboy {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.cpu.fmt(f)?;
        Ok(())
    }
}

impl Gameboy {
    pub fn new(cartridge: Cartridge, config: Box<EmulatorConfig>) -> Gameboy {
        let comp_mode = if let Some(mode) = config.comp_mode {
            mode
        } else if cartridge.header.is_cgb() {
            CompatibilityMode::CompCgb
        } else {
            CompatibilityMode::CompCgbDmg
        };

        let ctx = Rc::new(GbCtx {
            cgb: comp_mode == CompatibilityMode::CompCgb,
            rom_path: cartridge.rom_path.clone(),
            comp_mode,
        });

        let gb = Self {
            soc: soc::SOC::new(
                &cartridge,
                config.input_recv,
                config.sound_chan,
                config.frame_chan,
                config.enable_saving,
                config.sync_audio,
                config.sync_video,
                config.max_cycles,
                ctx.clone(),
            ),
            cpu: cpu::CPU::new(config.bp_chan),
            cartridge,
            ctx,
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

    pub fn boot(&mut self) {
        self.cpu
            .init(&mut self.soc, &self.cartridge, self.ctx.comp_mode);
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
    pub fn get_cpu(&mut self) -> &mut cpu::CPU {
        &mut self.cpu
    }
}
