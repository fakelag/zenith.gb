use cartridge::cartridge::Cartridge;
use emu::emu::Emu;

mod cpu;
mod emu;
mod util;
mod cartridge;

fn main() {
    let cart = Cartridge::new("dev/rgbds/gb_helloworld.gb");

    let mut emu = Emu::new(cart);

    emu.run();
}
