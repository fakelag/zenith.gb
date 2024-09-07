use cartridge::cartridge::Cartridge;

mod cpu;
mod ppu;
mod emu;
mod util;
mod cartridge;

use emu::emu::{Emu, FrameBuffer};

fn sdl2_create_window() -> (sdl2::render::Canvas<sdl2::video::Window>, sdl2::Sdl) {
    let sdl_ctx = sdl2::init().unwrap();

    let video_subsystem = sdl_ctx.video().unwrap();

    let asp = 144.0 / 160.0;

    let window = video_subsystem.window("Gameboy", 512, (512.0 * asp) as u32)
        .position_centered()
        .opengl()
        .build()
        .expect("could not create window");

    let canvas = window.into_canvas().build()
        .expect("could not create canvas");

    return (canvas, sdl_ctx);
}

const PALETTE: [sdl2::pixels::Color; 4] = [
    sdl2::pixels::Color::RGB(0x9a, 0x9e, 0x3f),
    sdl2::pixels::Color::RGB(0x49, 0x6b, 0x22),
    sdl2::pixels::Color::RGB(0x0e, 0x45, 0x0b),
    sdl2::pixels::Color::RGB(0x1b, 0x2a, 0x09)
];

fn main() {
    let (mut canvas, sdl_ctx) = sdl2_create_window();

    let cart = Cartridge::new("dev/rgbds/gb_helloworld.gb");

    let texture_creator = canvas.texture_creator();

    let mut texture = texture_creator
        .create_texture_streaming(sdl2::pixels::PixelFormatEnum::RGB24, 160, 144)
        .unwrap();

    let (frame_sender, frame_receiver) = std::sync::mpsc::sync_channel::<FrameBuffer>(1);

    let mut emu = Emu::new(cart, frame_sender);
    let emu_thread = std::thread::spawn(move || emu.run());
    
    let mut last_frame = std::time::Instant::now();
    let mut event_pump = sdl_ctx.event_pump().unwrap();
    'eventloop: loop {
        for event in event_pump.poll_iter() {
            match event {
                sdl2::event::Event::Quit {..} => {
                    break 'eventloop;
                }
                sdl2::event::Event::KeyDown {..} => {
                    println!("keydown lol");
                },
                _ => {}
            }
        }

        match frame_receiver.recv() {
            Ok(rt) => {
                let frame_time = last_frame.elapsed();
                
                canvas.window_mut().set_title(format!("fps={:?}", 1000 / std::cmp::max(frame_time.as_millis(), 1)).as_str()).unwrap();

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
                last_frame = std::time::Instant::now();
            },
            Err(..) => break 'eventloop,
        }

        // std::thread::sleep(std::time::Duration::from_millis(100));
    }

    drop(frame_receiver);
    emu_thread.join().unwrap();
}
