use std::{sync::mpsc::{Receiver, SyncSender}, time};

use cartridge::cartridge::Cartridge;

mod cartridge;
mod cpu;
mod emu;
mod mmu;
mod ppu;
mod timer;
mod util;

use emu::emu::{Emu, FrameBuffer, GbButton, InputEvent};
use sdl2::keyboard::Scancode;

const T_CYCLES_PER_FRAME: u64 = 4_194_304 / 60;

fn sdl2_create_window() -> (sdl2::render::Canvas<sdl2::video::Window>, sdl2::Sdl) {
    let sdl_ctx = sdl2::init().unwrap();

    let video_subsystem = sdl_ctx.video().unwrap();

    let asp = 144.0 / 160.0;

    let window = // video_subsystem.window("Gameboy", 160, 144 as u32) //
        video_subsystem.window("Gameboy", 512, (512.0 * asp) as u32)
        .position_centered()
        .resizable()
        .opengl()
        .build()
        .expect("could not create window");

    let canvas = window.into_canvas().build()
        .expect("could not create canvas");

    return (canvas, sdl_ctx);
}

const PALETTE: [sdl2::pixels::Color; 4] = [
    sdl2::pixels::Color::RGB(0x88, 0xa0, 0x48),
    sdl2::pixels::Color::RGB(0x48, 0x68, 0x30),
    sdl2::pixels::Color::RGB(0x28, 0x40, 0x20),
    sdl2::pixels::Color::RGB(0x18, 0x28, 0x08)
];

fn run_emulator(frame_chan: SyncSender<FrameBuffer>, input_chan: Receiver<InputEvent>) {
    let cart = Cartridge::new("dev/rgbds/gb_helloworld.gb");
    let mut emu = Emu::new(cart, Some(frame_chan), Some(input_chan));
    emu.dmg_boot();

    let m_cycles_per_frame = T_CYCLES_PER_FRAME / 4;

    loop {
        let start_time = time::Instant::now();
        let cycles_run = emu.run(m_cycles_per_frame);

        if cycles_run.is_none() {
            return;
        }

        let elapsed = start_time.elapsed().as_micros().try_into().unwrap();
        let sleep_time = (16000 as u64).saturating_sub(elapsed);

        if sleep_time > 0 {
            spin_sleep::sleep(time::Duration::from_micros(sleep_time));
        }
    }
}

fn scancode_to_gb_button(scancode: Option<Scancode>) -> Option<GbButton> {
    match scancode {
        Some(Scancode::W) => { Some(GbButton::GbButtonUp) }
        Some(Scancode::A) => { Some(GbButton::GbButtonLeft) }
        Some(Scancode::S) => { Some(GbButton::GbButtonDown) }
        Some(Scancode::D) => { Some(GbButton::GbButtonRight) }
        Some(Scancode::E) | Some(Scancode::O) => { Some(GbButton::GbButtonA) }
        Some(Scancode::R) | Some(Scancode::P) => { Some(GbButton::GbButtonB) }
        Some(Scancode::N) => { Some(GbButton::GbButtonSelect) }
        Some(Scancode::M) => { Some(GbButton::GbButtonStart) }
        _ => { None }
    }
}

