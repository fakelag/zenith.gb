use std::{sync::mpsc::{self, Sender}, time};

use cartridge::cartridge::Cartridge;

mod apu;
mod cartridge;
mod cpu;
mod emu;
mod mmu;
mod ppu;
mod timer;
mod util;

use emu::emu::{Emu, GbButton, InputEvent};
use ppu::ppu::FrameBuffer;

const T_CYCLES_PER_FRAME: u64 = 4_194_304 / 60;
const M_CYCLES_PER_FRAME: u64 = T_CYCLES_PER_FRAME / 4;

const GB_SCREEN_WIDTH: u32 = 160;
const GB_SCREEN_HEIGHT: u32 = 144;
const WINDOW_SIZE_MULT: u32 = 4;

fn sdl2_create_window(sdl_ctx: &sdl2::Sdl) -> sdl2::render::Canvas<sdl2::video::Window> {
    let video_subsystem = sdl_ctx.video().unwrap();

    let window = video_subsystem
        .window("Gameboy", GB_SCREEN_WIDTH * WINDOW_SIZE_MULT, GB_SCREEN_HEIGHT * WINDOW_SIZE_MULT)
        .position_centered()
        .resizable()
        .opengl()
        .build()
        .expect("could not create window");

    let mut canvas = window
        .into_canvas()
        .build()
        .expect("could not create canvas");

    canvas
        .set_logical_size(GB_SCREEN_HEIGHT, GB_SCREEN_WIDTH)
        .expect("canvast must set device independent resolution");

    return canvas;
}

#[derive(Debug)]
struct GbAudio {
    silence: i16,
    sound_recv: mpsc::Receiver<apu::apu::ApuSampleBuffer>,
}

impl sdl2::audio::AudioCallback for GbAudio {
    type Channel = i16;

    fn callback(&mut self, out: &mut [Self::Channel]) {
        match self.sound_recv.try_recv() {
            Ok(samples) => {
                for i in 0..4096 {
                    let (l, r) = samples[i];
                    let left = i16::from(l);
                    let right = i16::from(r);

                    let left_cvt = (left-32) << 10;
                    let right_cvt = (right-32) << 10;

                    out[i * 2] = left_cvt;
                    out[i * 2 + 1] = right_cvt;
                    self.silence = right_cvt;
                }
            }
            Err(_err) => {
                for dst in out.iter_mut() {
                    *dst = self.silence;
                }
                // eprintln!("Recv error: {} - {:?}", err, std::time::Instant::now());
            }
        }
    }
}

fn sdl2_create_audio(sdl_ctx: &sdl2::Sdl) -> (
    sdl2::audio::AudioDevice<GbAudio>,
    apu::apu::ApuSoundSender,
) {
    let audio_subsystem = sdl_ctx.audio().unwrap();

    let spec_desired = sdl2::audio::AudioSpecDesired{
        channels: Some(2),
        freq: Some(44_100),
        samples: Some(4096),
    };

    let (sound_send, sound_recv) = mpsc::sync_channel::<apu::apu::ApuSampleBuffer>(1);

    let device = audio_subsystem.open_playback(None, &spec_desired, |spec| {
        GbAudio {
            silence: 0,
            sound_recv,
        }
    }).unwrap();

    device.resume();

    (device, sound_send)
}

const PALETTE: [sdl2::pixels::Color; 4] = [
    sdl2::pixels::Color::RGB(0x88, 0xa0, 0x48),
    sdl2::pixels::Color::RGB(0x48, 0x68, 0x30),
    sdl2::pixels::Color::RGB(0x28, 0x40, 0x20),
    sdl2::pixels::Color::RGB(0x18, 0x28, 0x08)
];

fn create_emulator(rom_path: &str, sound_chan: Option<apu::apu::ApuSoundSender>) -> Emu {
    let cart = Cartridge::new(rom_path);
    let mut emu = Emu::new(cart, sound_chan);
    emu.dmg_boot();
    emu
}

