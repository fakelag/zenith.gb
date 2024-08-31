use std::fmt::{self, Display};

use crate::{emu::emu::Emu, util::*};

pub struct PPU {
    //
}

impl Display for PPU {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "PPU")?;
        Ok(())
    }
}

impl PPU {
    pub fn new() -> Self {
        Self { }
    }
}

pub fn step(emu: &mut Emu) -> u8 {
    println!("ppu step");
    return 1;
}
