use std::sync::mpsc::SyncSender;

use crate::{GB_DEFAULT_FPS, TARGET_FPS};

use super::{
    audiocvt::AudioCVT, channel1::Channel1, channel2::Channel2, channel3::Channel3,
    channel4::Channel4, wav_file::write_wav, Channel,
};

pub const APU_FREQ: u32 = 44_100;
pub const APU_SAMPLES_PER_CHANNEL: u16 = 4096;
pub const APU_NUM_CHANNELS: u8 = 2;
pub const APU_SAMPLES: usize = APU_SAMPLES_PER_CHANNEL as usize * APU_NUM_CHANNELS as usize;

pub type ApuSoundSender = SyncSender<Vec<i16>>;

pub type AudioBuffer = Vec<u8>;

const T_CYCLES_PER_FRAME: u64 = (4_194_304.0 * (TARGET_FPS / GB_DEFAULT_FPS)) as u64;
const FRAME_SEQUENCER_START: u16 = (T_CYCLES_PER_FRAME / (512.0 * 4.0) as u64) as u16;
const SAMPLE_COUNTER_START: u16 = (T_CYCLES_PER_FRAME / APU_FREQ as f64 as u64) as u16;

const RECORD_WAV_FILE: bool = false;

pub struct APU {
    channel1: Channel1,
    channel2: Channel2,
    channel3: Channel3,
    channel4: Channel4,

    sample_counter: u16,
    sample_output: Option<(ApuSoundSender, AudioCVT)>,
    sync_audio: bool,

    wav_data: Vec<i16>,
    sample_buffer: AudioBuffer,
    frame_sequencer: u16,
    frame_sequencer_step: u8,

    audio_enabled: bool,
    right_pan: [bool; 4],
    right_vol: u8,
    left_pan: [bool; 4],
    left_vol: u8,
    left_vin: bool,
    right_vin: bool,
}

impl APU {
    pub fn new(sound_chan: Option<ApuSoundSender>, sync_audio: bool) -> Self {
        let mut apu = Self {
            sync_audio,
            sample_output: None,
            channel1: Channel1::new(),
            channel2: Channel2::new(),
            channel3: Channel3::new(),
            channel4: Channel4::new(),
            sample_counter: SAMPLE_COUNTER_START,
            frame_sequencer: FRAME_SEQUENCER_START,
            frame_sequencer_step: 0,
            sample_buffer: Vec::with_capacity(APU_SAMPLES),
            wav_data: Vec::new(),
            audio_enabled: true,
            right_pan: [true, true, false, false],
            right_vol: 7,
            left_pan: [true; 4],
            left_vol: 7,
            left_vin: false,
            right_vin: false,
        };

        if let Some(channel) = sound_chan {
            apu.sample_output = Some((channel, AudioCVT::new()));
        }

        apu
    }

    pub fn close(&mut self) {
        if RECORD_WAV_FILE {
            if self.wav_data.len() == 0 {
                return;
            }

            write_wav("dev/test.wav", &self.wav_data);
        }
    }

    pub fn clock(&mut self) {
        self.frame_sequencer();

        for _c in 0..4 {
            self.channel1.clock();
            self.channel2.clock();
            self.channel3.clock();
            self.channel4.clock();

            self.sample_counter -= 1;

            if self.sample_counter == 0 {
                self.sample_counter = SAMPLE_COUNTER_START;
                self.sample_audio();
            }
        }
    }

    pub fn frame_sequencer(&mut self) {
        // Frame sequencer is clocked at 512 Hz and triggers the following:
        // Step   Length Ctr  Vol Env     Sweep
        // ---------------------------------------
        // 0      Clock       -           -
        // 1      -           -           -
        // 2      Clock       -           Clock
        // 3      -           -           -
        // 4      Clock       -           -
        // 5      -           -           -
        // 6      Clock       -           Clock
        // 7      -           Clock       -
        // ---------------------------------------
        // Rate   256 Hz      64 Hz       128 Hz
        self.frame_sequencer -= 1;

        if self.frame_sequencer != 0 {
            return;
        }

        self.frame_sequencer = FRAME_SEQUENCER_START;

        match self.frame_sequencer_step {
            0 | 2 | 4 | 6 => {
                for channel in self.get_channels() {
                    channel.length_clock();
                }

                if self.frame_sequencer_step == 2 || self.frame_sequencer_step == 6 {
                    self.channel1.sweep_clock();
                }
            }
            7 => {
                for channel in self.get_channels() {
                    if let Some(envelope) = channel.get_envelope() {
                        envelope.clock();
                    }
                }
            }
            1 | 3 | 5 => {}
            _ => unreachable!(),
        }

        let next_step = (self.frame_sequencer_step + 1) & 0x7;
        self.frame_sequencer_step = next_step;

        for channel in self.get_channels() {
            channel
                .get_length_counter()
                .update_frame_sequencer_step(next_step);
        }
    }