fn scancode_to_gb_button(scancode: Option<sdl2::keyboard::Scancode>) -> Option<GbButton> {
    match scancode {
        Some(sdl2::keyboard::Scancode::W) => { Some(GbButton::GbButtonUp) }
        Some(sdl2::keyboard::Scancode::A) => { Some(GbButton::GbButtonLeft) }
        Some(sdl2::keyboard::Scancode::S) => { Some(GbButton::GbButtonDown) }
        Some(sdl2::keyboard::Scancode::D) => { Some(GbButton::GbButtonRight) }
        Some(sdl2::keyboard::Scancode::E) | Some(sdl2::keyboard::Scancode::O) => { Some(GbButton::GbButtonA) }
        Some(sdl2::keyboard::Scancode::R) | Some(sdl2::keyboard::Scancode::P) => { Some(GbButton::GbButtonB) }
        Some(sdl2::keyboard::Scancode::N) => { Some(GbButton::GbButtonSelect) }
        Some(sdl2::keyboard::Scancode::M) => { Some(GbButton::GbButtonStart) }
        _ => { None }
    }
}

enum State {
    Exit,
    Idle,
    Running(Emu),
}

fn poll_events(
    input_vec: &mut Vec<InputEvent>,
    event_pump: &mut sdl2::EventPump,
    sound_chan: &apu::apu::ApuSoundSender,
) -> Option<State> {
    for event in event_pump.poll_iter() {
        match event {
            sdl2::event::Event::DropFile { filename, .. } => {
                return Some(State::Running(create_emulator(&filename, Some(sound_chan.clone()))));
            }
            sdl2::event::Event::Quit {..} => {
                return Some(State::Exit);
            }
            sdl2::event::Event::KeyDown { scancode, repeat, .. } => {
                if repeat {
                    continue;
                }
                if let Some(gb_button) = scancode_to_gb_button(scancode) {
                    input_vec.push(InputEvent { down: true, button: gb_button });
                }
            }
            sdl2::event::Event::KeyUp { scancode, .. } => {
                if let Some(gb_button) = scancode_to_gb_button(scancode) {
                    input_vec.push(InputEvent { down: false, button: gb_button });
                }
            },
            _ => {}
        }
    }

    None
}

fn vsync_canvas(
    rt: &FrameBuffer,
    texture: &mut sdl2::render::Texture,
    canvas: &mut sdl2::render::WindowCanvas,
    last_frame: &mut std::time::Instant, // @todo - Refactor into a system
) {
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
    canvas.present();

    let frame_time = last_frame.elapsed();
    *last_frame = std::time::Instant::now();
    canvas.window_mut().set_title(format!("fps={:?}", (1000_000 / std::cmp::max(frame_time.as_micros(), 1))).as_str()).unwrap();
}

fn run(
    state: &mut State,
    canvas: &mut sdl2::render::WindowCanvas,
    event_pump: &mut sdl2::EventPump,
    sound_chan: apu::apu::ApuSoundSender,
) -> State {
    match state {
        State::Idle => {
            loop {
                for event in event_pump.wait_timeout_iter(100) {
                    match event {
                        sdl2::event::Event::DropFile { filename, .. } => {
                            // @todo - Handle sound channel better
                            return State::Running(create_emulator(&filename, Some(sound_chan.clone())));
                        }
                        sdl2::event::Event::Quit {..} => {
                            return State::Exit;
                        }
                        _ => {}
                    }
                }
            }
        }
        State::Running(ref mut emu) => {
            let mut last_frame = std::time::Instant::now();
            let texture_creator = canvas.texture_creator();

            let mut texture = texture_creator
                .create_texture_streaming(sdl2::pixels::PixelFormatEnum::RGB24, 160, 144)
                .unwrap();

            let mut input_vec = Vec::new();
            loop {
                let start_time = time::Instant::now();

                if let Some(next_state) = poll_events(&mut input_vec, event_pump, &sound_chan) {
                    return next_state;
                }

                if input_vec.len() > 0 {
                    emu.input_update(&input_vec);
                    input_vec.clear();
                }

                loop {
                    let (cycles_run, vsync) = emu.run(M_CYCLES_PER_FRAME);

                    if vsync {
                        let rt = emu.ppu.get_framebuffer();
                        vsync_canvas(rt, &mut texture, canvas, &mut last_frame);
                    }

                    if cycles_run >= M_CYCLES_PER_FRAME {
                        break;
                    }
                }

                let elapsed = start_time.elapsed().as_micros().try_into().unwrap();
                let sleep_time = (16000 as u64).saturating_sub(elapsed);

                if sleep_time > 0 {
                   spin_sleep::sleep(time::Duration::from_micros(sleep_time));
                }
            }
        }
        State::Exit => { return State::Exit; }
    }
}

