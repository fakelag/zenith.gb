use std::sync::mpsc::SyncSender;

use crate::util::util;

use super::{
    channel1::Channel1,
    channel2::Channel2,
    channel3::Channel3,
    channel4::Channel4,
    wav_file::write_wav,
    Channel
};

pub type ApuSampleBuffer = [(u8, u8); 4096];
pub type ApuSoundSender = SyncSender<ApuSampleBuffer>;

const FRAME_SEQUENCER_START: u16 = (4_194_304 / 512 as u32) as u16;
const SAMPLE_COUNTER_START: u16 = (4_194_304 / 44_100 as u32) as u16;

const RECORD_WAV_FILE: bool = false;

pub struct APU {
    channel1: Channel1,
    channel2: Channel2,
    channel3: Channel3,
    channel4: Channel4,

    sample_counter: u16,
    sound_chan: Option<ApuSoundSender>,

    tmp: Vec<i16>,
    sample_buffer: ApuSampleBuffer,
    sample_count: usize,
    frame_sequencer: u16,
    frame_sequencer_step: u8,

    audio_enabled: bool,
    right_pan: [bool; 4],
    right_vol: u8,
    left_pan: [bool; 4],
    left_vol: u8,
}

impl APU {
    pub fn new(sound_chan: Option<ApuSoundSender>) -> Self {
        Self {
            channel1: Channel1::new(),
            channel2: Channel2::new(),
            channel3: Channel3::new(),
            channel4: Channel4::new(),
            sample_counter: SAMPLE_COUNTER_START,
            frame_sequencer: FRAME_SEQUENCER_START,
            frame_sequencer_step: 0,
            sound_chan,
            sample_buffer: [(0, 0); 4096],
            sample_count: 0,
            tmp: Vec::new(),
            audio_enabled: true,
            right_pan: [false, false, true, true],
            right_vol: 7,
            left_pan: [true; 4],
            left_vol: 7,
        }
    }

    pub fn close(&mut self) {
        if RECORD_WAV_FILE {
            if self.tmp.len() == 0 {
                return;
            }

            write_wav("dev/test.wav", &self.tmp);
        }
    }

    pub fn step(&mut self, cycles: u8) {
        for _c in 0..(cycles * 4) {
            self.frame_sequencer();

            for channel in self.get_channels() {
                channel.step();
            }

            self.sample_audio();
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
                for counter in self.get_channels() {
                    counter.get_length_counter().step();
                }

                if self.frame_sequencer_step == 2 || self.frame_sequencer_step == 6 {
                    self.channel1.sweep_step();
                }
            }
            7 => {
                for channel in self.get_channels() {
                    if let Some(envelope) = channel.get_envelope() {
                        envelope.step();
                    }
                }
            }
            1 | 3 | 5 => {}
            _ => unreachable!(),
        }

        self.frame_sequencer_step = (self.frame_sequencer_step + 1) & 0x7;
    }

    pub fn sample_audio(&mut self) {
        self.sample_counter -= 1;

        if self.sample_counter > 0 {
            return;
        }

        self.sample_counter = SAMPLE_COUNTER_START;

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

        let left: u8 = left_scaled as u8;
        let right: u8 = right_scaled as u8;

        if self.sample_count < 4096 {
            self.sample_buffer[self.sample_count] = (left, right);
            self.sample_count += 1;
        } else {
            if let Some(sound_chan) = &self.sound_chan {
                // try_send for non-synced audio with the compromise of audio glitches due to
                // missing sample buffers in-between
                sound_chan.send(self.sample_buffer).unwrap();
                self.sample_count = 0;
            }
        }

        if RECORD_WAV_FILE {
            self.tmp.push(util::audio_sample_u8_to_i16(left));
            self.tmp.push(util::audio_sample_u8_to_i16(right));
        }
    }

    pub fn get_channel1(&mut self) -> &mut Channel1 {
        &mut self.channel1
    }

    pub fn get_channel2(&mut self) -> &mut Channel2 {
        &mut self.channel2
    }

    pub fn get_channel3(&mut self) -> &mut Channel3 {
        &mut self.channel3
    }

    pub fn write_nr50(&mut self, data: u8) {
        self.right_vol = data & 0x7;
        self.left_vol = (data >> 4) & 0x7;
    }

    pub fn write_nr51(&mut self, data: u8) {
        for i in 0..4 {
            self.right_pan[i] = (data >> i) & 0x1 != 0;
            self.left_pan[i] = (data >> (i+4)) & 0x1 != 0;
        }
    }

    pub fn write_nr52(&mut self, data: u8) {
        // @todo Audio on/off: Turns all APU registers RO, except NR52 and length timers (NRx1)
        self.audio_enabled = data & 0x80 != 0;
        // @todo Turning audio off should reset all APU registers
        // @todo Reset frame sequencer when switching off->on
    }

    pub fn read_nr50(&mut self) -> u8 {
        self.right_vol | (self.left_vol << 4)
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
        let enable_bit = if self.audio_enabled { 0x80 } else { 0x0 };

        enable_bit |
            (if self.channel3.is_enabled() { 1 << 3 } else { 0 })
    }

    fn get_volume_scale(vol: u8) -> f32 {
        f32::from(vol + 1) / 8.0
    }

    fn get_channels(&mut self) -> [&mut dyn Channel; 4] {
        [
            &mut self.channel1,
            &mut self.channel2,
            &mut self.channel3,
            &mut self.channel4
        ]
    }
}
