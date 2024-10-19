#![cfg_attr(not(any(test, debug_assertions)), windows_subsystem = "windows")]

extern crate gbemu_lib;

use gameboy::gameboy::EmulatorConfig;
use gbemu_lib::*;

fn main() {
    let enable_saving = true;
    let sync_video = true;
    let sync_audio = true;

    let sdl_ctx = sdl2::init().unwrap();
    let mut canvas = sdl2_create_window(&sdl_ctx);

    let (_ad, sound_chan) = sdl2_create_audio(&sdl_ctx);

    let _controller = sdl2_enable_controller(&sdl_ctx);

    let (mut frame_send, mut frame_recv) =
        std::sync::mpsc::sync_channel::<ppu::ppu::FrameBuffer>(1);

    let mut event_pump = sdl_ctx.event_pump().unwrap();
    let mut state = State::Idle;

    if let Some(preload_rom) = std::env::args().nth(1) {
        state = State::Running(Box::new(run_emulator(
            &preload_rom,
            EmulatorConfig {
                bp_chan: None,
                sound_chan: Some(sound_chan.clone()),
                frame_chan: Some(frame_send.clone()),
                input_recv: None,
                max_cycles: None,
                enable_saving,
                sync_audio,
                sync_video,
            },
        )));
    }

    'eventloop: loop {
        let next_state = match state {
            State::Idle => state_idle(&mut event_pump),
            State::Running(ref ctx) => {
                state_running(ctx, &mut canvas, &frame_recv, &mut event_pump, sync_video)
            }
        };

        match next_state {
            Some(NextState::LoadRom(rom_path)) => {
                if let State::Running(ctx) = state {
                    drop(frame_recv);
                    _ = ctx.handle.join();
                    (frame_send, frame_recv) =
                        std::sync::mpsc::sync_channel::<ppu::ppu::FrameBuffer>(1);
                }

                state = State::Running(Box::new(run_emulator(
                    &rom_path,
                    EmulatorConfig {
                        bp_chan: None,
                        sound_chan: Some(sound_chan.clone()),
                        frame_chan: Some(frame_send.clone()),
                        input_recv: None,
                        max_cycles: None,
                        enable_saving,
                        sync_audio,
                        sync_video,
                    },
                )));
            }
            Some(NextState::Exit) => {
                break 'eventloop;
            }
            None => {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            _ => {}
        }
    }
}
