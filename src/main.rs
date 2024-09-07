use cartridge::cartridge::Cartridge;
use emu::emu::Emu;

mod cpu;
mod ppu;
mod emu;
mod util;
mod cartridge;

fn sdl2_create_window() -> (sdl2::render::Canvas<sdl2::video::Window>, sdl2::Sdl) {
    let sdl_ctx = sdl2::init().unwrap();
    let video_subsystem = sdl_ctx.video().unwrap();

    let window = video_subsystem.window("Gameboy", 160, 144)
        .position_centered()
        .build()
        .expect("could not create window");

    let canvas = window.into_canvas().build()
        .expect("could not create canvas");

    return (canvas, sdl_ctx);
}

fn main() {
    let (mut canvas, sdl_ctx) = sdl2_create_window();

    let cart = Cartridge::new("dev/rgbds/gb_helloworld.gb");

    let mut emu = Emu::new(cart, Box::new(move |rt| {
        canvas.clear();

        for x in 0..160 {
            for y in 0..144 {
                let color = rt[y][x];
                if color == 3 {
                    canvas.set_draw_color(sdl2::pixels::Color::RGB( 0x1b, 0x2a, 0x09));
                } else if color == 2 {
                    canvas.set_draw_color(sdl2::pixels::Color::RGB( 0x0e, 0x45, 0x0b));
                } else if color == 1 {
                    canvas.set_draw_color(sdl2::pixels::Color::RGB( 0x49, 0x6b, 0x22));
                } else if color == 0 {
                    canvas.set_draw_color(sdl2::pixels::Color::RGB( 0x9a, 0x9e, 0x3f));
                } else {
                    unreachable!();
                }
                // canvas.set_draw_color(sdl2::pixels::Color::RGB(color * 50, color * 50, color * 50));
                canvas.draw_point(sdl2::rect::Point::new(x.try_into().unwrap(), y.try_into().unwrap())).unwrap();
            }
        }

        canvas.present();

    }));

    emu.dmg_boot();

//    emu.run();

    let mut event_pump = sdl_ctx.event_pump().unwrap();

    'eventloop: loop {
        for event in event_pump.poll_iter() {
            match event {
                sdl2::event::Event::Quit {..} => {
                    break 'eventloop;
                }
                sdl2::event::Event::KeyDown {..} => {
                    println!("keydown lol");
                    break 'eventloop;
                },
                _ => {}
            }
        }

        // canvas.clear();

        // for x in 0..160 {
        //     for y in 0..144 {
        //         canvas.set_draw_color(sdl2::pixels::Color::RGB(y, x, 0));
        //         canvas.draw_point(sdl2::rect::Point::new(x.into(), y.into())).unwrap();
        //     }
        // }

        // canvas.present();

        emu.step();

        // std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
