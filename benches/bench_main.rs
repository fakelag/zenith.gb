use criterion::{criterion_group, criterion_main, Criterion};
use gbemu_lib::{cartridge::cartridge::Cartridge, gameboy::gameboy::Gameboy};
use std::hint::black_box;

pub fn create_test_emulator(rom_path: &str) -> Gameboy {
    let cart = Cartridge::new(rom_path);
    let mut gb = Gameboy::new(cart);

    gb.dmg_boot();
    gb
}

fn run_test_emulator(gb: &mut Gameboy, cycles: u64) -> u64 {
    let mut cycles_run = 0;

    while cycles_run < cycles {
        let (cycles_passed, vsync) = gb.run(cycles - cycles_run);

        if vsync {
            black_box(gb.get_framebuffer());
        }

        cycles_run += cycles_passed;
    }

    cycles_run
}

fn cpu_instrs_bench() {
    let cycles = 56_152_830;

    let mut gb = create_test_emulator("./tests/roms/blargg/cpu_instrs/cpu_instrs.gb");
    black_box(run_test_emulator(&mut gb, cycles));
}

fn ppu_bench() {
    let cycles = 56_152_830;

    let mut gb = create_test_emulator("./dev/rgbds/gb_sprites_and_tiles.gb");
    black_box(run_test_emulator(&mut gb, cycles));
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