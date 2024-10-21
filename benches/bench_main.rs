use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;
use zenith_lib::{gameboy::gameboy::EmulatorConfig, ppu::ppu, run_emulator};

pub fn run_bench(rom_path: &str, num_cycles: u64) {
    let (sound_send, sound_recv) = std::sync::mpsc::sync_channel::<Vec<i16>>(1);
    let (frame_send, frame_recv) = std::sync::mpsc::sync_channel::<ppu::FrameBuffer>(1);

    let emu_ctx = run_emulator(
        rom_path,
        EmulatorConfig {
            bp_chan: None,
            sound_chan: Some(sound_send),
            frame_chan: Some(frame_send),
            input_recv: None,
            max_cycles: Some(num_cycles),
            enable_saving: false,
            sync_audio: false,
            sync_video: false,
        },
    );

    let a_thread = std::thread::spawn(move || loop {
        match sound_recv.recv() {
            Ok(val) => {
                black_box(val);
            }
            Err(_err) => break,
        }
    });

    loop {
        match frame_recv.recv() {
            Ok(val) => {
                black_box(val);
            }
            Err(_err) => break,
        }
    }

    emu_ctx.handle.join().unwrap();
    a_thread.join().unwrap();
}

fn cpu_instrs_bench() {
    let cycles = 56_152_830;
    run_bench("./tests/roms/blargg/cpu_instrs/cpu_instrs.gb", cycles);
}

fn ppu_bench() {
    let cycles = 56_152_830;
    run_bench("./dev/rgbds/gb_sprites_and_tiles.gb", cycles);
}

fn criterion_benchmark(c: &mut Criterion) {
    let mut g = c.benchmark_group("bench_group");

    g.sample_size(10);
    g.bench_function("cpu_instrs", |b| b.iter(|| cpu_instrs_bench()));
    g.bench_function("ppu_bench", |b| b.iter(|| ppu_bench()));
    g.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
