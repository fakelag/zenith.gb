use std::sync::mpsc::Sender;

use super::channel3::Channel3;

pub type ApuSample = (u8, u8);

pub struct APU {
    channel3: Channel3,
    sample_counter: u16,
    sound_chan: Option<Sender<ApuSample>>,
}

impl APU {
    pub fn new(sound_chan: Option<Sender<ApuSample>>) -> Self {
        Self {
            channel3: Channel3::new(),
            sample_counter: 95,
            sound_chan,
        }
    }

    pub fn step(&mut self, cycles: u8) {
        for _c in 0..=cycles {
            self.channel3.step();

            self.sample_counter -= 1;

            if self.sample_counter > 0 {
                continue;
            }
            self.sample_counter = 95;

            let chan3_sample = self.channel3.get_last_sample();

            let left = chan3_sample;
            let right = chan3_sample;

            // Send to SDL land
            // println!("chan3: {}", chan3_sample);
            if let Some(sound_chan) = &self.sound_chan {
                sound_chan.send((left, right)).unwrap();
            }
        }
    }

    pub fn get_channel3(&mut self) -> &mut Channel3 {
        &mut self.channel3
    }
}
