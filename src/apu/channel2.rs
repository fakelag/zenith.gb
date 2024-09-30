use super::{lengthcounter::LengthCounter, Channel};

const LENGTH_COUNTER_INIT: u16 = 64;

pub struct Channel2 {
    length_counter: LengthCounter,
}

impl Channel2 {
    pub fn new() -> Self {
        Self {
            length_counter: LengthCounter::new(LENGTH_COUNTER_INIT),
        }
    }

    fn trigger(&mut self) {
        todo!();
    }

    pub fn write_nr20(&mut self, data: u8) {
        todo!();
    }

    pub fn write_nr21(&mut self, data: u8) {
        todo!();
    }

    pub fn write_nr22(&mut self, data: u8) {
        todo!();
    }

    pub fn write_nr23(&mut self, data: u8) {
        todo!();
    }

    pub fn write_nr24(&mut self, data: u8) {
        todo!();
    }

    pub fn read_nr20(&mut self) -> u8 {
        todo!();
    }

    pub fn read_nr21(&mut self) -> u8 {
        todo!();
    }

    pub fn read_nr22(&mut self) -> u8 {
        todo!();
    }

    pub fn read_nr23(&mut self) -> u8 {
        todo!();
    }

    pub fn read_nr24(&mut self) -> u8 {
        todo!();
    }
}

impl Channel for Channel2 {
    fn step(&mut self) {
    }

    fn get_sample(&self) -> u8 {
        0
    }

    fn get_length_counter(&mut self) -> &mut LengthCounter {
        &mut self.length_counter
    }

    fn get_envelope(&mut self) -> Option<&mut super::envelope::Envelope> {
        None
    }

    fn is_enabled(&self) -> bool {
        todo!();
    }
}
