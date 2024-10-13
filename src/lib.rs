use std::{
    sync::mpsc::{self},
    time,
};

use apu::apu::{APU_FREQ, APU_NUM_CHANNELS, APU_SAMPLES, APU_SAMPLES_PER_CHANNEL};
use cartridge::cartridge::Cartridge;
use gameboy::gameboy::*;
use ppu::ppu::FrameBuffer;
use sdl2::audio::AudioFormatNum;

pub mod apu;
pub mod cartridge;
pub mod cpu;
pub mod gameboy;
pub mod mbc;
pub mod ppu;
pub mod serial;
pub mod soc;
pub mod timer;
pub mod util;

pub enum State {
    Exit,
    Idle,
    Running(Gameboy),
}

pub const GB_DEFAULT_FPS: f64 = 59.73;
pub const TARGET_FPS: f64 = GB_DEFAULT_FPS;

const T_CYCLES_PER_FRAME: u64 = (4_194_304.0 / GB_DEFAULT_FPS) as u64;
const M_CYCLES_PER_FRAME: u64 = T_CYCLES_PER_FRAME / 4;
const FRAME_TIME: u64 = ((1.0 / TARGET_FPS) * 1000_000.0) as u64;

const GB_SCREEN_WIDTH: u32 = 160;
const GB_SCREEN_HEIGHT: u32 = 144;
const WINDOW_SIZE_MULT: u32 = 4;

const PALETTE: [sdl2::pixels::Color; 4] = [
    sdl2::pixels::Color::RGB(0x88, 0xa0, 0x48),
    sdl2::pixels::Color::RGB(0x48, 0x68, 0x30),
    sdl2::pixels::Color::RGB(0x28, 0x40, 0x20),
    sdl2::pixels::Color::RGB(0x18, 0x28, 0x08),
];