fn main() {
    let sdl_ctx = sdl2::init().unwrap();
    let mut canvas = sdl2_create_window(&sdl_ctx);

    let (_audiodevice, sound_chan) = sdl2_create_audio(&sdl_ctx);
    
    let mut event_pump = sdl_ctx.event_pump().unwrap();
    let mut state = State::Idle;

    'eventloop: loop {
        let next_state = run(&mut state, &mut canvas, &mut event_pump, sound_chan.clone());

        match next_state {
            State::Exit => {
                if let State::Running(ref mut emu) = state {
                    emu.close();
                }
                break 'eventloop;
            }
            _ => {
                state = next_state;
            }
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::{Path, PathBuf}, sync::mpsc::{Receiver, SyncSender}};
    use colored::Colorize;
    use cpu::cpu::CPU;
    use rayon::prelude::*;

    fn create_test_emulator(rom_path: &str) -> Option<Emu> {
        let cart = Cartridge::new(rom_path);
        let mut emu = Emu::new(cart, None);

        if !emu.mmu.is_supported_cart_type() {
            println!("Skipping {rom_path} due to unsupported cart type");
            return None;
        }

        emu.dmg_boot();
        Some(emu)
    }

    fn run_test_emulator<T>(
        emu: &mut Emu,
        break_chan: Receiver<T>,
        input_chan: Option<&Receiver<InputEvent>>,
        frame_chan: Option<&SyncSender<FrameBuffer>>,
    ) -> Option<T> {
        let mcycles_per_frame = T_CYCLES_PER_FRAME / 4;
        let max_cycles = mcycles_per_frame * 120 * 60; // 120 seconds, 60 fps

        let mut trigger: Option<T> = None;
        let mut cycles_run = 0;

        while cycles_run < max_cycles {
            let (cycles_passed, vsync) = emu.run(mcycles_per_frame);

            if vsync {
                if let Some(fs) = frame_chan {
                    if let Err(err) = fs.send(*emu.ppu.get_framebuffer()) {
                        panic!("Frame sender error: {err}");
                    }
                }
            }

            if let Some(input) = input_chan {
                if let Ok(next_input) = input.try_recv() {
                    emu.input_update(&vec![next_input]);
                }
            }

            if let Ok(val) = break_chan.try_recv() {
                trigger = Some(val);
                break;
            }

            cycles_run += cycles_passed;
        }

        return trigger;
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

    fn mts_runner(rom_path: &str, _inputs: Option<Vec<GbButton>>) -> Option<bool> {
        let mut emu_res = create_test_emulator(rom_path);

        if let Some(emu) = &mut emu_res {
            let (break_send, break_recv) = std::sync::mpsc::channel::<u8>();

            emu.cpu.set_breakpoint(Some(break_send));

            let bp_triggered = run_test_emulator(emu, break_recv, None, None);
            let test_passed = bp_triggered.is_some() && mts_passed(&mut emu.cpu);
            return Some(test_passed);
        } else {
            None
        }
    }

    fn snapshot_runner(rom_path: &str, inputs: Option<Vec<GbButton>>) -> Option<bool> {
        let root_path = PathBuf::from(
            rom_path
                .strip_prefix("tests/roms/")
                .unwrap_or("unnamed")
            );

        let snapshot_dir = root_path
            .parent()
            .expect("must be a valid path")
            .to_str()
            .unwrap_or("unnamed")
            .to_string();

        return snapshot_test(rom_path, &snapshot_dir, inputs);
    }

    fn snapshot_test(rom_path: &str, snapshot_dir: &str, mut inputs: Option<Vec<GbButton>>) -> Option<bool> {
        let (break_send, break_recv) = std::sync::mpsc::channel::<u8>();
        let (frame_send, frame_recv) = std::sync::mpsc::sync_channel::<FrameBuffer>(1);
        let (input_send, input_recv) = std::sync::mpsc::channel::<InputEvent>();

        if let Some(input_list) = &mut inputs {
            input_list.reverse();
        }

        let emu_result = create_test_emulator(rom_path);

        if emu_result.is_none() {
            return None;
        }

        let mut emu = emu_result.unwrap();

        let rom_path_string = rom_path.to_string();
        let snapshot_dir_string = snapshot_dir.to_string();

        std::thread::spawn(move || {
            let mut frame_count: u64 = 0;
            let mut release_inputs: Vec<(u64, GbButton)> = Vec::new();
            let mut last_frame: Option<[[u8; 160]; 144]> = None;
            let rom_filepath = Path::new(&rom_path_string);
            let rom_filename = rom_filepath
                .file_name()
                .expect("filename must exist")
                .to_str()
                .expect("filename must be valid utf-8");

            let cmp_image = if let Ok(snapshot) = bmp::open(
                format!("tests/snapshots/{snapshot_dir_string}/{rom_filename}.bmp")
            ) {
                if snapshot.get_height() != 144 || snapshot.get_width() != 160 {
                    eprintln!("Invalid image snapshot found for {rom_filename}. Image from tests will be written to the verify directory");
                    None
                } else {
                    Some(snapshot)
                }
            } else {
                eprintln!("No image snapshot found for {rom_filename}. Image from tests will be written to the verify directory");
                None
            };

            loop {
                match frame_recv.recv() {
                    Ok(rt) => {
                        last_frame = Some(rt);
                        frame_count += 1;

                        if let Some(input_vector) = &mut inputs {
                            if frame_count > 60 && frame_count % 20 == 0 {
                                if let Some(press_next) = input_vector.pop() {
                                    input_send.send(InputEvent{ button: press_next, down: true }).unwrap();
                                    release_inputs.push((frame_count + 10, press_next));
                                }
                            }
                        }

                        release_inputs.retain(|release_next| {
                            if release_next.0 > frame_count {
                                return true;
                            }

                            input_send.send(InputEvent{ button: release_next.1, down: false }).unwrap();
                            return false;
                        });

                        if let Some(img) = &cmp_image {
                            let mut match_snapshot = true;

                            'img_check: for x in 0..160 {
                                for y in 0..144 {
                                    let gb_color = rt[y][x];
                                    let palette_color = PALETTE[gb_color as usize];

                                    let snapshot_pixel = img.get_pixel(x as u32, y as u32);
                                    if snapshot_pixel.r != palette_color.r
                                        || snapshot_pixel.g != palette_color.g
                                        || snapshot_pixel.b != palette_color.b {
                                            match_snapshot = false;
                                            break 'img_check;
                                        }
                                }
                            }

                            if match_snapshot {
                                _ = break_send.send(0);
                                // break;
                            }
                        }
                    }
                    Err(_) => {
                        if cmp_image.is_none() {
                            if let Some(frame) = last_frame {
                                let mut bmp_img = bmp::Image::new(160, 144);

                                for x in 0..160 {
                                    for y in 0..144 {
                                        let gb_color = frame[y][x];
                                        let palette_color = PALETTE[gb_color as usize];
                                        bmp_img.set_pixel(
                                            x as u32,
                                            y as u32,
                                            bmp::Pixel{ r: palette_color.r, g: palette_color.g, b: palette_color.b }
                                        );
                                    }
                                }

                                _ = bmp_img.save(format!("tests/snapshots/verify/{rom_filename}.bmp"));
                            }
                        }

                        drop(break_send);
                        break;
                    }
                }
            }
        });

        let frame_check_passed = run_test_emulator(
            &mut emu,
            break_recv,
            Some(&input_recv),
            Some(&frame_send),
        );
        let test_passed = frame_check_passed.is_some();
        return Some(test_passed);
    }

    fn find_roms(rom_or_dir: &str) -> Vec<String> {
        let path = PathBuf::from(rom_or_dir);

        let roms = if path.is_file() {
            vec!(rom_or_dir.to_string())
        } else {
            let mut rom_paths = Vec::new();
            let dir_files = fs::read_dir(rom_or_dir).expect("directory must exist");

            for dir_file in dir_files {
                let p = dir_file.unwrap();
                let metadata = p.metadata().unwrap();
    
                if !metadata.is_file() {
                    continue;
                }
    
                if p.path().extension().unwrap() != "gb" {
                    continue;
                }
    
                let full_path: String = p.path().display().to_string();
                rom_paths.push(full_path);
            }

            rom_paths
        };

        return roms;
    }

    #[test]
    fn mts() {
        type RunnerFn = fn(path:&str, Option<Vec<GbButton>>) -> Option<bool>;

        let test_roms: Vec<(RunnerFn, &str, Option<Vec<GbButton>>)>  = vec!(
            (snapshot_runner, "tests/roms/blargg/cpu_instrs/", None),
            (snapshot_runner, "tests/roms/rtc3test/rtc3test.0.gb", Some(vec![GbButton::GbButtonA])),
            (snapshot_runner, "tests/roms/rtc3test/rtc3test.1.gb", Some(vec![GbButton::GbButtonDown, GbButton::GbButtonA])),
            (snapshot_runner, "tests/roms/rtc3test/rtc3test.2.gb", Some(vec![GbButton::GbButtonDown, GbButton::GbButtonDown, GbButton::GbButtonA])),
            (snapshot_runner, "tests/roms/blargg/instr_timing/", None),
            (snapshot_runner, "tests/roms/mts/manual-only/sprite_priority.gb", None),
            (mts_runner, "tests/roms/mts/acceptance/bits/", None),
            (mts_runner, "tests/roms/mts/acceptance/instr/", None),
            (mts_runner, "tests/roms/mts/acceptance/interrupts/", None),
            (mts_runner, "tests/roms/mts/acceptance/oam_dma/", None),
            (mts_runner, "tests/roms/mts/acceptance/ppu/", None),
            (mts_runner, "tests/roms/mts/acceptance/timer/", None),
            (mts_runner, "tests/roms/mts/acceptance/", None),
            (mts_runner, "tests/roms/mts/emulator-only/mbc1/", None),
            (mts_runner, "tests/roms/mts/emulator-only/mbc2/", None),
            (mts_runner, "tests/roms/mts/emulator-only/mbc5/", None),
            (mts_runner, "tests/roms/mts/misc/bits/", None),
            (mts_runner, "tests/roms/mts/misc/ppu/", None),
            (mts_runner, "tests/roms/mts/misc/", None),
            // (mts_runner, "tests/roms/mts/acceptance/serial/", None),
        );

        let rom_files = test_roms
            .par_iter()
            .flat_map(|(runner, rom_or_dir, inputs)| {
                let roms_with_runners = find_roms(rom_or_dir)
                    .iter()
                    .map(|path: &String| (runner.clone(), path.clone(), inputs.clone()))
                    .collect::<Vec<(RunnerFn, String, Option<Vec<GbButton>>)>>();

                return roms_with_runners;
            })
            .collect::<Vec<(RunnerFn, String, Option<Vec<GbButton>>)>>();

        let results = rom_files
            .par_iter()
            .map(|(runner, rom_path, inputs)| (rom_path, runner(rom_path, inputs.clone())))
            .collect::<Vec<(&String, Option<bool>)>>();

        results
            .iter()
            .for_each(|(path, pass_opt)| {
                if let Some(is_passing) = pass_opt {
                    if *is_passing {
                        println!("[{}]: {path}", "Passed".green().bold());
                    } else {
                        eprintln!("[{}]: {path}", "Failed".red().bold());
                    }
                }
            });
        
        let stats = results
            .iter()
            .fold((0, 0, 0), |acc, res| {
                if let Some(is_passing) = res.1 {
                    if is_passing {
                        return (acc.0 + 1, acc.1, acc.2);
                    } else {
                        return (acc.0, acc.1 + 1, acc.2);
                    }
                }
                return (acc.0, acc.1, acc.2 + 1);
            });

        println!("{} passed {} failed, {} skipped, {} total", stats.0, stats.1, stats.2, results.len());

        // assert!(result_vec.iter().all(|(_, pass)| *pass));
    }
}