    pub fn sample_audio(&mut self) {
        let mut left_scaled = 0.0;
        let mut right_scaled = 0.0;

        for i in 0..4 {
            let channel = &*self.get_channels()[i];

            let sample = f32::from(channel.get_sample());

            if self.left_pan[i] {
                left_scaled += sample * APU::get_volume_scale(self.left_vol);
            }
            if self.right_pan[i] {
                right_scaled += sample * APU::get_volume_scale(self.right_vol);
            }
        }

        if let Some((chan, cvt)) = &self.sample_output {
            let left: u8 = left_scaled as u8;
            let right: u8 = right_scaled as u8;

            self.sample_buffer.push(left);
            self.sample_buffer.push(right);

            if self.sample_buffer.len() < APU_SAMPLES {
                return;
            }

            debug_assert!(self.sample_buffer.len() == APU_SAMPLES);

            let cvt_audio = cvt.convert_u8_i16(&self.sample_buffer);

            if RECORD_WAV_FILE {
                for c in cvt_audio.iter() {
                    self.wav_data.push(*c);
                }
            }

            if self.sync_audio {
                chan.send(cvt_audio).unwrap();
            } else {
                _ = chan.try_send(cvt_audio);
            }
            self.sample_buffer.clear();
        }
    }

    pub fn write_nr50(&mut self, data: u8) {
        if !self.audio_enabled {
            return;
        }

        self.right_vol = data & 0x7;
        self.right_vin = data & 0x8 != 0;
        self.left_vol = (data >> 4) & 0x7;
        self.left_vin = data & 0x80 != 0;
    }

    pub fn write_nr51(&mut self, data: u8) {
        if !self.audio_enabled {
            return;
        }

        for i in 0..4 {
            self.right_pan[i] = (data >> i) & 0x1 != 0;
            self.left_pan[i] = (data >> (i + 4)) & 0x1 != 0;
        }
    }

    pub fn write_nr52(&mut self, data: u8) {
        let enable_bit = data & 0x80 != 0;

        match (self.audio_enabled, enable_bit) {
            (true, false) => {
                self.left_vin = false;
                self.right_vin = false;
                self.left_vol = 0;
                self.right_vol = 0;

                for i in 0..4 {
                    self.left_pan[i] = false;
                    self.right_pan[i] = false;
                    self.get_channels()[i].shutdown();
                }
            }
            (false, true) => {
                self.frame_sequencer_step = 0;
            }
            _ => {}
        }

        self.audio_enabled = enable_bit;
    }

    pub fn read_nr50(&mut self) -> u8 {
        self.right_vol
            | (self.left_vol << 4)
            | ((self.left_vin as u8) << 7)
            | ((self.right_vin as u8) << 3)
    }

    pub fn read_nr51(&mut self) -> u8 {
        return (0..4)
            .into_iter()
            .map(|i| {
                let bottom_bit = if self.right_pan[i] { 1 << i } else { 0 };
                let top_bit = if self.left_pan[i] { 1 << (i + 4) } else { 0 };
                bottom_bit | top_bit
            })
            .fold(0, |acc, curr| acc | curr);
    }

    pub fn read_nr52(&mut self) -> u8 {
        let enable_bit = (self.audio_enabled as u8) << 7;

        let channel_enable_bits = (0..4)
            .into_iter()
            .map(|i| {
                let channel_enable_bit = (self.get_channels()[i].is_enabled() as u8) * (1 << i);
                channel_enable_bit
            })
            .fold(0, |acc, curr| acc | curr);

        enable_bit | channel_enable_bits | 0x70
    }

    pub fn write_nr10(&mut self, data: u8) {
        if !self.audio_enabled {
            return;
        }
        self.channel1.write_nr10(data);
    }

    pub fn write_nr11(&mut self, data: u8) {
        // @todo CGB: Audio disabled prevents writing to length counters
        self.channel1.write_nr11(if self.audio_enabled {
            data
        } else {
            data & 0x3F
        });
    }

    pub fn write_nr12(&mut self, data: u8) {
        if !self.audio_enabled {
            return;
        }
        self.channel1.write_nr12(data);
    }

    pub fn write_nr13(&mut self, data: u8) {
        if !self.audio_enabled {
            return;
        }
        self.channel1.write_nr13(data);
    }

    pub fn write_nr14(&mut self, data: u8) {
        if !self.audio_enabled {
            return;
        }
        self.channel1.write_nr14(data);
    }

