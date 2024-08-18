use cartridge::cartridge::Cartridge;
use cpu::cpu::{CPU, FLAG_C, FLAG_H, FLAG_N, FLAG_Z};
use emu::emu::Emu;
use util::util::*;

mod cpu;
mod emu;
mod util;
mod cartridge;

fn main() {
    let cart = Cartridge::new("dev/rgbds/gb_helloworld.gb");

    let mut emu = Emu::new(cart);

    emu.run();

}
