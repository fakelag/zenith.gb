use std::{fs, io::Write, sync::mpsc::Sender};

use super::{channel3::Channel3, Channel};

pub type ApuSample = (u8, u8);

const FRAME_SEQUENCER_START: u16 = (4_194_304 / 512 as u32) as u16;
const SAMPLE_COUNTER_START: u8 = 95;

pub struct APU {
    channel3: Channel3,
    sample_counter: u8,
    sound_chan: Option<Sender<ApuSample>>,

    tmp: Vec<i16>,
    frame_sequencer: u16,

    audio_enabled: bool,
    right_pan: [bool; 4],
    right_vol: u8,
    left_pan: [bool; 4],
    left_vol: u8,
}

impl APU {
    pub fn new(sound_chan: Option<Sender<ApuSample>>) -> Self {
        Self {
            channel3: Channel3::new(),
            sample_counter: SAMPLE_COUNTER_START,
            frame_sequencer: FRAME_SEQUENCER_START,
            sound_chan,
            tmp: Vec::new(),
            audio_enabled: true,
            right_pan: [false, false, true, true],
            right_vol: 7,
            left_pan: [true; 4],
            left_vol: 7,
        }
    }

    pub fn close(&mut self) {
        if self.tmp.len() == 0 {
            return;
        }

        let mut wav_file = Vec::new();

        let num_samples = self.tmp.len() / 2;
        let file_size = (num_samples * 4 + 36) as u32;

        wav_file.write("RIFF".as_bytes()).unwrap();
        wav_file.write(&[((file_size >> 0) & 0xFF) as u8]).unwrap();
        wav_file.write(&[((file_size >> 8) & 0xFF) as u8]).unwrap();
        wav_file.write(&[((file_size >> 16) & 0xFF) as u8]).unwrap();
        wav_file.write(&[((file_size >> 24) & 0xFF) as u8]).unwrap();


        wav_file.write("WAVE".as_bytes()).unwrap();
        wav_file.write("fmt ".as_bytes()).unwrap();

        // format size
        wav_file.write(&[0x10]).unwrap();
        wav_file.write(&[0]).unwrap();
        wav_file.write(&[0]).unwrap();
        wav_file.write(&[0]).unwrap();

        // format PCM
        wav_file.write(&[1]).unwrap();
        wav_file.write(&[0]).unwrap();

        // Num Channels
        wav_file.write(&[2]).unwrap();
        wav_file.write(&[0]).unwrap();

        // rate=44100
        let y = 44100;
        let x = 176400;
        wav_file.write(&[0x44]).unwrap();
        wav_file.write(&[0xAC]).unwrap();
        wav_file.write(&[0]).unwrap();
        wav_file.write(&[0]).unwrap();

        // rate*2*2
        wav_file.write(&[0x10]).unwrap();
        wav_file.write(&[0xB1]).unwrap();
        wav_file.write(&[0x2]).unwrap();
        wav_file.write(&[0]).unwrap();

        // bytes per sample
        wav_file.write(&[4]).unwrap();
        wav_file.write(&[0]).unwrap();

        // bits per sample
        wav_file.write(&[0x10]).unwrap();
        wav_file.write(&[0]).unwrap();

        wav_file.write("data".as_bytes()).unwrap();
        let data_size = num_samples * 4;
        wav_file.write(&[((data_size >> 0) & 0xFF) as u8]).unwrap();
        wav_file.write(&[((data_size >> 8) & 0xFF) as u8]).unwrap();
        wav_file.write(&[((data_size >> 16) & 0xFF) as u8]).unwrap();
        wav_file.write(&[((data_size >> 24) & 0xFF) as u8]).unwrap();

        for s in &self.tmp {
            wav_file.write(&[(*s & 0xFF) as u8]).unwrap();
            wav_file.write(&[(*s >> 8) as u8]).unwrap();
        }

        println!("l={} s={}", wav_file.len(), self.tmp.len());
        // fs::write("dev/test.wav", wav_file).unwrap();

    }

    pub fn step(&mut self, cycles: u8) {
        for _c in 0..(cycles * 4) {
            self.frame_sequencer();

            self.channel3.step();

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

        let sequencer_step = self.frame_sequencer & 0x7;

        match sequencer_step {
            2 | 6 => {
                self.channel3.get_length_counter().step();
                // @todo Sweep
            }
            0 | 4 => {
                self.channel3.get_length_counter().step();
            }
            7 => {
                // @todo Volume envelopes
            }
            1 | 3 | 5 => {}
            _ => unreachable!(),
        }

        if self.frame_sequencer == 0 {
            self.frame_sequencer = FRAME_SEQUENCER_START;
        }
    }

    pub fn sample_audio(&mut self) {
        self.sample_counter -= 1;

        if self.sample_counter > 0 {
            return;
        }

        self.sample_counter = SAMPLE_COUNTER_START;

        let chan3_sample = self.channel3.get_sample();

        let mut left_scaled = 0.0;
        let mut right_scaled = 0.0;

        for i in 0..4 {
            let channel_sample = if i == 2 {
                f32::from(chan3_sample)
            } else {
                0.0
            };
            
            if self.left_pan[i] {
                left_scaled += channel_sample * APU::get_volume_scale(self.left_vol);
            }
            if self.right_pan[i] {
                right_scaled += channel_sample * APU::get_volume_scale(self.right_vol);
            }
        }

        let left: u8 = left_scaled as u8;
        let right: u8 = right_scaled as u8;

        // Send to SDL land
        // println!("chan3: {}", chan3_sample);
        if let Some(sound_chan) = &self.sound_chan {
            sound_chan.send((left, right)).unwrap();
        }

        // let mut left_i: i16 = 0;
        // let mut right_i: i16 = 0;

        // left_i += left as i8 as i16;
        // right_i += right as i8 as i16;

        // let left_cvt = (left_i-32) << 10;
        // let right_cvt = (right_i-32) << 10;
        // self.tmp.push(left_cvt);
        // self.tmp.push(right_cvt);
    }

    pub fn get_channel3(&mut self) -> &mut Channel3 {
        &mut self.channel3
    }

    pub fn write_nr50(&mut self, data: u8) {
        self.right_vol = data & 0x7;
        self.left_vol = (data >> 4) & 0x7;
        // @todo VIN left/right
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
}