    pub fn write_nr21(&mut self, data: u8) {
        // @todo CGB: Audio disabled prevents writing to length counters
        self.channel2.write_nr21(if self.audio_enabled {
            data
        } else {
            data & 0x3F
        });
    }

    pub fn write_nr22(&mut self, data: u8) {
        if !self.audio_enabled {
            return;
        }
        self.channel2.write_nr22(data);
    }

    pub fn write_nr23(&mut self, data: u8) {
        if !self.audio_enabled {
            return;
        }
        self.channel2.write_nr23(data);
    }

    pub fn write_nr24(&mut self, data: u8) {
        if !self.audio_enabled {
            return;
        }
        self.channel2.write_nr24(data);
    }

    pub fn write_wave_ram(&mut self, address: u16, data: u8) {
        self.channel3.write_wave_ram(usize::from(address), data);
    }

    pub fn write_nr30(&mut self, data: u8) {
        if !self.audio_enabled {
            return;
        }
        self.channel3.write_nr30(data);
    }

    pub fn write_nr31(&mut self, data: u8) {
        // @todo CGB: Audio disabled prevents writing to length counters
        self.channel3.write_nr31(data);
    }

    pub fn write_nr32(&mut self, data: u8) {
        if !self.audio_enabled {
            return;
        }
        self.channel3.write_nr32(data);
    }

    pub fn write_nr33(&mut self, data: u8) {
        if !self.audio_enabled {
            return;
        }
        self.channel3.write_nr33(data);
    }

    pub fn write_nr34(&mut self, data: u8) {
        if !self.audio_enabled {
            return;
        }
        self.channel3.write_nr34(data);
    }

    pub fn write_nr41(&mut self, data: u8) {
        // @todo CGB: Audio disabled prevents writing to length counters
        self.channel4.write_nr41(data);
    }

    pub fn write_nr42(&mut self, data: u8) {
        if !self.audio_enabled {
            return;
        }
        self.channel4.write_nr42(data);
    }

    pub fn write_nr43(&mut self, data: u8) {
        if !self.audio_enabled {
            return;
        }
        self.channel4.write_nr43(data);
    }

    pub fn write_nr44(&mut self, data: u8) {
        if !self.audio_enabled {
            return;
        }
        self.channel4.write_nr44(data);
    }

    pub fn read_nr10(&mut self) -> u8 {
        self.channel1.read_nr10()
    }

    pub fn read_nr11(&mut self) -> u8 {
        self.channel1.read_nr11()
    }

    pub fn read_nr12(&mut self) -> u8 {
        self.channel1.read_nr12()
    }

    pub fn read_nr13(&mut self) -> u8 {
        self.channel1.read_nr13()
    }

    pub fn read_nr14(&mut self) -> u8 {
        self.channel1.read_nr14()
    }

    pub fn read_nr21(&mut self) -> u8 {
        self.channel2.read_nr21()
    }

    pub fn read_nr22(&mut self) -> u8 {
        self.channel2.read_nr22()
    }

    pub fn read_nr23(&mut self) -> u8 {
        self.channel2.read_nr23()
    }

    pub fn read_nr24(&mut self) -> u8 {
        self.channel2.read_nr24()
    }

    pub fn read_nr30(&mut self) -> u8 {
        self.channel3.read_nr30()
    }

    pub fn read_nr31(&mut self) -> u8 {
        self.channel3.read_nr31()
    }

    pub fn read_nr32(&mut self) -> u8 {
        self.channel3.read_nr32()
    }

    pub fn read_nr33(&mut self) -> u8 {
        self.channel3.read_nr33()
    }

    pub fn read_nr34(&mut self) -> u8 {
        self.channel3.read_nr34()
    }

    pub fn read_nr41(&mut self) -> u8 {
        self.channel4.read_nr41()
    }

    pub fn read_nr42(&mut self) -> u8 {
        self.channel4.read_nr42()
    }

    pub fn read_nr43(&mut self) -> u8 {
        self.channel4.read_nr43()
    }

    pub fn read_nr44(&mut self) -> u8 {
        self.channel4.read_nr44()
    }

    pub fn read_wave_ram(&mut self, address: u16) -> u8 {
        self.channel3.read_wave_ram(usize::from(address))
    }

    fn get_volume_scale(vol: u8) -> f32 {
        f32::from(vol + 1) / 8.0
    }

    fn get_channels(&mut self) -> [&mut dyn Channel; 4] {
        [
            &mut self.channel1,
            &mut self.channel2,
            &mut self.channel3,
            &mut self.channel4,
        ]
    }
}
