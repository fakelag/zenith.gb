use crate::apu::apu::APU_SAMPLES;

pub struct AudioCVT {
    cvt: sdl2::audio::AudioCVT,
}

impl AudioCVT {
    pub fn new() -> Self {
        let cvt = sdl2::audio::AudioCVT::new(
            sdl2::audio::AudioFormat::S8,
            2,
            44_100,
            sdl2::audio::AudioFormat::s16_sys(),
            2,
            44_100,
        )
        .expect("conversion should be possible");

        Self { cvt }
    }

    pub fn convert_u8_i16(&self, src: &Vec<u8>) -> Vec<i16> {
        // We could roll out our own converter, which might be faster and remove sdl dependency
        // from APU code. @todo - Benchmark and test. Something similar to (i16::from(sample) - 32) << 10 but
        // with 0 value as silence

        let converted_audio: Vec<_> = self
            .cvt
            .convert(src.to_vec())
            .chunks_exact(2)
            .map(|sample_byte| i16::from_ne_bytes([sample_byte[0], sample_byte[1]]))
            .collect();

        debug_assert!(converted_audio.len() == APU_SAMPLES);
        converted_audio
    }
}
