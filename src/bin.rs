#![cfg_attr(not(any(test, debug_assertions)), windows_subsystem = "windows")]

extern crate gbemu_lib;

use gbemu_lib::*;

fn main() {
    let sdl_ctx = sdl2::init().unwrap();
    let mut canvas = sdl2_create_window(&sdl_ctx);

    let (_ad, sound_chan) = sdl2_create_audio(&sdl_ctx);

    let _controller = sdl2_enable_controller(&sdl_ctx);

    let mut event_pump = sdl_ctx.event_pump().unwrap();
    let mut state = State::Idle;

    'eventloop: loop {
        let mut next_state = run_state(&mut state, &mut canvas, &mut event_pump);

        match state {
            State::Running(ref mut gb) => {
                gb.close();
            }
            _ => {}
        }

        match next_state {
            State::Exit => {
                break 'eventloop;
            }
            State::Running(ref mut gb) => {
                gb.enable_external_audio(sound_chan.clone());
            }
            _ => {}
        }

        state = next_state;
    }
}