fn main() {
    let (mut canvas, sdl_ctx) = sdl2_create_window();

    let texture_creator = canvas.texture_creator();

    let mut texture = texture_creator
        .create_texture_streaming(sdl2::pixels::PixelFormatEnum::RGB24, 160, 144)
        .unwrap();

    let (frame_sender, frame_receiver) = std::sync::mpsc::sync_channel::<FrameBuffer>(1);
    let (input_sender, input_receiver) = std::sync::mpsc::sync_channel::<InputEvent>(1);

    let emu_thread = std::thread::spawn(move || run_emulator(frame_sender, input_receiver));
    
    let mut last_frame = std::time::Instant::now();
    let mut event_pump = sdl_ctx.event_pump().unwrap();
    'eventloop: loop {
        for event in event_pump.poll_iter() {
            match event {
                sdl2::event::Event::Quit {..} => {
                    break 'eventloop;
                }
                sdl2::event::Event::KeyDown { scancode, repeat, .. } => {
                    if !repeat {
                        if let Some(gb_button) = scancode_to_gb_button(scancode) {
                            if let Err(err) = input_sender.send(InputEvent { down: true, button: gb_button }) {
                                println!("err={:?}", err);
                                break 'eventloop;
                            }
                        }
                    }
                }
                sdl2::event::Event::KeyUp { scancode, .. } => {
                    if let Some(gb_button) = scancode_to_gb_button(scancode) {
                        if let Err(err) = input_sender.send(InputEvent { down: false, button: gb_button }) {
                            println!("err={:?}", err);
                            break 'eventloop;
                        }
                    }
                },
                _ => {}
            }
        }

        match frame_receiver.recv() {
            Ok(rt) => {
                let frame_time = last_frame.elapsed();
                last_frame = std::time::Instant::now();

                // println!("received frame");
                
                canvas.window_mut().set_title(format!("fps={:?}", (1000_000 / std::cmp::max(frame_time.as_micros(), 1))).as_str()).unwrap();

                texture.with_lock(None, |buffer, size| {
                    for x in 0..160 {
                        for y in 0..144 {
                            let color = rt[y][x];

                            let index = y * size + x * 3;
                            match color {
                                0..=3 => {
                                    let c = PALETTE[color as usize];
                                    buffer[index] = c.r;
                                    buffer[index+1] = c.g;
                                    buffer[index+2] = c.b;
                                }
                                _ => unreachable!(),
                            }
                        }
                    }
                }).unwrap();

                canvas.clear();
                canvas.copy(&texture, None, None).unwrap();
                // canvas.copy_ex(
                //     &texture,
                //     None,
                //     None,
                //     0.0,
                //     None,
                //     false,
                //     false,
                // ).unwrap();
                canvas.present();
            },
            Err(..) => break 'eventloop,
        }

    }

    drop(frame_receiver);
    emu_thread.join().unwrap();
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use colored::Colorize;
    use cpu::cpu::CPU;
    use rayon::prelude::*;

    fn run_emulator(rom_path: &str) -> Emu {
        let cart = Cartridge::new(rom_path);
        let mut emu = Emu::new(cart, None, None);
        emu.dmg_boot();
        emu
    }

    fn mts_passed(cpu: &mut CPU) -> bool {
        if cpu.b().get() != 3
            || cpu.c().get() != 5
            || cpu.d().get() != 8
            || cpu.e().get() != 13
            || cpu.h().get() != 21
            || cpu.l().get() != 34 {
            return false;
        }

        return true;
    }

    fn mts_test(rom_path: &str) -> bool {
        let mut emu = run_emulator(rom_path);

        let (result_send, result_recv) = std::sync::mpsc::channel::<u8>();

        emu.cpu.set_breakpoint(Some(result_send));

        let mcycles_per_frame = T_CYCLES_PER_FRAME / 4;
        let max_cycles = mcycles_per_frame * 60 * 30; // 30 seconds

        let mut bp_triggered = false;
        let mut cycles_run = 0;
        while cycles_run < max_cycles {
            let cycles_passed = emu.run(mcycles_per_frame);

            if let Some(cycles) = cycles_passed {
                cycles_run += cycles;
            } else {
                unreachable!();
            }

            if let Ok(_) = result_recv.try_recv() {
                bp_triggered = true;
                break;
            }
        }

        let test_passed = bp_triggered && mts_passed(&mut emu.cpu);
        return test_passed;
    }

    fn roms_in_dir(path: &str) -> Vec<String> {
        let mut roms = Vec::new();
        let paths = fs::read_dir(path).unwrap();

        for path in paths {
            let p = path.unwrap();
            let metadata = p.metadata().unwrap();

            if !metadata.is_file() {
                continue;
            }

            if p.path().extension().unwrap() != "gb" {
                continue;
            }

            let full_path: String = p.path().display().to_string();
            roms.push(full_path);
        }

        return roms;
    }

    fn mts_suite(dir: &str) -> Vec<(String, bool)> {
        let rom_paths = roms_in_dir(dir);
    
        let result_vec = rom_paths
            .par_iter()
            .map(|rom_path| (
                rom_path.to_string(),
                mts_test(rom_path.as_str()),
            ))
            .collect::<Vec<(String, bool)>>();

        return result_vec;
    }

    #[test]
    fn mts() {
        let mut results: Vec<(String, bool)> = Vec::new();
        results.append(&mut mts_suite("dev/rgbds/mts/acceptance/bits/"));
        results.append(&mut mts_suite("dev/rgbds/mts/acceptance/instr/"));
        results.append(&mut mts_suite("dev/rgbds/mts/acceptance/interrupts/"));
        results.append(&mut mts_suite("dev/rgbds/mts/acceptance/oam_dma/"));
        results.append(&mut mts_suite("dev/rgbds/mts/acceptance/ppu/"));
        // results.append(&mut mts_suite("dev/rgbds/mts/acceptance/serial/"));
        results.append(&mut mts_suite("dev/rgbds/mts/acceptance/timer/"));

        // Rest of acceptance suite
        results.append(&mut mts_suite("dev/rgbds/mts/acceptance/"));

        // MBC
        results.append(&mut mts_suite("dev/rgbds/mts/emulator-only/mbc1/"));
        // results.append(&mut mts_suite("dev/rgbds/mts/emulator-only/mbc2"));
        // results.append(&mut mts_suite("dev/rgbds/mts/emulator-only/mbc3"));

        results.append(&mut mts_suite("dev/rgbds/mts/misc/bits/"));
        results.append(&mut mts_suite("dev/rgbds/mts/misc/ppu/"));
        results.append(&mut mts_suite("dev/rgbds/mts/misc/"));

        results
            .iter()
            .for_each(|(path, pass)| {
                if *pass {
                    println!("[{}]: {path}", "Passed".green().bold());
                } else {
                    eprintln!("[{}]: {path}", "Failed".red().bold());
                }
            });
        
        let stats = results
            .iter()
            .fold((0, 0), |acc, res| {
                if res.1 {
                    return (acc.0 + 1, acc.1);
                } else {
                    return (acc.0, acc.1 + 1);
                }
            });

        println!("{} passed out of {} total", stats.0, results.len());

        // assert!(result_vec.iter().all(|(_, pass)| *pass));
    }
}