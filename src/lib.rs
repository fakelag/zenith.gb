use std::{
    path::Path,
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

pub struct EmulatorContext {
    pub handle: std::thread::JoinHandle<()>,
    pub input_send: InputSender,
    pub rom_filename: String,
}

pub enum State {
    Idle,
    Running(Box<EmulatorContext>),
}

pub enum NextState {
    Exit,
    LoadRom(String),
}

pub const GB_DEFAULT_FPS: f64 = 59.73;
pub const TARGET_FPS: f64 = GB_DEFAULT_FPS;

const T_CYCLES_PER_SECOND: u64 = 4_194_304;
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
            "Zenith",
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
        .accelerated()
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

pub fn run_emulator(rom_path: &str, mut config: EmulatorConfig) -> EmulatorContext {
    let rom_path_string = rom_path.to_string();

    let (input_send, input_recv) = std::sync::mpsc::sync_channel::<InputEvent>(10);

    config.input_recv = Some(input_recv);

    let handle = std::thread::spawn(move || {
        let cart = Cartridge::new(&rom_path_string);
        let mut gb = Gameboy::new(cart, Box::new(config));
        gb.dmg_boot();
        gb.run();
    });

    let rom_filename = Path::new(&rom_path)
        .file_name()
        .expect("filename must exist")
        .to_str()
        .expect("filename must be valid utf-8");

    EmulatorContext {
        handle,
        input_send,
        rom_filename: rom_filename.to_string(),
    }
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

fn vsync_canvas(
    rt: &FrameBuffer,
    texture: &mut sdl2::render::Texture,
    canvas: &mut sdl2::render::WindowCanvas,
    num_frames: &mut u64,
    last_fps_update: &mut time::Instant,
    rom_filename: &str,
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

    let curtime = time::Instant::now();
    let dur = curtime.duration_since(*last_fps_update).as_secs_f64();
    if dur >= 4.0 {
        canvas
            .window_mut()
            .set_title(
                format!(
                    "Zenith ({:.1} fps) - {}",
                    (*num_frames as f64) / dur,
                    rom_filename
                )
                .as_str(),
            )
            .unwrap();

        *num_frames = 1;
        *last_fps_update = curtime;
    } else {
        *num_frames += 1;
    }
}

pub fn state_idle(event_pump: &mut sdl2::EventPump) -> Option<NextState> {
    for event in event_pump.poll_iter() {
        match event {
            sdl2::event::Event::DropFile { filename, .. } => {
                return Some(NextState::LoadRom(filename));
            }
            sdl2::event::Event::Quit { .. } => {
                return Some(NextState::Exit);
            }
            _ => {}
        }
    }

    None
}

pub fn state_running(
    ctx: &Box<EmulatorContext>,
    canvas: &mut sdl2::render::WindowCanvas,
    frame_recv: &std::sync::mpsc::Receiver<FrameBuffer>,
    event_pump: &mut sdl2::EventPump,
    sync_va: bool,
) -> Option<NextState> {
    let mut num_frames = 0;
    let mut last_fps_update = time::Instant::now();

    let texture_creator = canvas.texture_creator();

    let mut texture = texture_creator
        .create_texture_streaming(sdl2::pixels::PixelFormatEnum::RGB24, 160, 144)
        .unwrap();

    loop {
        let start_time = time::Instant::now();

        for event in event_pump.poll_iter() {
            match event {
                sdl2::event::Event::DropFile { filename, .. } => {
                    return Some(NextState::LoadRom(filename));
                }
                sdl2::event::Event::Quit { .. } => {
                    return Some(NextState::Exit);
                }
                sdl2::event::Event::KeyDown {
                    scancode, repeat, ..
                } => {
                    if repeat {
                        continue;
                    }
                    if let Some(gb_button) = scancode_to_gb_btn(scancode) {
                        ctx.input_send
                            .send(InputEvent {
                                down: true,
                                button: gb_button,
                            })
                            .unwrap();
                    }
                }
                sdl2::event::Event::KeyUp { scancode, .. } => {
                    if let Some(gb_button) = scancode_to_gb_btn(scancode) {
                        ctx.input_send
                            .send(InputEvent {
                                down: false,
                                button: gb_button,
                            })
                            .unwrap();
                    }
                }
                sdl2::event::Event::ControllerButtonDown { which, button, .. } => {
                    if let Some(gb_button) = controller_btn_to_gb_btn(button, which) {
                        ctx.input_send
                            .send(InputEvent {
                                down: true,
                                button: gb_button,
                            })
                            .unwrap();
                    }
                }
                sdl2::event::Event::ControllerButtonUp { which, button, .. } => {
                    if let Some(gb_button) = controller_btn_to_gb_btn(button, which) {
                        ctx.input_send
                            .send(InputEvent {
                                down: false,
                                button: gb_button,
                            })
                            .unwrap();
                    }
                }
                sdl2::event::Event::ControllerAxisMotion { axis, value, .. } => {
                    let dead_zone = 10_000;

                    if let Some((btn_neg, btn_pos)) = controller_axis_gb_btn(axis) {
                        ctx.input_send
                            .send(InputEvent {
                                down: value < -dead_zone,
                                button: btn_neg,
                            })
                            .unwrap();
                        ctx.input_send
                            .send(InputEvent {
                                down: value > dead_zone,
                                button: btn_pos,
                            })
                            .unwrap();
                    }
                }
                _ => {}
            }
        }

        match frame_recv.recv() {
            Ok(rt) => {
                vsync_canvas(
                    &rt,
                    &mut texture,
                    canvas,
                    &mut num_frames,
                    &mut last_fps_update,
                    &ctx.rom_filename,
                );
            }
            Err(_err) => panic!("frame channel should not get dropped"),
        }

        let elapsed = start_time.elapsed().as_micros().try_into().unwrap();
        let sleep_time = FRAME_TIME.saturating_sub(elapsed);

        if sync_va && sleep_time > 0 {
            spin_sleep::sleep(time::Duration::from_micros(sleep_time));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use colored::Colorize;
    use rayon::prelude::*;
    use std::{
        fs,
        path::{Path, PathBuf},
    };
    use util::util;

    fn mts_passed(regs: (u16, u16, u16)) -> bool {
        if util::get_high(regs.0) != 3
            || util::get_low(regs.0) != 5
            || util::get_high(regs.1) != 8
            || util::get_low(regs.1) != 13
            || util::get_high(regs.2) != 21
            || util::get_low(regs.2) != 34
        {
            return false;
        }
        return true;
    }

    fn mts_runner(rom_path: &str, _inputs: Option<Vec<GbButton>>) -> Option<bool> {
        let (frame_send, frame_recv) = std::sync::mpsc::sync_channel::<ppu::ppu::FrameBuffer>(1);
        let (break_send, break_recv) = std::sync::mpsc::sync_channel::<(u16, u16, u16)>(1);

        let emu_ctx = run_emulator(
            rom_path,
            EmulatorConfig {
                enable_saving: false,
                sync_audio: false,
                sync_video: false,
                bp_chan: Some(break_send),
                frame_chan: Some(frame_send),
                sound_chan: None,
                input_recv: None,
                max_cycles: Some(T_CYCLES_PER_SECOND * 120),
            },
        );

        let test_passed = match break_recv.recv_timeout(time::Duration::from_secs(5)) {
            Ok(regs) => mts_passed(regs),
            Err(_) => false,
        };

        drop(frame_recv);
        emu_ctx.handle.join().unwrap();

        return Some(test_passed);
    }

    fn snapshot_runner(rom_path: &str, mut inputs: Option<Vec<GbButton>>) -> Option<bool> {
        let root_path = PathBuf::from(rom_path.strip_prefix("tests/roms/").unwrap_or("unnamed"));

        let snapshot_dir = root_path
            .parent()
            .expect("must be a valid path")
            .to_str()
            .unwrap_or("unnamed")
            .to_string();

        let (frame_send, frame_recv) = std::sync::mpsc::sync_channel::<FrameBuffer>(2);

        if let Some(input_list) = &mut inputs {
            input_list.reverse();
        }

        let emu_ctx = run_emulator(
            rom_path,
            EmulatorConfig {
                enable_saving: false,
                sync_audio: false,
                // Note: sync video to guarantee receiving every frame for snapshot comparison
                sync_video: true,
                bp_chan: None,
                frame_chan: Some(frame_send),
                sound_chan: None,
                input_recv: None,
                max_cycles: Some(T_CYCLES_PER_SECOND * 120),
            },
        );

        let rom_path_string = rom_path.to_string();
        let snapshot_dir_string = snapshot_dir.to_string();

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

        let passed = loop {
            match frame_recv.recv_timeout(time::Duration::from_secs(5)) {
                Ok(rt) => {
                    last_frame = Some(rt);
                    frame_count += 1;

                    if let Some(input_vector) = &mut inputs {
                        if frame_count > 60 && frame_count % 20 == 0 {
                            if let Some(press_next) = input_vector.pop() {
                                emu_ctx
                                    .input_send
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

                        emu_ctx
                            .input_send
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
                            break true;
                        }
                    }
                }
                Err(_) => {
                    break false;
                }
            }
        };

        if !passed && cmp_image.is_none() {
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

                _ = bmp_img.save(format!("tests/snapshots/verify/{rom_filename}.bmp"));
            }
        }

        drop(frame_recv);
        emu_ctx.handle.join().unwrap();

        return Some(passed);
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