pub fn sdl2_create_window(sdl_ctx: &sdl2::Sdl) -> sdl2::render::Canvas<sdl2::video::Window> {
    let video_subsystem = sdl_ctx.video().unwrap();

    let window = video_subsystem
        .window(
            "Gameboy",
            GB_SCREEN_WIDTH * WINDOW_SIZE_MULT,
            GB_SCREEN_HEIGHT * WINDOW_SIZE_MULT,
        )
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

pub struct GbAudio {
    sound_recv: mpsc::Receiver<Vec<i16>>,
}

impl sdl2::audio::AudioCallback for GbAudio {
    type Channel = i16;

    fn callback(&mut self, out: &mut [Self::Channel]) {
        match self
            .sound_recv
            .recv_timeout(time::Duration::from_millis(15))
        {
            Ok(samples) => {
                debug_assert!(samples.len() == out.len());

                out.copy_from_slice(&samples);
            }
            Err(_err) => {
                // println!("recv err {:?}", time::Instant::now());
                for i in 0..APU_SAMPLES {
                    out[i] = Self::Channel::SILENCE;
                }
            }
        }
    }
}

pub fn sdl2_create_audio(
    sdl_ctx: &sdl2::Sdl,
) -> (sdl2::audio::AudioDevice<GbAudio>, apu::apu::ApuSoundSender) {
    let audio_subsystem = sdl_ctx.audio().unwrap();

    let spec_desired = sdl2::audio::AudioSpecDesired {
        channels: Some(APU_NUM_CHANNELS),
        samples: Some(APU_SAMPLES_PER_CHANNEL),
        freq: Some(APU_FREQ as i32),
    };

    let (sound_send, sound_recv) = mpsc::sync_channel::<Vec<i16>>(1);

    let device = audio_subsystem
        .open_playback(None, &spec_desired, |_spec| GbAudio { sound_recv })
        .unwrap();

    device.resume();

    (device, sound_send)
}

pub fn sdl2_enable_controller(
    sdl_ctx: &sdl2::Sdl,
) -> Result<sdl2::controller::GameController, String> {
    let controller_subsystem = sdl_ctx.game_controller()?;

    let available = controller_subsystem
        .num_joysticks()
        .map_err(|e| format!("can't enumerate joysticks: {}", e))?;

    let mut controller = (0..available)
        .find_map(|id| {
            if !controller_subsystem.is_game_controller(id) {
                return None;
            }

            match controller_subsystem.open(id) {
                Ok(c) => Some(c),
                Err(_e) => None,
            }
        })
        .ok_or_else(|| format!("Couldn't find any controllers"))?;

    _ = controller.set_rumble(0, 20_000, 500);
    _ = controller.set_led(PALETTE[0].r, PALETTE[0].g, PALETTE[0].b);

    return Ok(controller);
}

pub fn create_emulator(rom_path: &str) -> Gameboy {
    let cart = Cartridge::new(rom_path);
    let mut gb = Gameboy::new(cart);
    gb.dmg_boot();
    gb
}

fn scancode_to_gb_btn(scancode: Option<sdl2::keyboard::Scancode>) -> Option<GbButton> {
    match scancode {
        Some(sdl2::keyboard::Scancode::Up | sdl2::keyboard::Scancode::W) => {
            Some(GbButton::GbButtonUp)
        }
        Some(sdl2::keyboard::Scancode::Left | sdl2::keyboard::Scancode::A) => {
            Some(GbButton::GbButtonLeft)
        }
        Some(sdl2::keyboard::Scancode::Down | sdl2::keyboard::Scancode::S) => {
            Some(GbButton::GbButtonDown)
        }
        Some(sdl2::keyboard::Scancode::Right | sdl2::keyboard::Scancode::D) => {
            Some(GbButton::GbButtonRight)
        }
        Some(sdl2::keyboard::Scancode::C) | Some(sdl2::keyboard::Scancode::O) => {
            Some(GbButton::GbButtonA)
        }
        Some(sdl2::keyboard::Scancode::V) | Some(sdl2::keyboard::Scancode::P) => {
            Some(GbButton::GbButtonB)
        }
        Some(sdl2::keyboard::Scancode::N) => Some(GbButton::GbButtonSelect),
        Some(sdl2::keyboard::Scancode::M) => Some(GbButton::GbButtonStart),
        _ => None,
    }
}

fn controller_btn_to_gb_btn(btn: sdl2::controller::Button, _which: u32) -> Option<GbButton> {
    match btn {
        sdl2::controller::Button::DPadUp => Some(GbButton::GbButtonUp),
        sdl2::controller::Button::DPadLeft => Some(GbButton::GbButtonLeft),
        sdl2::controller::Button::DPadDown => Some(GbButton::GbButtonDown),
        sdl2::controller::Button::DPadRight => Some(GbButton::GbButtonRight),
        sdl2::controller::Button::A => Some(GbButton::GbButtonA),
        sdl2::controller::Button::B => Some(GbButton::GbButtonB),
        sdl2::controller::Button::Guide => Some(GbButton::GbButtonSelect),
        sdl2::controller::Button::Start => Some(GbButton::GbButtonStart),
        _ => None,
    }
}

fn controller_axis_gb_btn(axis: sdl2::controller::Axis) -> Option<(GbButton, GbButton)> {
    match axis {
        sdl2::controller::Axis::LeftX => Some((GbButton::GbButtonLeft, GbButton::GbButtonRight)),
        sdl2::controller::Axis::LeftY => Some((GbButton::GbButtonUp, GbButton::GbButtonDown)),
        _ => None,
    }
}

fn poll_events(input_vec: &mut Vec<InputEvent>, event_pump: &mut sdl2::EventPump) -> Option<State> {
    for event in event_pump.poll_iter() {
        match event {
            sdl2::event::Event::DropFile { filename, .. } => {
                return Some(State::Running(create_emulator(&filename)));
            }
            sdl2::event::Event::Quit { .. } => {
                return Some(State::Exit);
            }
            sdl2::event::Event::KeyDown {
                scancode, repeat, ..
            } => {
                if repeat {
                    continue;
                }
                if let Some(gb_button) = scancode_to_gb_btn(scancode) {
                    input_vec.push(InputEvent {
                        down: true,
                        button: gb_button,
                    });
                }
            }
            sdl2::event::Event::KeyUp { scancode, .. } => {
                if let Some(gb_button) = scancode_to_gb_btn(scancode) {
                    input_vec.push(InputEvent {
                        down: false,
                        button: gb_button,
                    });
                }
            }
            sdl2::event::Event::ControllerButtonDown { which, button, .. } => {
                if let Some(gb_button) = controller_btn_to_gb_btn(button, which) {
                    input_vec.push(InputEvent {
                        down: true,
                        button: gb_button,
                    });
                }
            }
            sdl2::event::Event::ControllerButtonUp { which, button, .. } => {
                if let Some(gb_button) = controller_btn_to_gb_btn(button, which) {
                    input_vec.push(InputEvent {
                        down: false,
                        button: gb_button,
                    });
                }
            }
            sdl2::event::Event::ControllerAxisMotion { axis, value, .. } => {
                let dead_zone = 10_000;

                if let Some((btn_neg, btn_pos)) = controller_axis_gb_btn(axis) {
                    input_vec.push(InputEvent {
                        down: value < -dead_zone,
                        button: btn_neg,
                    });
                    input_vec.push(InputEvent {
                        down: value > dead_zone,
                        button: btn_pos,
                    });
                }
            }
            _ => {}
        }
    }

    None
}

fn vsync_canvas(
    rt: &FrameBuffer,
    texture: &mut sdl2::render::Texture,
    canvas: &mut sdl2::render::WindowCanvas,
    last_frame: &mut time::Instant, // @todo - Refactor into a system
) {
    texture
        .with_lock(None, |buffer, size| {
            for x in 0..160 {
                for y in 0..144 {
                    let color = rt[y][x];

                    let index = y * size + x * 3;
                    match color {
                        0..=3 => {
                            let c = PALETTE[color as usize];
                            buffer[index] = c.r;
                            buffer[index + 1] = c.g;
                            buffer[index + 2] = c.b;
                        }
                        _ => unreachable!(),
                    }
                }
            }
        })
        .unwrap();

    canvas.clear();
    canvas.copy(&texture, None, None).unwrap();
    canvas.present();

    let frame_time = last_frame.elapsed();
    *last_frame = time::Instant::now();
    canvas
        .window_mut()
        .set_title(
            format!(
                "fps={:?}",
                (1000_000 / std::cmp::max(frame_time.as_micros(), 1))
            )
            .as_str(),
        )
        .unwrap();
}

pub fn run_state(
    state: &mut State,
    canvas: &mut sdl2::render::WindowCanvas,
    event_pump: &mut sdl2::EventPump,
) -> State {
    match state {
        State::Idle => loop {
            for event in event_pump.wait_timeout_iter(100) {
                match event {
                    sdl2::event::Event::DropFile { filename, .. } => {
                        return State::Running(create_emulator(&filename));
                    }
                    sdl2::event::Event::Quit { .. } => {
                        return State::Exit;
                    }
                    _ => {}
                }
            }
        },
        State::Running(ref mut gb) => {
            let mut last_frame = time::Instant::now();
            let texture_creator = canvas.texture_creator();

            let mut texture = texture_creator
                .create_texture_streaming(sdl2::pixels::PixelFormatEnum::RGB24, 160, 144)
                .unwrap();

            let mut input_vec = Vec::new();
            let mut saved_at = time::Instant::now();

            loop {
                let start_time = time::Instant::now();

                if let Some(next_state) = poll_events(&mut input_vec, event_pump) {
                    return next_state;
                }

                if input_vec.len() > 0 {
                    gb.input_update(&input_vec);
                    input_vec.clear();
                }

                let mut cycles_left = M_CYCLES_PER_FRAME;
                while cycles_left > 0 {
                    let (cycles_run, vsync) = gb.run(cycles_left);

                    if vsync {
                        let rt = gb.get_framebuffer();
                        vsync_canvas(rt, &mut texture, canvas, &mut last_frame);
                    }

                    cycles_left = cycles_left.saturating_sub(cycles_run);
                }

                if saved_at.elapsed() > time::Duration::from_secs(60) {
                    gb.save();
                    saved_at = time::Instant::now();
                }

                let elapsed = start_time.elapsed().as_micros().try_into().unwrap();
                let sleep_time = FRAME_TIME.saturating_sub(elapsed);

                if sleep_time > 0 {
                    spin_sleep::sleep(time::Duration::from_micros(sleep_time));
                }
            }
        }
        State::Exit => {
            return State::Exit;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use colored::Colorize;
    use cpu::cpu::CPU;
    use rayon::prelude::*;
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::mpsc::{Receiver, SyncSender},
    };

    pub fn create_test_emulator(rom_path: &str) -> Option<Gameboy> {
        let cart = Cartridge::new(rom_path);
        let mut gb = Gameboy::new(cart);

        gb.dmg_boot();
        Some(gb)
    }

    fn run_test_emulator<T>(
        gb: &mut Gameboy,
        break_chan: Receiver<T>,
        input_chan: Option<&Receiver<InputEvent>>,
        frame_chan: Option<&SyncSender<FrameBuffer>>,
    ) -> Option<T> {
        let mcycles_per_frame = T_CYCLES_PER_FRAME / 4;
        let max_cycles = mcycles_per_frame * 120 * 60; // 120 seconds, 60 fps

        let mut trigger: Option<T> = None;
        let mut cycles_run = 0;

        while cycles_run < max_cycles {
            let (cycles_passed, vsync) = gb.run(mcycles_per_frame);

            if vsync {
                if let Some(fs) = frame_chan {
                    if let Err(err) = fs.send(*gb.get_framebuffer()) {
                        panic!("Frame sender error: {err}");
                    }
                }
            }

            if let Some(input) = input_chan {
                if let Ok(next_input) = input.try_recv() {
                    gb.input_update(&vec![next_input]);
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
            || cpu.l().get() != 34
        {
            return false;
        }

        return true;
    }

    fn mts_runner(rom_path: &str, _inputs: Option<Vec<GbButton>>) -> Option<bool> {
        let mut emu_res = create_test_emulator(rom_path);

        if let Some(gb) = &mut emu_res {
            let (break_send, break_recv) = std::sync::mpsc::channel::<u8>();

            gb.set_breakpoint(Some(break_send));

            let bp_triggered = run_test_emulator(gb, break_recv, None, None);
            let test_passed = bp_triggered.is_some() && mts_passed(gb.get_cpu());
            return Some(test_passed);
        } else {
            None
        }
    }

    fn snapshot_runner(rom_path: &str, inputs: Option<Vec<GbButton>>) -> Option<bool> {
        let root_path = PathBuf::from(rom_path.strip_prefix("tests/roms/").unwrap_or("unnamed"));

        let snapshot_dir = root_path
            .parent()
            .expect("must be a valid path")
            .to_str()
            .unwrap_or("unnamed")
            .to_string();

        return snapshot_test(rom_path, &snapshot_dir, inputs);
    }

    fn snapshot_test(
        rom_path: &str,
        snapshot_dir: &str,
        mut inputs: Option<Vec<GbButton>>,
    ) -> Option<bool> {
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

        let mut gb = emu_result.unwrap();

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

            let cmp_image = if let Ok(snapshot) = bmp::open(format!(
                "tests/snapshots/{snapshot_dir_string}/{rom_filename}.bmp"
            )) {
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
                                    input_send
                                        .send(InputEvent {
                                            button: press_next,
                                            down: true,
                                        })
                                        .unwrap();
                                    release_inputs.push((frame_count + 10, press_next));
                                }
                            }
                        }

                        release_inputs.retain(|release_next| {
                            if release_next.0 > frame_count {
                                return true;
                            }

                            input_send
                                .send(InputEvent {
                                    button: release_next.1,
                                    down: false,
                                })
                                .unwrap();
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
                                        || snapshot_pixel.b != palette_color.b
                                    {
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
                                            bmp::Pixel {
                                                r: palette_color.r,
                                                g: palette_color.g,
                                                b: palette_color.b,
                                            },
                                        );
                                    }
                                }

                                _ = bmp_img
                                    .save(format!("tests/snapshots/verify/{rom_filename}.bmp"));
                            }
                        }

                        drop(break_send);
                        break;
                    }
                }
            }
        });

        let frame_check_passed =
            run_test_emulator(&mut gb, break_recv, Some(&input_recv), Some(&frame_send));
        let test_passed = frame_check_passed.is_some();
        return Some(test_passed);
    }

    fn find_roms(rom_or_dir: &str) -> Vec<String> {
        let path = PathBuf::from(rom_or_dir);

        let roms = if path.is_file() {
            vec![rom_or_dir.to_string()]
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
        type RunnerFn = fn(path: &str, Option<Vec<GbButton>>) -> Option<bool>;

        #[rustfmt::skip]
        let test_roms: Vec<(RunnerFn, &str, Option<Vec<GbButton>>)>  = vec!(
            (snapshot_runner, "tests/roms/blargg/cpu_instrs/", None),
            (snapshot_runner, "tests/roms/rtc3test/rtc3test.0.gb", Some(vec![GbButton::GbButtonA])),
            (snapshot_runner, "tests/roms/rtc3test/rtc3test.1.gb", Some(vec![GbButton::GbButtonDown, GbButton::GbButtonA])),
            (snapshot_runner, "tests/roms/rtc3test/rtc3test.2.gb", Some(vec![GbButton::GbButtonDown, GbButton::GbButtonDown, GbButton::GbButtonA])),
            (snapshot_runner, "tests/roms/blargg/instr_timing/", None),
            (snapshot_runner, "tests/roms/blargg/dmg_sound/", None),
            (snapshot_runner, "tests/roms/blargg/mem_timing/", None),
            (snapshot_runner, "tests/roms/blargg/mem_timing-2/", None),
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

        results.iter().for_each(|(path, pass_opt)| {
            if let Some(is_passing) = pass_opt {
                if *is_passing {
                    println!("[{}]: {path}", "Passed".green().bold());
                } else {
                    eprintln!("[{}]: {path}", "Failed".red().bold());
                }
            }
        });

        let stats = results.iter().fold((0, 0, 0), |acc, res| {
            if let Some(is_passing) = res.1 {
                if is_passing {
                    return (acc.0 + 1, acc.1, acc.2);
                } else {
                    return (acc.0, acc.1 + 1, acc.2);
                }
            }
            return (acc.0, acc.1, acc.2 + 1);
        });

        println!(
            "{} passed {} failed, {} skipped, {} total",
            stats.0,
            stats.1,
            stats.2,
            results.len()
        );

        // assert!(result_vec.iter().all(|(_, pass)| *pass));
    }
}
